use crate::api::TrackerClient;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use std::fs;

#[derive(Subcommand)]
pub enum TransactionCommands {
    /// Generate unsigned redemption transaction
    GenerateRedemption {
        /// Issuer public key (hex)
        #[arg(long)]
        issuer_pubkey: String,
        /// Recipient public key (hex)
        #[arg(long)]
        recipient_pubkey: String,
        /// Redemption amount in nanoERG
        #[arg(long)]
        amount: u64,
        /// Output file for the transaction JSON (optional, defaults to stdout)
        #[arg(long)]
        output_file: Option<String>,
    },
}

pub async fn handle_transaction_command(
    cmd: TransactionCommands,
    client: &TrackerClient,
) -> Result<()> {
    match cmd {
        TransactionCommands::GenerateRedemption {
            issuer_pubkey,
            recipient_pubkey,
            amount,
            output_file,
        } => {
            generate_redemption_transaction(client, &issuer_pubkey, &recipient_pubkey, amount, output_file).await
        }
    }
}

async fn generate_redemption_transaction(
    client: &TrackerClient,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    output_file: Option<String>,
) -> Result<()> {
    // Validate public keys
    if hex::decode(issuer_pubkey).map_err(|e| anyhow::anyhow!("Invalid issuer public key: {}", e))?.len() != 33 {
        return Err(anyhow::anyhow!("Issuer public key must be 33 bytes (66 hex characters)"));
    }

    if hex::decode(recipient_pubkey).map_err(|e| anyhow::anyhow!("Invalid recipient public key: {}", e))?.len() != 33 {
        return Err(anyhow::anyhow!("Recipient public key must be 33 bytes (66 hex characters)"));
    }

    println!("🔍 Retrieving note information...");
    let note = client.get_note(issuer_pubkey, recipient_pubkey).await?
        .ok_or_else(|| anyhow::anyhow!("Note not found for issuer {} and recipient {}", issuer_pubkey, recipient_pubkey))?;

    // Verify that the redemption amount does not exceed the note's outstanding debt
    if note.outstanding_debt() < amount {
        return Err(anyhow::anyhow!("Insufficient outstanding debt: {} nanoERG available, {} nanoERG requested",
            note.outstanding_debt(), amount));
    }

    println!("🔍 Retrieving issuer's reserve box...");
    let reserves_response = client.get_reserves_by_issuer(issuer_pubkey).await?;
    let reserve_box = reserves_response.first()
        .ok_or_else(|| anyhow::anyhow!("No reserve box found for issuer {}", issuer_pubkey))?;

    // Verify sufficient collateral
    if reserve_box.base_info.collateral_amount < amount {
        return Err(anyhow::anyhow!("Insufficient collateral in reserve: {} nanoERG available, {} nanoERG requested",
            reserve_box.base_info.collateral_amount, amount));
    }

    let reserve_box_id = &reserve_box.box_id;

    println!("🔍 Retrieving latest tracker box...");
    let tracker_box_response = client.get_latest_tracker_box_id().await?;
    let tracker_box_id = tracker_box_response.tracker_box_id;

    println!("🔗 Converting public keys to addresses...");
    let recipient_address = pubkey_to_address(recipient_pubkey)?;
    let issuer_address = pubkey_to_address(issuer_pubkey)?;

    // Calculate remaining collateral after redemption
    let remaining_collateral = reserve_box.base_info.collateral_amount - amount;
    let transaction_fee = 1_000_000; // 0.001 ERG

    // Create transaction structure following the Ergo node's /wallet/transaction/send format
    let transaction_json = json!({
        "requests": [
            {
                "address": recipient_address,
                "value": amount,
                "assets": [],
                "registers": {}
            },
            {
                "address": issuer_address,
                "value": remaining_collateral - transaction_fee,
                "assets": [
                    {
                        "tokenId": reserve_box.tracker_nft_id.as_ref().unwrap_or(&"tracker_nft_id_not_configured".to_string()),
                        "amount": 1
                    }
                ],
                "registers": {
                    "R4": issuer_pubkey,
                    "R5": "avl_tree_root_digest_placeholder" // This would be the actual AVL tree root digest in a real implementation
                }
            }
        ],
        "fee": transaction_fee,
        "inputsRaw": [
            format!("serialized_reserve_box_{}", reserve_box_id)
        ],
        "dataInputsRaw": [
            format!("serialized_tracker_box_{}", tracker_box_id)
        ]
    });

    let json_string = serde_json::to_string_pretty(&transaction_json)?;

    match output_file {
        Some(file_path) => {
            fs::write(&file_path, &json_string)?;
            println!("✅ Transaction JSON written to: {}", file_path);
        }
        None => {
            println!("{}", json_string);
        }
    }

    println!("✅ Redemption transaction generated successfully!");
    println!("📋 Transaction details:");
    println!("   Issuer: {}", issuer_pubkey);
    println!("   Recipient: {}", recipient_pubkey);
    println!("   Redemption amount: {} nanoERG", amount);
    println!("   Reserve box ID: {}", reserve_box_id);
    println!("   Tracker box ID: {}", tracker_box_id);
    println!("   Transaction fee: {} nanoERG", transaction_fee);

    Ok(())
}

// Helper function to convert public key to a placeholder address
// In a real implementation, this would use the Ergo node's /utils/rawToAddress API
fn pubkey_to_address(pubkey_hex: &str) -> Result<String> {
    let pubkey_bytes = hex::decode(pubkey_hex)
        .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;

    if pubkey_bytes.len() != 33 {
        return Err(anyhow::anyhow!("Public key must be 33 bytes"));
    }

    // For now, return a placeholder address based on the public key
    // In a real implementation, this would call the Ergo node's /utils/rawToAddress API
    Ok(format!("9{}", &pubkey_hex[..30])) // Create a placeholder P2PK address starting with '9'
}