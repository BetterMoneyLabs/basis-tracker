use crate::account::AccountManager;
use crate::api::{TrackerClient, CreateNoteRequest, RedeemRequest, CompleteRedemptionRequest, KeyStatusResponse};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum NoteCommands {
    /// Create a new debt note
    Create {
        /// Recipient public key (hex)
        #[arg(long)]
        recipient: String,
        /// Amount in nanoERG
        #[arg(long)]
        amount: u64,
    },
    /// List notes
    List {
        /// List notes by issuer
        #[arg(long)]
        issuer: bool,
        /// List notes by recipient
        #[arg(long)]
        recipient: bool,
    },
    /// Get a specific note
    Get {
        /// Issuer public key (hex)
        #[arg(long)]
        issuer: String,
        /// Recipient public key (hex)
        #[arg(long)]
        recipient: String,
    },
    /// Redeem a note
    Redeem {
        /// Issuer public key (hex)
        #[arg(long)]
        issuer: String,
        /// Amount to redeem in nanoERG
        #[arg(long)]
        amount: u64,
    },
}

pub async fn handle_note_command(
    cmd: NoteCommands,
    account_manager: &AccountManager,
    client: &TrackerClient,
) -> Result<()> {
    match cmd {
        NoteCommands::Create { recipient, amount } => {
            let current_account = account_manager.get_current()
                .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;
            
            let issuer_pubkey = current_account.get_pubkey_hex();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            
            // Get reserve status before note creation
            println!("📊 Reserve Status Before Note Creation:");
            let status_before = client.get_reserve_status(&issuer_pubkey).await?;
            print_reserve_status(&status_before);
            
            // Create signing message: recipient_pubkey || amount_be_bytes || timestamp_be_bytes
            let mut message = Vec::new();
            message.extend_from_slice(&hex::decode(&recipient)?);
            message.extend_from_slice(&amount.to_be_bytes());
            message.extend_from_slice(&timestamp.to_be_bytes());
            
            let signature = current_account.sign_message(&message)?;
            let signature_hex = hex::encode(signature);
            
            let request = CreateNoteRequest {
                issuer_pubkey: issuer_pubkey.clone(),
                recipient_pubkey: recipient.clone(),
                amount,
                timestamp,
                signature: signature_hex,
            };
            
            client.create_note(request).await?;
            
            // Get reserve status after note creation
            println!("\n📊 Reserve Status After Note Creation:");
            let status_after = client.get_reserve_status(&issuer_pubkey).await?;
            print_reserve_status(&status_after);
            
            println!("\n✅ Note created successfully");
            println!("📝 Note Details:");
            println!("  Issuer: {}", issuer_pubkey);
            println!("  Recipient: {}", recipient);
            println!("  Amount: {} nanoERG ({:.6} ERG)", amount, amount as f64 / 1_000_000_000.0);
            println!("  Timestamp: {}", timestamp);
        }
        NoteCommands::List { issuer, recipient } => {
            let current_account = account_manager.get_current()
                .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;
            
            if issuer {
                let notes = client.get_issuer_notes(&current_account.get_pubkey_hex()).await?;
                if notes.is_empty() {
                    println!("No notes found where you are the issuer");
                } else {
                    println!("Notes where you are the issuer:");
                    for note in notes {
                        println!("  To: {}", note.recipient_pubkey);
                        println!("    Amount: {} nanoERG", note.amount_collected);
                        println!("    Redeemed: {} nanoERG", note.amount_redeemed);
                        println!("    Outstanding: {} nanoERG", note.amount_collected - note.amount_redeemed);
                        println!("    Created: {}", note.timestamp);
                    }
                }
            } else if recipient {
                let notes = client.get_recipient_notes(&current_account.get_pubkey_hex()).await?;
                if notes.is_empty() {
                    println!("No notes found where you are the recipient");
                } else {
                    println!("Notes where you are the recipient:");
                    for note in notes {
                        println!("  From: {}", note.issuer_pubkey);
                        println!("    Amount: {} nanoERG", note.amount_collected);
                        println!("    Redeemed: {} nanoERG", note.amount_redeemed);
                        println!("    Outstanding: {} nanoERG", note.amount_collected - note.amount_redeemed);
                        println!("    Created: {}", note.timestamp);
                    }
                }
            } else {
                println!("Please specify --issuer or --recipient");
            }
        }
        NoteCommands::Get { issuer, recipient } => {
            let note = client.get_note(&issuer, &recipient).await?;
            
            if let Some(note) = note {
                println!("Note found:");
                println!("  Issuer: {}", note.issuer_pubkey);
                println!("  Recipient: {}", note.recipient_pubkey);
                println!("  Amount: {} nanoERG", note.amount_collected);
                println!("  Redeemed: {} nanoERG", note.amount_redeemed);
                println!("  Outstanding: {} nanoERG", note.amount_collected - note.amount_redeemed);
                println!("  Created: {}", note.timestamp);
            } else {
                println!("Note not found");
            }
        }
        NoteCommands::Redeem { issuer, amount } => {
            let current_account = account_manager.get_current()
                .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;
            
            let recipient_pubkey = current_account.get_pubkey_hex();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            
            // Initiate redemption
            let redeem_request = RedeemRequest {
                issuer_pubkey: issuer.clone(),
                recipient_pubkey: recipient_pubkey.clone(),
                amount,
                timestamp,
            };
            
            let response = client.initiate_redemption(redeem_request).await?;
            println!("✅ Redemption initiated");
            println!("  Redemption ID: {}", response.redemption_id);
            println!("  Amount: {} nanoERG", response.amount);
            println!("  Proof available: {}", response.proof_available);
            
            // Complete redemption
            let complete_request = CompleteRedemptionRequest {
                issuer_pubkey: issuer,
                recipient_pubkey,
                redeemed_amount: amount,
            };
            
            client.complete_redemption(complete_request).await?;
            println!("✅ Redemption completed");
        }
    }
    
    Ok(())
}

fn print_reserve_status(status: &KeyStatusResponse) {
    println!("  Total Debt: {} nanoERG ({:.6} ERG)", 
        status.total_debt, 
        status.total_debt as f64 / 1_000_000_000.0
    );
    println!("  Collateral: {} nanoERG ({:.6} ERG)", 
        status.collateral, 
        status.collateral as f64 / 1_000_000_000.0
    );
    println!("  Collateralization Ratio: {:.4}", status.collateralization_ratio);
    println!("  Note Count: {}", status.note_count);
    println!("  Last Updated: {}", status.last_updated);
    
    // Show collateralization status
    let status_text = match status.collateralization_ratio {
        r if r < 1.0 => "UNDER-COLLATERALIZED ⚠️",
        r if r < 1.5 => "LOW ⚠️",
        r if r < 2.0 => "ADEQUATE",
        r if r < 3.0 => "GOOD",
        _ => "EXCELLENT",
    };
    println!("  Status: {}", status_text);
}