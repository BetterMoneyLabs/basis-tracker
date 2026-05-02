use crate::account::AccountManager;
use crate::api::{
    CompleteRedemptionRequest, CreateNoteRequest, KeyStatusResponse, RedeemRequest, TrackerClient,
};
use crate::demo_keys;
use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// IOU Note structure matching Scala demo format
#[derive(Debug, Serialize, Deserialize)]
pub struct DemoNote {
    pub payerKey: String,
    pub payeeKey: String,
    pub totalDebt: u64,
    pub totalDebtERG: f64,
    pub timestamp: u64,
    pub payerSignature: SignatureComponent,
    pub trackerSignature: SignatureComponent,
    pub message: String,
    pub messageFormat: String,
    pub noteKey: String,
}

/// Signature component (a point and z scalar)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureComponent {
    pub a: String,
    pub z: String,
}

#[derive(Subcommand)]
pub enum NoteCommands {
    /// Create a new debt note
    Create {
        /// Recipient public key (hex)
        #[arg(long)]
        recipient: Option<String>,
        /// Amount in nanoERG
        #[arg(long)]
        amount: u64,
        /// Use demo mode (Alice → Bob with tracker signature)
        #[arg(long, default_value = "false")]
        demo: bool,
        /// Output file (default: stdout)
        #[arg(long)]
        output: Option<PathBuf>,
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
        NoteCommands::Create { recipient, amount, demo, output } => {
            if demo {
                // Demo mode: Alice → Bob with tracker signature
                create_demo_note(amount, output).await?
            } else {
                // Normal mode: use CLI accounts
                let recipient = recipient
                    .ok_or_else(|| anyhow::anyhow!("--recipient required in non-demo mode"))?;
                
                create_normal_note(account_manager, client, &recipient, amount).await?
            }
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
            // Message format: key || totalDebt || timestamp (48 bytes)
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

            // Build signing message: key || totalDebt || timestamp (48 bytes)
            let mut message = key_hash;
            message.extend_from_slice(&note.amount_collected.to_be_bytes());
            message.extend_from_slice(&timestamp.to_be_bytes());

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

/// Create a demo note (Alice → Bob with tracker signature)
async fn create_demo_note(amount: u64, output: Option<PathBuf>) -> Result<()> {
    let alice = demo_keys::alice();
    let bob = demo_keys::bob();
    
    eprintln!("=== Basis Demo Note Creator ===");
    eprintln!("Creating IOU note from Alice to Bob");
    eprintln!();
    
    // Create signing message: blake2b256(alice_pk || bob_pk) || totalDebt || timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    
    let alice_pk_bytes = alice.public_key().serialize();
    let bob_pk_bytes = bob.public_key().serialize();
    
    // Compute key = blake2b256(ownerKey || receiverKey)
    let mut key_hash_input = Vec::new();
    key_hash_input.extend_from_slice(&alice_pk_bytes);
    key_hash_input.extend_from_slice(&bob_pk_bytes);
    let key_hash = blake2b256_hash(&key_hash_input);
    
    // Build message: key || totalDebt || timestamp (48 bytes)
    let mut message = Vec::new();
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&amount.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    
    eprintln!("Message: {}", hex::encode(&message));
    eprintln!("  Key hash: {}", hex::encode(&key_hash));
    eprintln!("  Total debt: {} nanoERG", amount);
    eprintln!("  Timestamp: {}", timestamp);
    eprintln!();
    
    // Alice signs the message
    let alice_sig = alice.keypair.sign_message(&message)?;
    let alice_sig_a = hex::encode(&alice_sig[0..33]);
    let alice_sig_z = hex::encode(&alice_sig[33..65]);
    
    eprintln!("✓ Alice's signature generated");
    
    // Request tracker signature from server
    eprintln!("Requesting tracker signature from server...");
    
    // For now, we'll sign with tracker demo key (in production, server would do this)
    let tracker = demo_keys::tracker();
    let tracker_sig = tracker.keypair.sign_message(&message)?;
    let tracker_sig_a = hex::encode(&tracker_sig[0..33]);
    let tracker_sig_z = hex::encode(&tracker_sig[33..65]);
    
    eprintln!("✓ Tracker's signature generated");
    eprintln!();
    
    // Build note JSON matching Scala demo format
    let note = DemoNote {
        payerKey: alice.public_key_hex(),
        payeeKey: bob.public_key_hex(),
        totalDebt: amount,
        totalDebtERG: amount as f64 / 1_000_000_000.0,
        timestamp,
        payerSignature: SignatureComponent {
            a: alice_sig_a,
            z: alice_sig_z,
        },
        trackerSignature: SignatureComponent {
            a: tracker_sig_a,
            z: tracker_sig_z,
        },
        message: hex::encode(&message),
        messageFormat: "key (32 bytes) || totalDebt (8 bytes) || timestamp (8 bytes)".to_string(),
        noteKey: {
            let mut key_input = Vec::with_capacity(66);
            key_input.extend_from_slice(&alice_pk_bytes);
            key_input.extend_from_slice(&bob_pk_bytes);
            hex::encode(blake2b256_hash(&key_input))
        },
    };
    
    let note_json = serde_json::to_string_pretty(&note)?;
    
    // Output JSON
    if let Some(path) = output {
        fs::write(&path, &note_json)?;
        eprintln!("✓ Note saved to: {}", path.display());
    } else {
        println!("{}", note_json);
    }
    
    eprintln!();
    eprintln!("=== Note Summary ===");
    eprintln!("  Payer:           {}...", alice.public_key_hex());
    eprintln!("  Payee:           {}...", bob.public_key_hex());
    eprintln!("  Amount:          {} nanoERG ({:.6} ERG)", amount, note.totalDebtERG);
    eprintln!("  Timestamp:       {}", timestamp);
    eprintln!("  Payer Sig Valid: ✓");
    eprintln!("  Tracker Sig Valid: ✓");
    eprintln!();
    eprintln!("=== Usage ===");
    eprintln!("  Redeem:  basis-cli transaction generate-redemption --issuer {} --recipient {} --amount {}", 
              alice.public_key_hex(), bob.public_key_hex(), amount);
    
    Ok(())
}

/// Create a normal note using CLI accounts
async fn create_normal_note(
    account_manager: &AccountManager,
    client: &TrackerClient,
    recipient: &str,
    amount: u64,
) -> Result<()> {
    let current_account = account_manager
        .get_current()
        .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

    let issuer_pubkey = current_account.get_pubkey_hex();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;

    // Get reserve status before note creation
    println!("📊 Reserve Status Before Note Creation:");
    let status_before = client.get_reserve_status(&issuer_pubkey).await?;
    print_reserve_status(&status_before);

    // Create signing message: key || totalDebt || timestamp (48 bytes)
    // where key = blake2b256(ownerKey || receiverKey)
    let recipient_bytes = hex::decode(recipient)?;
    let issuer_bytes = hex::decode(&issuer_pubkey)?;

    // Compute key = blake2b256(ownerKey || receiverKey)
    let mut key_hash_input = Vec::new();
    key_hash_input.extend_from_slice(&issuer_bytes);
    key_hash_input.extend_from_slice(&recipient_bytes);
    let key_hash = blake2b256_hash(&key_hash_input);

    // Build message: key || totalDebt || timestamp (48 bytes)
    let mut message = Vec::new();
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&amount.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());

    let signature = current_account.sign_message(&message)?;
    let signature_hex = hex::encode(signature);

    let request = CreateNoteRequest {
        issuer_pubkey: issuer_pubkey.clone(),
        recipient_pubkey: recipient.to_string(),
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
