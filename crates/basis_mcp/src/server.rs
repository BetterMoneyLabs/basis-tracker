//! MCP server exposing the Basis wallet as tools over stdio.
//!
//! Wraps the typed command cores from `basis_cli_lib::commands::*` (the same
//! functions backing `basis-cli --json`). Private keys never leave the process:
//! there is deliberately no key-export tool, and signing happens in-process.
//!
//! Authentication credentials for the tracker server are read from environment
//! variables (`BASIS_TRACKER_AUTH_MODE`, `BASIS_TRACKER_API_KEY`,
//! `BASIS_TRACKER_AUTH_PUBKEY`, `BASIS_TRACKER_AUTH_SECRET_KEY`) so deployments
//! can inject secrets without modifying the CLI config file. See
//! `specs/server/authentication_authorization.md` for the supported modes.

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;

use basis_cli_lib::account::AccountManager;
use basis_cli_lib::api::{TrackerAuth, TrackerClient, UploadPolicyRequest};
use basis_cli_lib::commands;
use basis_cli_lib::config::ConfigManager;
use basis_core::acceptance::AcceptanceConfig;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::policy::UiConfigManager;

/// Shared wallet state, initialized once at startup.
pub struct AppState {
    pub account_manager: AccountManager,
    pub client: TrackerClient,
    pub ui_config: UiConfigManager,
}

/// Basis wallet MCP server.
#[derive(Clone)]
pub struct BasisMcp {
    state: Arc<Mutex<AppState>>,
    tool_router: ToolRouter<Self>,
}

/// Account view returned by `account_list` (never includes key material).
#[derive(Debug, Serialize)]
struct AccountView {
    name: String,
    pubkey_hex: String,
    created_at: u64,
    current: bool,
}

/// Result of `policy_set`.
#[derive(Debug, Serialize)]
struct PolicySetResult {
    saved: bool,
    uploaded: bool,
    policy_hash: String,
    uploaded_at: u64,
}

/// Render an operation result as MCP tool content: JSON text on success,
/// an isError content item carrying the full error chain on failure.
fn json_result<T: Serialize>(result: Result<T>) -> Result<CallToolResult, McpError> {
    match result {
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "failed to serialize result: {}",
                e
            ))])),
        },
        Err(e) => Ok(CallToolResult::error(vec![Content::text(error_chain(&e))])),
    }
}

fn tool_error(message: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(message.into())]))
}

/// Flatten an error and its sources into a single message.
fn error_chain(err: &anyhow::Error) -> String {
    err.chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

/// Serialize the policy, sign it with the account, and upload it to the server.
/// Mirrors `save_and_upload_policy` in crates/basis_app/src/ui.rs.
async fn upload_policy(
    client: &TrackerClient,
    account: &basis_cli_lib::account::Account,
    policy: &AcceptanceConfig,
) -> Result<basis_cli_lib::api::UploadPolicyResponse> {
    let policy_json = serde_json::to_string(policy)?;
    let signature = account.sign_message(policy_json.as_bytes())?;
    let request = UploadPolicyRequest {
        recipient_pubkey: account.get_pubkey_hex(),
        policy_json,
        signature: hex::encode(signature),
    };
    client.upload_policy(request).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServerStatusParams {
    /// Tracker server URL override; defaults to the configured server URL.
    #[serde(default)]
    pub server_url: Option<String>,
}

/// Which side of a note the given account is on.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NoteDirection {
    /// Notes issued by the account (it is the payer).
    Issued,
    /// Notes received by the account (it is the payee).
    #[default]
    Received,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteListParams {
    /// Account public key (hex); defaults to the current account.
    #[serde(default)]
    pub pubkey: Option<String>,
    /// "issued" or "received" (default: "received").
    #[serde(default)]
    pub direction: NoteDirection,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteGetParams {
    /// Issuer public key (hex).
    pub issuer: String,
    /// Recipient public key (hex).
    pub recipient: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReserveStatusParams {
    /// Issuer public key (hex); defaults to the current account.
    #[serde(default)]
    pub pubkey: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AccountNameParams {
    /// Account name.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AccountImportParams {
    /// Account name.
    pub name: String,
    /// Private key in hex format (32 bytes). Stored in the local config; never returned.
    pub private_key_hex: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteCreateParams {
    /// Recipient public key (hex).
    pub recipient: String,
    /// Amount in nanoERG.
    pub amount: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteRedeemParams {
    /// Issuer public key (hex).
    pub issuer: String,
    /// Amount to redeem in nanoERG.
    pub amount: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReserveCreateParams {
    /// Reserve NFT ID (hex-encoded, 64 chars) - identifies this reserve instance.
    pub nft_id: String,
    /// Amount of ERG to put into the reserve (in nanoERG).
    pub amount: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolicySetParams {
    /// Acceptance policy as a JSON object matching basis_core AcceptanceConfig.
    pub policy: serde_json::Value,
}

impl BasisMcp {
    /// Initialize shared wallet state from the default config (~/.basis).
    pub fn new(server_url: Option<String>) -> Result<Self> {
        let config_manager = ConfigManager::new(None)?;
        let account_manager = AccountManager::new(config_manager.clone())?;
        let server_url =
            server_url.unwrap_or_else(|| config_manager.get_config().server_url.clone());
        let auth = tracker_auth_from_env_or_config(config_manager.get_config());
        let client = TrackerClient::with_auth(server_url, auth);
        let ui_config = UiConfigManager::new()?;

        Ok(Self {
            state: Arc::new(Mutex::new(AppState {
                account_manager,
                client,
                ui_config,
            })),
            tool_router: Self::tool_router(),
        })
    }
}

/// Load tracker authentication credentials from environment variables, falling
/// back to `~/.basis/cli.toml`.
///
/// Environment variables take precedence. When they are absent or empty, the
/// corresponding fields from `CliConfig` are used.
///
/// - `BASIS_TRACKER_AUTH_MODE`: `none` (default), `api_key`, or `signature`.
/// - `BASIS_TRACKER_API_KEY`: shared secret when mode is `api_key`.
/// - `BASIS_TRACKER_AUTH_PUBKEY`: hex public key when mode is `signature`.
/// - `BASIS_TRACKER_AUTH_SECRET_KEY`: hex secret key when mode is `signature`.
fn tracker_auth_from_env_or_config(config: &basis_cli_lib::config::CliConfig) -> TrackerAuth {
    let mode = std::env::var("BASIS_TRACKER_AUTH_MODE")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| config.server_auth_mode.clone())
        .unwrap_or_else(|| "none".to_string())
        .to_lowercase();

    match mode.as_str() {
        "api_key" => {
            let key = std::env::var("BASIS_TRACKER_API_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| config.server_api_key.clone());
            match key {
                Some(key) => TrackerAuth::api_key(key),
                None => {
                    tracing::warn!(
                        "BASIS_TRACKER_AUTH_MODE=api_key but BASIS_TRACKER_API_KEY and \
                         server_api_key are both empty"
                    );
                    TrackerAuth::None
                }
            }
        }
        "signature" => {
            let pubkey = std::env::var("BASIS_TRACKER_AUTH_PUBKEY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| config.server_auth_pubkey.clone());
            let secret = std::env::var("BASIS_TRACKER_AUTH_SECRET_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| config.server_auth_secret_key.clone());
            match (pubkey, secret) {
                (Some(pubkey), Some(secret)) if !pubkey.is_empty() && !secret.is_empty() => {
                    TrackerAuth::signature(pubkey, secret)
                }
                _ => {
                    tracing::warn!(
                        "BASIS_TRACKER_AUTH_MODE=signature but pubkey/secret env vars and \
                         cli.toml fields are missing"
                    );
                    TrackerAuth::None
                }
            }
        }
        _ => TrackerAuth::None,
    }
}

#[tool_router]
impl BasisMcp {
    /// Get tracker server health and recent events.
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn server_status(
        &self,
        Parameters(params): Parameters<ServerStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        let result = match &params.server_url {
            Some(url) => {
                commands::status::get_server_status(&TrackerClient::new(url.clone())).await
            }
            None => commands::status::get_server_status(&state.client).await,
        };
        json_result(result)
    }

    /// List all wallet accounts (name, pubkey, created_at; never private keys).
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn account_list(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        let current_name = state.account_manager.get_current().map(|a| a.name.clone());
        let accounts: Vec<AccountView> = state
            .account_manager
            .accounts
            .values()
            .map(|a| AccountView {
                current: current_name.as_deref() == Some(a.name.as_str()),
                name: a.name.clone(),
                pubkey_hex: a.get_pubkey_hex(),
                created_at: a.created_at,
            })
            .collect();
        json_result(Ok(accounts))
    }

    /// Get the current account name and public key (null if none is selected).
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn account_current(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(Ok(commands::account::current_account_info(
            &state.account_manager,
        )))
    }

    /// List notes for an account (issued or received).
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn note_list(
        &self,
        Parameters(params): Parameters<NoteListParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        let pubkey = match params.pubkey {
            Some(pubkey) => pubkey,
            None => match state.account_manager.get_current_pubkey_hex() {
                Some(pubkey) => pubkey,
                None => {
                    return tool_error("No current account selected and no pubkey given");
                }
            },
        };
        let notes = match params.direction {
            NoteDirection::Issued => state.client.get_issuer_notes(&pubkey).await,
            NoteDirection::Received => state.client.get_recipient_notes(&pubkey).await,
        };
        let result = notes.map(|notes| {
            notes
                .iter()
                .map(|note| commands::note::NoteListEntry {
                    issuer_pubkey: note.issuer_pubkey.clone(),
                    recipient_pubkey: note.recipient_pubkey.clone(),
                    amount: note.amount_collected,
                    redeemed: note.amount_redeemed,
                    outstanding: note.outstanding_debt(),
                    timestamp: note.timestamp,
                })
                .collect::<Vec<_>>()
        });
        json_result(result)
    }

    /// Get a specific note by issuer and recipient public keys.
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn note_get(
        &self,
        Parameters(params): Parameters<NoteGetParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(
            commands::note::get_note(&state.client, &params.issuer, &params.recipient).await,
        )
    }

    /// Get reserve status for an issuer (defaults to the current account).
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn reserve_status(
        &self,
        Parameters(params): Parameters<ReserveStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(
            commands::reserve::get_reserve_status(
                &state.account_manager,
                &state.client,
                params.pubkey,
            )
            .await,
        )
    }

    /// Get the local acceptance policy from ~/.basis/ui.toml.
    #[tool(annotations(read_only_hint = true, destructive_hint = false))]
    async fn policy_get(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(Ok(state.ui_config.get_acceptance().clone()))
    }

    /// Create a new account and persist it to the local config.
    #[tool(annotations(read_only_hint = false, destructive_hint = false))]
    async fn account_create(
        &self,
        Parameters(params): Parameters<AccountNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().await;
        json_result(commands::account::create_account(
            &mut state.account_manager,
            &params.name,
        ))
    }

    /// Switch the current account.
    #[tool(annotations(read_only_hint = false, destructive_hint = false))]
    async fn account_switch(
        &self,
        Parameters(params): Parameters<AccountNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().await;
        json_result(commands::account::switch_account(
            &mut state.account_manager,
            &params.name,
        ))
    }

    /// Import an account from a hex-encoded private key. The key is stored in
    /// the local config and is never returned by any tool.
    #[tool(annotations(read_only_hint = false, destructive_hint = false))]
    async fn account_import(
        &self,
        Parameters(params): Parameters<AccountImportParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().await;
        json_result(commands::account::import_account(
            &mut state.account_manager,
            &params.name,
            &params.private_key_hex,
        ))
    }

    /// Create a debt note to a recipient, signed with the current account.
    #[tool(annotations(read_only_hint = false, destructive_hint = false))]
    async fn note_create(
        &self,
        Parameters(params): Parameters<NoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(
            commands::note::create_normal_note(
                &state.account_manager,
                &state.client,
                &params.recipient,
                params.amount,
            )
            .await,
        )
    }

    /// Redeem a note (local-signing path; the current account is the recipient).
    #[tool(annotations(read_only_hint = false, destructive_hint = true))]
    async fn note_redeem(
        &self,
        Parameters(params): Parameters<NoteRedeemParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(
            commands::note::redeem_note(
                &state.account_manager,
                &state.client,
                &params.issuer,
                params.amount,
                false,
            )
            .await,
        )
    }

    /// Build a reserve-creation payload (owner = current account) via the tracker.
    #[tool(annotations(read_only_hint = false, destructive_hint = false))]
    async fn reserve_create(
        &self,
        Parameters(params): Parameters<ReserveCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().await;
        json_result(
            commands::reserve::create_reserve(
                &state.account_manager,
                &state.client,
                params.nft_id,
                None,
                params.amount,
                None,
                None,
                false,
            )
            .await,
        )
    }

    /// Replace the local acceptance policy: saves to ~/.basis/ui.toml and
    /// uploads it to the server, signed with the current account.
    #[tool(annotations(read_only_hint = false, destructive_hint = true))]
    async fn policy_set(
        &self,
        Parameters(params): Parameters<PolicySetParams>,
    ) -> Result<CallToolResult, McpError> {
        let policy: AcceptanceConfig = match serde_json::from_value(params.policy) {
            Ok(policy) => policy,
            Err(e) => return tool_error(format!("invalid policy JSON: {}", e)),
        };

        let mut state = self.state.lock().await;

        // 1. Save to the local ui.toml
        if let Err(e) = state.ui_config.update_acceptance(policy.clone()) {
            return tool_error(error_chain(&e));
        }

        // 2. Upload to the server, signed with the current account
        //    (mirrors save_and_upload_policy in crates/basis_app/src/ui.rs)
        let account = match state.account_manager.get_current() {
            Some(account) => account,
            None => {
                return tool_error(
                    "policy saved locally, but no current account selected for upload",
                );
            }
        };

        let upload = upload_policy(&state.client, account, &policy).await;

        match upload {
            Ok(response) => json_result(Ok(PolicySetResult {
                saved: true,
                uploaded: true,
                policy_hash: response.policy_hash,
                uploaded_at: response.uploaded_at,
            })),
            Err(e) => tool_error(format!(
                "policy saved locally but upload failed: {}",
                error_chain(&e)
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for BasisMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "basis-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some(
                "Basis wallet MCP server: query and manage Basis tracker accounts, notes, \
                 reserves, and the acceptance policy. Signing happens in-process; private \
                 keys are never exposed through any tool."
                    .to_string(),
            ),
        }
    }
}
