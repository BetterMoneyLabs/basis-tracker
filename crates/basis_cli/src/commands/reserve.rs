use crate::account::AccountManager;
use crate::api::{CreateReserveRequest, TrackerClient};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ReserveCommands {
    /// Create a new reserve
    Create {
        /// NFT ID for the tracker (hex-encoded)
        #[arg(long)]
        nft_id: String,

        /// Owner public key (hex-encoded, 33 bytes)
        #[arg(long)]
        owner: Option<String>,

        /// Amount of ERG to put into the reserve (in nanoERG)
        #[arg(long)]
        amount: u64,
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

pub async fn handle_reserve_command(
    cmd: ReserveCommands,
    account_manager: &AccountManager,
    client: &TrackerClient,
) -> Result<()> {
    match cmd {
        ReserveCommands::Create { nft_id, owner, amount } => {
            // Get the owner public key from either the command line argument or current account
            let owner_pubkey = if let Some(owner_key) = owner {
                owner_key
            } else {
                account_manager.get_current_pubkey_hex().ok_or_else(|| {
                    anyhow::anyhow!("No current account selected and no owner specified")
                })?
            };

            // Validate that the public key is 66 hex characters (33 bytes)
            if owner_pubkey.len() != 66 {
                return Err(anyhow::anyhow!("Owner public key must be 33 bytes (66 hex characters), got {} characters", owner_pubkey.len()));
            }

            println!("Creating reserve with:");
            println!("  NFT ID: {}", nft_id);
            println!("  Owner: {}", owner_pubkey);
            println!("  Amount: {} nanoERG", amount);

            // Create the reserve creation request
            let request = CreateReserveRequest {
                nft_id,
                owner_pubkey,
                erg_amount: amount,
            };

            // Call the API to create the reserve payload
            let response = client.create_reserve(request).await?;

            println!("\n✅ Reserve creation payload created successfully!");
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
                    println!("      {{ token_id: \"{}\", amount: {} }},", asset.token_id, asset.amount);
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

            println!();
            println!("💡 To create the reserve, submit this payload to your Ergo wallet using:");
            println!("   curl -X POST http://your-ergo-node:9053/wallet/payment/send \\");
            println!("        -H \"Content-Type: application/json\" \\");
            println!("        -H \"api_key: your-api-key\" \\");
            println!("        -d '...' # (replace with the full payload above)");
        }
        ReserveCommands::Status { issuer } => {
            let pubkey = if let Some(issuer) = issuer {
                issuer
            } else {
                account_manager.get_current_pubkey_hex().ok_or_else(|| {
                    anyhow::anyhow!("No current account selected and no issuer specified")
                })?
            };

            let status = client.get_reserve_status(&pubkey).await?;

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
        ReserveCommands::Collateralization { issuer } => {
            let pubkey = if let Some(issuer) = issuer {
                issuer
            } else {
                account_manager.get_current_pubkey_hex().ok_or_else(|| {
                    anyhow::anyhow!("No current account selected and no issuer specified")
                })?
            };

            let status = client.get_reserve_status(&pubkey).await?;

            println!("Collateralization for {}:", status.issuer_pubkey);
            println!("  Ratio: {:.4}", status.collateralization_ratio);
            println!(
                "  Status: {}",
                get_collateralization_status(status.collateralization_ratio)
            );

            if status.collateralization_ratio < 1.0 {
                println!("⚠️  WARNING: Under-collateralized!");
            } else if status.collateralization_ratio < 1.5 {
                println!("⚠️  WARNING: Low collateralization");
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
