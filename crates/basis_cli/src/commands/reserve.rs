use crate::account::AccountManager;
use crate::api::{CreateReserveRequest, KeyStatusResponse, ReserveCreationResponse, TrackerClient};
use crate::output::progress;
use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;

#[derive(Subcommand)]
pub enum ReserveCommands {
    /// Create a new reserve
    Create {
        /// Reserve NFT ID (hex-encoded, 64 chars) - identifies this reserve instance
        #[arg(long)]
        nft_id: String,

        /// Owner public key (hex-encoded, 33 bytes)
        #[arg(long)]
        owner: Option<String>,

        /// Amount of ERG to put into the reserve (in nanoERG)
        #[arg(long)]
        amount: u64,

        /// Amount of the reserve token to lock (raw token units). Requires --token-id.
        #[arg(long)]
        token_amount: Option<u64>,

        /// Reserve token ID (hex-encoded, 64 chars). When set, creates a token-backed reserve.
        #[arg(long)]
        token_id: Option<String>,

        /// Submit the generated payload to the configured Ergo node (`node_url` in cli.toml);
        /// the node's wallet must be the reserve owner's wallet — the tracker never pays.
        #[arg(long)]
        submit: bool,
    },
    /// Get reserve status for an issuer
    Status {
        /// Issuer public key (hex)
        #[arg(long)]
        issuer: Option<String>,
    },
    /// Get collateralization ratio
    Collateralization {
        /// Issuer public key (hex)
        #[arg(long)]
        issuer: Option<String>,
    },
}

/// Result of building a reserve-creation payload (`reserve create`).
#[derive(Debug, Serialize)]
pub struct ReserveCreateResult {
    pub nft_id: String,
    pub owner_pubkey: String,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_amount: Option<u64>,
    /// Payload to submit to an Ergo wallet to create the reserve on-chain.
    pub payload: ReserveCreationResponse,
    /// Transaction id if the payload was submitted via the configured Ergo node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
}

/// Collateralization ratio and classification for an issuer.
#[derive(Debug, serde::Serialize)]
pub struct CollateralizationDenominationResult {
    pub denomination: String,
    pub symbol: String,
    pub ratio: f64,
    pub status: String,
}

/// Collateralization overview for an issuer, per denomination.
#[derive(Debug, serde::Serialize)]
pub struct CollateralizationResult {
    pub issuer_pubkey: String,
    /// Per-denomination ratios; never aggregated across units.
    pub denominations: Vec<CollateralizationDenominationResult>,
}

/// Resolve the issuer/owner public key from an optional CLI argument or the current account.
fn resolve_pubkey(
    account_manager: &AccountManager,
    given: Option<String>,
    label: &str,
) -> Result<String> {
    if let Some(key) = given {
        Ok(key)
    } else {
        account_manager.get_current_pubkey_hex().ok_or_else(|| {
            anyhow::anyhow!("No current account selected and no {} specified", label)
        })
    }
}

/// Build the reserve creation payload via the tracker server.
pub async fn create_reserve(
    account_manager: &AccountManager,
    client: &TrackerClient,
    nft_id: String,
    owner: Option<String>,
    amount: u64,
    token_amount: Option<u64>,
    token_id: Option<String>,
    submit: bool,
) -> Result<ReserveCreateResult> {
    // Get the owner public key from either the command line argument or current account
    let owner_pubkey = resolve_pubkey(account_manager, owner, "owner")?;

    // Validate that the public key is 66 hex characters (33 bytes)
    if owner_pubkey.len() != 66 {
        return Err(anyhow::anyhow!(
            "Owner public key must be 33 bytes (66 hex characters), got {} characters",
            owner_pubkey.len()
        ));
    }

    let token_amount = token_amount.unwrap_or(0);
    let token_id = token_id.unwrap_or_default();

    progress!("Creating reserve with:");
    progress!("  NFT ID: {}", nft_id);
    progress!("  Owner: {}", owner_pubkey);
    progress!("  ERG value: {} nanoERG", amount);
    if !token_id.is_empty() {
        progress!("  Token ID: {}", token_id);
        progress!("  Token amount: {}", token_amount);
    }

    // Create the reserve creation request
    let request = CreateReserveRequest {
        nft_id: nft_id.clone(),
        owner_pubkey: owner_pubkey.clone(),
        erg_amount: amount,
        token_amount,
        token_id: token_id.clone(),
    };

    // Call the API to create the reserve payload
    let payload = client.create_reserve(request.clone()).await?;
    // The tracker only builds the payload and whoever submits it pays what it says, so it must be
    // the reserve requested above before it is shown, returned or submitted.
    crate::verify::verify_reserve_payload(&request, &payload, None)?;

    let tx_id = if submit {
        let cfg = account_manager.config_manager.get_config();
        let node_url = cfg.node_url.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "--submit requires an Ergo node: set node_url (and node_api_key) in cli.toml, \
                 or submit the returned payload to your node's /wallet/payment/send yourself"
            )
        })?;
        Some(crate::api::submit_reserve_to_node(
            node_url,
            cfg.node_api_key.as_deref(),
            &request,
            &payload,
        )?)
    } else {
        None
    };

    Ok(ReserveCreateResult {
        nft_id,
        owner_pubkey,
        amount,
        token_id: if token_id.is_empty() {
            None
        } else {
            Some(token_id)
        },
        token_amount: if token_amount == 0 {
            None
        } else {
            Some(token_amount)
        },
        payload,
        tx_id,
    })
}

/// Get the reserve status for an issuer (or the current account).
pub async fn get_reserve_status(
    account_manager: &AccountManager,
    client: &TrackerClient,
    issuer: Option<String>,
) -> Result<KeyStatusResponse> {
    let pubkey = resolve_pubkey(account_manager, issuer, "issuer")?;
    client.get_reserve_status(&pubkey).await
}

/// Get the collateralization ratio and classification for an issuer (or the current account).
pub async fn get_collateralization(
    account_manager: &AccountManager,
    client: &TrackerClient,
    issuer: Option<String>,
) -> Result<CollateralizationResult> {
    let status = get_reserve_status(account_manager, client, issuer).await?;
    Ok(CollateralizationResult {
        issuer_pubkey: status.issuer_pubkey.clone(),
        denominations: status
            .denominations
            .iter()
            .map(|d| CollateralizationDenominationResult {
                denomination: d.denomination.clone(),
                symbol: d.symbol.clone(),
                ratio: d.collateralization_ratio,
                status: get_collateralization_status(d.collateralization_ratio).to_string(),
            })
            .collect(),
    })
}

pub async fn handle_reserve_command(
    cmd: ReserveCommands,
    account_manager: &AccountManager,
    client: &TrackerClient,
    json: bool,
) -> Result<()> {
    match cmd {
        ReserveCommands::Create {
            nft_id,
            owner,
            amount,
            token_amount,
            token_id,
            submit,
        } => {
            let result = create_reserve(
                account_manager,
                client,
                nft_id,
                owner,
                amount,
                token_amount,
                token_id,
                submit,
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let response = &result.payload;
                println!("\n✅ Reserve creation payload created successfully!");
                if let Some(ref tx_id) = result.tx_id {
                    println!("Transaction submitted: {}", tx_id);
                }
                println!("The following payload can be used with the Ergo wallet API:");
                println!();

                // Print the response in a readable format
                println!("Requests:");
                for (i, req) in response.requests.iter().enumerate() {
                    println!("  Request {}: {{", i + 1);
                    println!("    address: \"{}\"", req.address);
                    println!("    value: {}", req.value);
                    println!("    assets: [");
                    for asset in &req.assets {
                        println!(
                            "      {{ token_id: \"{}\", amount: {} }},",
                            asset.token_id, asset.amount
                        );
                    }
                    println!("    ]");
                    println!("    registers: {{");
                    for (key, value) in &req.registers {
                        println!("      \"{}\": \"{}\",", key, value);
                    }
                    println!("    }}");
                    println!("  }}");
                }
                println!();
                println!("Fee: {} nanoERG", response.fee);
                println!("Change address: {}", response.change_address);

                if result.tx_id.is_none() {
                    println!();
                    println!(
                        "💡 To create the reserve, submit this payload to your Ergo wallet using:"
                    );
                    println!("   curl -X POST http://your-ergo-node:9053/wallet/payment/send \\");
                    println!("        -H \"Content-Type: application/json\" \\");
                    println!("        -H \"api_key: your-api-key\" \\");
                    println!("        -d '...' # (replace with the full payload above)");
                    println!();
                    println!("   Or re-run with --submit to broadcast via the tracker node.");
                }
            }
        }
        ReserveCommands::Status { issuer } => {
            let status = get_reserve_status(account_manager, client, issuer).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("Reserve Status for {}:", status.issuer_pubkey);
                if status.denominations.is_empty() {
                    println!("  No notes or reserves");
                }
                for d in &status.denominations {
                    let scale = 10f64.powi(d.decimals as i32);
                    println!("  [{}]", d.denomination);
                    println!(
                        "    Total Debt: {} raw ({:.prec$} {})",
                        d.total_debt,
                        d.total_debt as f64 / scale,
                        d.symbol,
                        prec = d.decimals as usize
                    );
                    println!(
                        "    Collateral: {} raw ({:.prec$} {})",
                        d.collateral,
                        d.collateral as f64 / scale,
                        d.symbol,
                        prec = d.decimals as usize
                    );
                    println!(
                        "    Collateralization Ratio: {:.2}",
                        d.collateralization_ratio
                    );
                    println!("    Note Count: {}", d.note_count);
                }
                println!("  Note Count (all denominations): {}", status.note_count);
                println!("  Last Updated: {}", status.last_updated);
            }
        }
        ReserveCommands::Collateralization { issuer } => {
            let result = get_collateralization(account_manager, client, issuer).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Collateralization for {}:", result.issuer_pubkey);
                if result.denominations.is_empty() {
                    println!("  No notes or reserves");
                }
                for d in &result.denominations {
                    println!("  [{}]", d.denomination);
                    println!("    Ratio: {:.4}", d.ratio);
                    println!("    Status: {}", d.status);

                    if d.ratio < 1.0 {
                        println!("    ⚠️  WARNING: Under-collateralized!");
                    } else if d.ratio < 1.5 {
                        println!("    ⚠️  WARNING: Low collateralization");
                    }
                }
            }
        }
    }

    Ok(())
}

fn get_collateralization_status(ratio: f64) -> &'static str {
    match ratio {
        r if r < 1.0 => "UNDER-COLLATERALIZED",
        r if r < 1.5 => "LOW",
        r if r < 2.0 => "ADEQUATE",
        r if r < 3.0 => "GOOD",
        _ => "EXCELLENT",
    }
}
