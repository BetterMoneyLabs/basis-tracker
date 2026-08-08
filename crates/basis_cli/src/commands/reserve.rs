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

        /// Retired compatibility flag; tracker-side wallet submission is rejected
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
    /// Payload to submit to an Ergo wallet to create the reserve on-chain.
    pub payload: ReserveCreationResponse,
    /// Transaction id if the payload was submitted via the tracker node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
}

/// Collateralization ratio and classification for an issuer.
#[derive(Debug, Serialize)]
pub struct CollateralizationResult {
    pub issuer_pubkey: String,
    pub ratio: f64,
    pub status: String,
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
    submit: bool,
) -> Result<ReserveCreateResult> {
    if submit {
        anyhow::bail!(
            "Tracker-side reserve submission is retired; generate the payload and sign it with the reserve owner's wallet"
        );
    }

    // Get the owner public key from either the command line argument or current account
    let owner_pubkey = resolve_pubkey(account_manager, owner, "owner")?;

    // Validate that the public key is 66 hex characters (33 bytes)
    if owner_pubkey.len() != 66 {
        return Err(anyhow::anyhow!(
            "Owner public key must be 33 bytes (66 hex characters), got {} characters",
            owner_pubkey.len()
        ));
    }

    progress!("Creating reserve with:");
    progress!("  NFT ID: {}", nft_id);
    progress!("  Owner: {}", owner_pubkey);
    progress!("  Amount: {} nanoERG", amount);

    // Create the reserve creation request
    let request = CreateReserveRequest {
        nft_id: nft_id.clone(),
        owner_pubkey: owner_pubkey.clone(),
        erg_amount: amount,
    };

    // Call the API to create the reserve payload
    let payload = client.create_reserve(request).await?;

    let tx_id = None;

    Ok(ReserveCreateResult {
        nft_id,
        owner_pubkey,
        amount,
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
        ratio: status.collateralization_ratio,
        status: get_collateralization_status(status.collateralization_ratio).to_string(),
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
            submit,
        } => {
            let result =
                create_reserve(account_manager, client, nft_id, owner, amount, submit).await?;
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
                println!("  Total Debt: {} nanoERG", status.total_debt);
                println!("  Collateral: {} nanoERG", status.collateral);
                println!(
                    "  Collateralization Ratio: {:.2}",
                    status.collateralization_ratio
                );
                println!("  Note Count: {}", status.note_count);
                println!("  Last Updated: {}", status.last_updated);

                // Calculate ERG values
                let debt_erg = status.total_debt as f64 / 1_000_000_000.0;
                let collateral_erg = status.collateral as f64 / 1_000_000_000.0;

                println!("\nIn ERG:");
                println!("  Total Debt: {:.6} ERG", debt_erg);
                println!("  Collateral: {:.6} ERG", collateral_erg);
            }
        }
        ReserveCommands::Collateralization { issuer } => {
            let result = get_collateralization(account_manager, client, issuer).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Collateralization for {}:", result.issuer_pubkey);
                println!("  Ratio: {:.4}", result.ratio);
                println!("  Status: {}", result.status);

                if result.ratio < 1.0 {
                    println!("⚠️  WARNING: Under-collateralized!");
                } else if result.ratio < 1.5 {
                    println!("⚠️  WARNING: Low collateralization");
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
