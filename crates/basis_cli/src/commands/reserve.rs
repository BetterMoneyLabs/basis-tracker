use crate::account::AccountManager;
use crate::api::TrackerClient;
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ReserveCommands {
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
        ReserveCommands::Status { issuer } => {
            let pubkey = if let Some(issuer) = issuer {
                issuer
            } else {
                account_manager.get_current_pubkey_hex()
                    .ok_or_else(|| anyhow::anyhow!("No current account selected and no issuer specified"))?
            };
            
            let status = client.get_reserve_status(&pubkey).await?;
            
            println!("Reserve Status for {}:", status.issuer_pubkey);
            println!("  Total Debt: {} nanoERG", status.total_debt);
            println!("  Collateral: {} nanoERG", status.collateral);
            println!("  Collateralization Ratio: {:.2}", status.collateralization_ratio);
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
                account_manager.get_current_pubkey_hex()
                    .ok_or_else(|| anyhow::anyhow!("No current account selected and no issuer specified"))?
            };
            
            let status = client.get_reserve_status(&pubkey).await?;
            
            println!("Collateralization for {}:", status.issuer_pubkey);
            println!("  Ratio: {:.4}", status.collateralization_ratio);
            println!("  Status: {}", get_collateralization_status(status.collateralization_ratio));
            
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