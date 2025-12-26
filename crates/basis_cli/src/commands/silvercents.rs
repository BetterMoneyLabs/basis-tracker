use crate::account::AccountManager;
use crate::api::TrackerClient;
use anyhow::Result;
use clap::Subcommand;
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct SilverReserve {
    total_physical_silver: u64,
    total_silvercents_issued: u64,
    total_silvercents_redeemed: u64,
}

impl Default for SilverReserve {
    fn default() -> Self {
        Self {
            total_physical_silver: 0,
            total_silvercents_issued: 0,
            total_silvercents_redeemed: 0,
        }
    }
}

#[derive(Subcommand)]
pub enum SilverCentsCommands {
    /// Issue silver-backed offchain cash
    Issue {
        /// Amount of SilverCents to issue
        #[arg(long)]
        amount: u64,
        /// Recipient public key (hex)
        #[arg(long)]
        to: String,
    },
    /// Pay using offchain SilverCents
    Pay {
        /// Recipient public key (hex)
        #[arg(long)]
        to: String,
        /// Amount to pay
        #[arg(long)]
        amount: u64,
    },
    /// Redeem SilverCents from reserve
    Redeem {
        /// Issuer public key (hex)
        #[arg(long)]
        issuer: String,
        /// Amount to redeem
        #[arg(long)]
        amount: u64,
    },
    /// Deposit physical silver coins
    Deposit {
        /// Amount of silver coins to deposit
        #[arg(long)]
        amount: u64,
    },
    /// Check status / collateralization
    Status,
}

fn load_reserve() -> Result<SilverReserve> {
    let path = "silver_reserve.json";
    if std::path::Path::new(path).exists() {
        let data = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    } else {
        Ok(SilverReserve::default())
    }
}

fn save_reserve(reserve: &SilverReserve) -> Result<()> {
    let data = serde_json::to_string_pretty(reserve)?;
    fs::write("silver_reserve.json", data)?;
    Ok(())
}

pub async fn handle_silvercents_command(
    cmd: SilverCentsCommands,
    account_manager: &AccountManager,
    client: &TrackerClient,
) -> Result<()> {
    match cmd {
        SilverCentsCommands::Issue { amount, to } => {
            // Load reserve
            let mut reserve = load_reserve()?;
            
            // Check if enough silver
            if reserve.total_physical_silver < reserve.total_silvercents_issued - reserve.total_silvercents_redeemed + amount {
                println!("Not enough physical silver backing. Current backing: {}", reserve.total_physical_silver);
                return Ok(());
            }
            
            // Issue note (similar to create note)
            let recipient_pubkey = hex::decode(&to)?;
            let recipient_pubkey: [u8; 33] = recipient_pubkey.try_into().map_err(|_| anyhow::anyhow!("Invalid pubkey length"))?;
            
            let issuer_keypair = account_manager.get_keypair()?;
            let issuer_pubkey = issuer_keypair.get_public_key_bytes();
            
            // Create note
            let note = basis_store::IouNote::create_and_sign(
                recipient_pubkey,
                amount,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                &issuer_keypair.get_private_key_bytes(),
            )?;
            
            // Submit to tracker
            let request = crate::api::CreateNoteRequest {
                issuer_pubkey: hex::encode(issuer_pubkey),
                recipient_pubkey: hex::encode(note.recipient_pubkey),
                amount: note.amount_collected,
                timestamp: note.timestamp,
                signature: hex::encode(note.signature),
            };
            
            client.create_note(request).await?;
            
            // Update reserve
            reserve.total_silvercents_issued += amount;
            save_reserve(&reserve)?;
            
            println!("Issued {} SilverCents to {}", amount, to);
        }
        SilverCentsCommands::Pay { to, amount } => {
            // Similar to issue, but update existing note
            let recipient_pubkey = hex::decode(&to)?;
            let recipient_pubkey: [u8; 33] = recipient_pubkey.try_into().map_err(|_| anyhow::anyhow!("Invalid pubkey length"))?;
            
            let issuer_keypair = account_manager.get_keypair()?;
            let issuer_pubkey = issuer_keypair.get_public_key_bytes();
            
            // Get current note
            let current_note = client.get_note(&hex::encode(issuer_pubkey), &to).await?;
            
            let (new_amount, redeemed) = if let Some(note) = current_note {
                (note.amount_collected + amount, note.amount_redeemed)
            } else {
                (amount, 0)
            };
            
            let note = basis_store::IouNote::create_and_sign(
                recipient_pubkey,
                new_amount,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                &issuer_keypair.get_private_key_bytes(),
            )?;
            
            let request = crate::api::CreateNoteRequest {
                issuer_pubkey: hex::encode(issuer_pubkey),
                recipient_pubkey: hex::encode(note.recipient_pubkey),
                amount: note.amount_collected,
                timestamp: note.timestamp,
                signature: hex::encode(note.signature),
            };
            
            client.create_note(request).await?;
            
            println!("Paid {} SilverCents to {}", amount, to);
        }
        SilverCentsCommands::Redeem { issuer, amount } => {
            // Redeem from reserve
            let recipient_keypair = account_manager.get_keypair()?;
            let recipient_pubkey = recipient_keypair.get_public_key_bytes();
            
            let issuer_pubkey_bytes = hex::decode(&issuer)?;
            let issuer_pubkey: [u8; 33] = issuer_pubkey_bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid issuer pubkey"))?;
            
            // Get current note
            let current_note = client.get_note(&issuer, &hex::encode(recipient_pubkey)).await?;
            
            if let Some(note) = current_note {
                if note.amount_collected - note.amount_redeemed < amount {
                    println!("Not enough SilverCents to redeem");
                    return Ok(());
                }
                
                // Redeem
                let redeem_request = crate::api::RedeemRequest {
                    issuer_pubkey: issuer.clone(),
                    recipient_pubkey: hex::encode(recipient_pubkey),
                    amount,
                    timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                };
                
                client.initiate_redemption(redeem_request).await?;
                
                // Update reserve
                let mut reserve = load_reserve()?;
                reserve.total_silvercents_redeemed += amount;
                save_reserve(&reserve)?;
                
                println!("Redeemed {} SilverCents from {}. Physical silver release confirmed.", amount, issuer);
            } else {
                println!("No SilverCents to redeem from {}", issuer);
            }
        }
        SilverCentsCommands::Status => {
            let reserve = load_reserve()?;
            let collateralized = reserve.total_silvercents_issued - reserve.total_silvercents_redeemed;
            
            // Get on-chain reserve status
            let pubkey = hex::encode(account_manager.get_keypair()?.get_public_key_bytes());
            let key_status = client.get_reserve_status(&pubkey).await?;
            
            println!("SilverCents Status:");
            println!("Physical silver reserve: {}", reserve.total_physical_silver);
            println!("SilverCents issued: {}", reserve.total_silvercents_issued);
            println!("SilverCents redeemed: {}", reserve.total_silvercents_redeemed);
            println!("Outstanding SilverCents: {}", collateralized);
            println!("Collateralization ratio: {:.2}%", if collateralized > 0 { (reserve.total_physical_silver as f64 / collateralized as f64) * 100.0 } else { 0.0 });
            println!("On-chain reserve collateral: {}", key_status.collateral);
            println!("On-chain total debt: {}", key_status.total_debt);
            println!("On-chain collateralization ratio: {:.2}%", key_status.collateralization_ratio);
        }
        SilverCentsCommands::Deposit { amount } => {
            let mut reserve = load_reserve()?;
            reserve.total_physical_silver += amount;
            save_reserve(&reserve)?;
            println!("Deposited {} physical silver coins. Total reserve: {}", amount, reserve.total_physical_silver);
        }
    }
    Ok(())
}