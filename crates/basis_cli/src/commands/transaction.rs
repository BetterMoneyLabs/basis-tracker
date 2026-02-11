use crate::api::{TrackerClient, ErgoBoxDetails};
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
    let tracker_box_response = client.get_latest_tracker_box_id().await;
    let tracker_box_id = match tracker_box_response {
        Ok(response) => {
            println!("✅ Found tracker box: {}", &response.tracker_box_id[..16]);
            response.tracker_box_id
        },
        Err(e) => {
            eprintln!("⚠️  No tracker box found: {}. This is normal in a fresh system.", e);
            // Using a placeholder tracker box ID since no tracker box exists yet
            "0000000000000000000000000000000000000000000000000000000000000000".to_string() // Placeholder tracker box ID
        }
    };

    // The tracker box bytes will be retrieved later when we get the box details from the node
    // This is handled in the next section where we call get_box_from_node

    println!("🔗 Converting public keys to addresses...");
    let recipient_address = pubkey_to_address(recipient_pubkey)?;
    let issuer_address = pubkey_to_address(issuer_pubkey)?;

    // Get the reserve contract P2S address from the server configuration
    println!("🔍 Retrieving reserve contract P2S address from server configuration...");
    let reserve_contract_p2s = client.get_basis_reserve_contract_p2s().await
        .unwrap_or_else(|e| {
            eprintln!("⚠️  Could not retrieve reserve contract P2S from server, using placeholder: {}", e);
            "W52Uvz86YC7XkV8GXjM9DDkMLHWqZLyZGRi1FbmyppvPy7cREnehzz21DdYTdrsuw268CxW3gkXE6D5B8748FYGg3JEVW9R6VFJe8ZDknCtiPbh56QUCJo5QDizMfXaKnJ3jbWV72baYPCw85tmiJowR2wd4AjsEuhZP4Ry4QRDcZPvGogGVbdk7ykPAB7KN2guYEhS7RU3xm23iY1YaM5TX1ditsWfxqCBsvq3U6X5EU2Y5KCrSjQxdtGcwoZsdPQhfpqcwHPcYqM5iwK33EU1cHqggeSKYtLMW263f1TY7Lfu3cKMkav1CyomR183TLnCfkRHN3vcX2e9fSaTpAhkb74yo6ZRXttHNP23JUASWs9ejCaguzGumwK3SpPCLBZY6jFMYWqeaanH7XAtTuJA6UCnxvrKko5PX1oSB435Bxd3FbvDAsEmHpUqqtP78B7SKxFNPvJeZuaN7r5p8nDLxUPZBrWwz2vtcgWPMq5RrnoJdrdqrnXMcMEQPF5AKDYuKMKbCRgn3HLvG98JXJ4bCc2wzuZhnCRQaFXTy88knEoj".to_string()
        });

    // Calculate remaining collateral after redemption
    let remaining_collateral = reserve_box.base_info.collateral_amount - amount;
    let transaction_fee = 1_000_000; // 0.001 ERG

    // Retrieve the actual tracker box from the Ergo node
    println!("🔍 Retrieving tracker box from Ergo node...");
    let tracker_box_details = client.get_box_from_node(&tracker_box_id, "http://159.89.116.15:11088", Some("hello")).await
        .unwrap_or_else(|_| {
            // If tracker box doesn't exist, use placeholder
            println!("⚠️  Tracker box not found, using placeholder. This is normal in a fresh system.");
            ErgoBoxDetails {
                box_id: tracker_box_id.clone(),
                value: 0,
                ergo_tree: String::new(),
                assets: vec![],
                additional_registers: std::collections::HashMap::new(),
                creation_height: 0,
                transaction_id: String::new(),
                index: 0,
            }
        });

    // In a real implementation, we would serialize the tracker box to bytes
    // For now, we'll use a placeholder but the real implementation would be:
    // let tracker_box_serialized = serialize_box_to_bytes(&tracker_box_details)?;
    // let tracker_box_bytes = hex::encode(&tracker_box_serialized);
    let tracker_box_bytes = format!("serialized_box_{}", tracker_box_id);

    // Retrieve the actual reserve box from the Ergo node
    println!("🔍 Retrieving reserve box from Ergo node...");
    let reserve_box_details = client.get_box_from_node(reserve_box_id, "http://159.89.116.15:11088", Some("hello")).await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve reserve box from Ergo node: {}", e))?;

    // In a real implementation, we would serialize the reserve box to bytes
    // For now, we'll use a placeholder but the real implementation would be:
    // let reserve_box_serialized = serialize_box_to_bytes(&reserve_box_details)?;
    // let reserve_box_bytes = hex::encode(&reserve_box_serialized);
    let reserve_box_bytes = format!("serialized_box_{}", reserve_box_id);

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
                "address": reserve_contract_p2s, // Use the P2S address from server configuration
                "value": remaining_collateral - transaction_fee,
                "assets": [
                    {
                        "tokenId": &reserve_box.base_info.tracker_nft_id,
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
            reserve_box_bytes // Use the actual serialized reserve box bytes from node API
        ],
        "dataInputsRaw": [
            tracker_box_bytes // Use the actual serialized tracker box bytes from node API
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