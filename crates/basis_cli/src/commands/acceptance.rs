use crate::account::AccountManager;
use crate::api::TrackerClient;
use anyhow::{Context, Result};
use basis_core::acceptance::AcceptanceConfig;
use clap::Subcommand;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AcceptanceCommands {
    /// Upload a signed acceptance policy to the tracker server
    Upload {
        /// Path to a TOML file containing the acceptance policy
        #[arg(long)]
        policy_file: PathBuf,
    },
    /// Check whether a note would be accepted by a recipient's policy
    Check {
        /// Issuer public key (33-byte hex)
        #[arg(long)]
        issuer: String,
        /// Recipient public key (33-byte hex)
        #[arg(long)]
        recipient: String,
        /// Total cumulative debt in nanoERG (or raw token units with --denomination)
        #[arg(long)]
        total_debt: u64,
        /// Denomination of the note ("erg" or "token:<64-hex>"); omitted resolves to
        /// the pair's existing note denomination, else ERG
        #[arg(long)]
        denomination: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct AcceptanceUploadResult {
    pub uploaded: bool,
    pub policy_hash: String,
}

#[derive(Debug, Serialize)]
pub struct AcceptanceCheckResult {
    pub acceptable: bool,
    pub reason: Option<String>,
}

pub async fn handle_acceptance_command(
    cmd: AcceptanceCommands,
    account_manager: &AccountManager,
    client: &TrackerClient,
    json: bool,
) -> Result<()> {
    match cmd {
        AcceptanceCommands::Upload { policy_file } => {
            let policy_toml = std::fs::read_to_string(&policy_file)
                .with_context(|| format!("Failed to read policy file: {:?}", policy_file))?;

            let config = AcceptanceConfig::from_toml(&policy_toml)
                .with_context(|| "Invalid acceptance policy TOML")?;

            let policy_json = serde_json::to_string(&config)
                .with_context(|| "Failed to serialize policy to JSON")?;

            let account = account_manager
                .get_current()
                .context("No current account selected; use 'basis-cli account switch <name>'")?;

            let signature = account
                .sign_message(policy_json.as_bytes())
                .context("Failed to sign policy with current account")?;

            let response = client
                .upload_policy(crate::api::UploadPolicyRequest {
                    recipient_pubkey: account.get_pubkey_hex(),
                    policy_json,
                    signature: hex::encode(signature),
                })
                .await
                .context("Policy upload request failed")?;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&AcceptanceUploadResult {
                        uploaded: true,
                        policy_hash: response.policy_hash,
                    })?
                );
            } else {
                println!("✅ Acceptance policy uploaded successfully");
                println!("  Policy hash: {}", response.policy_hash);
                println!("  Recipient: {}", account.get_pubkey_hex());
            }
        }
        AcceptanceCommands::Check {
            issuer,
            recipient,
            total_debt,
            denomination,
        } => {
            let response = client
                .check_acceptance(&issuer, total_debt, Some(&recipient), denomination.as_deref())
                .await
                .context("Acceptance check request failed")?;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&AcceptanceCheckResult {
                        acceptable: response.acceptable,
                        reason: response.reason,
                    })?
                );
            } else if response.acceptable {
                println!("✅ Note would be accepted");
            } else {
                println!("❌ Note would be rejected");
                if let Some(reason) = response.reason {
                    println!("  Reason: {}", reason);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AcceptanceCommands,
    }

    #[test]
    fn check_parses_denomination_flag() {
        // N18: the acceptance check must be able to scope the policy
        // evaluation to a token denomination.
        let cli = TestCli::try_parse_from([
            "test",
            "check",
            "--issuer",
            &"02".repeat(33),
            "--recipient",
            &"03".repeat(33),
            "--total-debt",
            "1000",
            "--denomination",
            &format!("token:{}", "ab".repeat(32)),
        ])
        .expect("check with --denomination parses");
        match cli.cmd {
            AcceptanceCommands::Check { denomination, .. } => {
                assert_eq!(denomination, Some(format!("token:{}", "ab".repeat(32))));
            }
            _ => panic!("expected Check"),
        }

        // Omitted stays None (server resolves the pair's denomination).
        let cli = TestCli::try_parse_from([
            "test",
            "check",
            "--issuer",
            &"02".repeat(33),
            "--recipient",
            &"03".repeat(33),
            "--total-debt",
            "1000",
        ])
        .expect("check without --denomination parses");
        match cli.cmd {
            AcceptanceCommands::Check { denomination, .. } => {
                assert_eq!(denomination, None);
            }
            _ => panic!("expected Check"),
        }
    }
}
