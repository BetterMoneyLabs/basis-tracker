use crate::account::AccountManager;
use crate::api::{
    CompleteRedemptionRequest, CreateNoteRequest, KeyStatusResponse, RedeemRequest, TrackerClient,
};
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
            let current_account = account_manager
                .get_current()
                .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

            let issuer_pubkey = current_account.get_pubkey_hex();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

            // Get reserve status before note creation
            println!("📊 Reserve Status Before Note Creation:");
            let status_before = client.get_reserve_status(&issuer_pubkey).await?;
            print_reserve_status(&status_before);

            // Create signing message following new spec: key || totalDebt
            // where key = blake2b256(ownerKey || receiverKey)
            let recipient_bytes = hex::decode(&recipient)?;
            let issuer_bytes = hex::decode(&issuer_pubkey)?;
            
            // Compute key = blake2b256(ownerKey || receiverKey)
            let mut key_hash_input = Vec::new();
            key_hash_input.extend_from_slice(&issuer_bytes);
            key_hash_input.extend_from_slice(&recipient_bytes);
            let key_hash = blake2b256_hash(&key_hash_input);
            
            // Build message: key || totalDebt
            let mut message = Vec::new();
            message.extend_from_slice(&key_hash);
            message.extend_from_slice(&amount.to_be_bytes());

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
            println!(
                "  Amount: {} nanoERG ({:.6} ERG)",
                amount,
                amount as f64 / 1_000_000_000.0
            );
            println!("  Timestamp: {}", timestamp);
        }
        NoteCommands::List { issuer, recipient } => {
            let current_account = account_manager
                .get_current()
                .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

            if issuer {
                let notes = client
                    .get_issuer_notes(&current_account.get_pubkey_hex())
                    .await?;
                if notes.is_empty() {
                    println!("No notes found where you are the issuer");
                } else {
                    println!("Notes where you are the issuer:");
                    for note in notes {
                        println!("  To: {}", note.recipient_pubkey);
                        println!("    Amount: {} nanoERG", note.amount_collected);
                        println!("    Redeemed: {} nanoERG", note.amount_redeemed);
                        println!(
                            "    Outstanding: {} nanoERG",
                            note.amount_collected - note.amount_redeemed
                        );
                        println!("    Created: {}", note.timestamp);
                    }
                }
            } else if recipient {
                let notes = client
                    .get_recipient_notes(&current_account.get_pubkey_hex())
                    .await?;
                if notes.is_empty() {
                    println!("No notes found where you are the recipient");
                } else {
                    println!("Notes where you are the recipient:");
                    for note in notes {
                        println!("  From: {}", note.issuer_pubkey);
                        println!("    Amount: {} nanoERG", note.amount_collected);
                        println!("    Redeemed: {} nanoERG", note.amount_redeemed);
                        println!(
                            "    Outstanding: {} nanoERG",
                            note.amount_collected - note.amount_redeemed
                        );
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
                println!(
                    "  Outstanding: {} nanoERG",
                    note.amount_collected - note.amount_redeemed
                );
                println!("  Created: {}", note.timestamp);
            } else {
                println!("Note not found");
            }
        }
        NoteCommands::Redeem { issuer, amount } => {
            let current_account = account_manager
                .get_current()
                .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

            let recipient_pubkey = current_account.get_pubkey_hex();

            // First, get the note to retrieve its original timestamp
            let note = client.get_note(&issuer, &recipient_pubkey).await?
                .ok_or_else(|| anyhow::anyhow!("Note not found for issuer {} and recipient {}", issuer, recipient_pubkey))?;

            // Verify that the note has sufficient outstanding debt
            if note.outstanding_debt() < amount {
                return Err(anyhow::anyhow!("Insufficient outstanding debt: {} nanoERG available, {} nanoERG requested",
                    note.outstanding_debt(), amount));
            }

            // Use the note's original timestamp for redemption
            let timestamp = note.timestamp;

            // Generate issuer signature for redemption
            // Message format: key || totalDebt [|| 0L for emergency]
            // where key = blake2b256(ownerKey || receiverKey)
            let issuer_pubkey_bytes = hex::decode(&issuer)
                .map_err(|e| anyhow::anyhow!("Invalid issuer pubkey hex: {}", e))?;
            let recipient_pubkey_bytes = hex::decode(&recipient_pubkey)
                .map_err(|e| anyhow::anyhow!("Invalid recipient pubkey hex: {}", e))?;

            // Compute key hash
            use blake2::{Blake2b, Digest};
            use generic_array::typenum::U32;
            let mut key_hash_input = Vec::new();
            key_hash_input.extend_from_slice(&issuer_pubkey_bytes);
            key_hash_input.extend_from_slice(&recipient_pubkey_bytes);
            let key_hash = Blake2b::<U32>::new()
                .chain_update(&key_hash_input)
                .finalize()
                .to_vec();

            // Build signing message: key || totalDebt
            let mut message = key_hash;
            message.extend_from_slice(&note.amount_collected.to_be_bytes());
            // Note: For emergency redemption, we would append 0L here
            // let emergency = false; // Could be made configurable
            // if emergency {
            //     message.extend_from_slice(&0u64.to_be_bytes());
            // }

            // Sign the message with issuer's private key
            let issuer_signature = current_account.sign_message(&message)?;

            // Initiate redemption
            let redeem_request = RedeemRequest {
                issuer_pubkey: issuer.clone(),
                recipient_pubkey: recipient_pubkey.clone(),
                amount,
                timestamp,
                reserve_box_id: String::new(), // Will be looked up by server
                tracker_box_id: String::new(), // Will be fetched by server
                tracker_nft_id: String::new(), // Will be fetched by server
                current_height: 0, // Will be fetched by server
                recipient_address: String::new(), // Will be derived from recipient_pubkey by server
                change_address: String::new(), // Will be derived from tracker pubkey by server
                issuer_signature: hex::encode(&issuer_signature),
                emergency: false,
                tracker_signature: None, // Server will generate tracker signature
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
    println!(
        "  Total Debt: {} nanoERG ({:.6} ERG)",
        status.total_debt,
        status.total_debt as f64 / 1_000_000_000.0
    );
    println!(
        "  Collateral: {} nanoERG ({:.6} ERG)",
        status.collateral,
        status.collateral as f64 / 1_000_000_000.0
    );
    println!(
        "  Collateralization Ratio: {:.4}",
        status.collateralization_ratio
    );
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

/// Blake2b256 hash function for creating signing message keys
fn blake2b256_hash(data: &[u8]) -> [u8; 32] {
    use blake2::{Blake2b, Digest};
    use generic_array::typenum::U32;
    
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    let result = hasher.finalize();
    result[..32]
        .try_into()
        .expect("Blake2b should produce at least 32 bytes")
}
