use crate::api::{TrackerClient, ErgoBoxDetails};
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use std::collections::HashMap;
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
        /// Emergency redemption flag (after 3 days tracker unavailability)
        #[arg(long, default_value = "false")]
        emergency: bool,
    },
}

pub async fn handle_transaction_command(
    cmd: TransactionCommands,
    client: &TrackerClient,
    account_manager: &crate::account::AccountManager,
) -> Result<()> {
    match cmd {
        TransactionCommands::GenerateRedemption {
            issuer_pubkey,
            recipient_pubkey,
            amount,
            output_file,
            emergency,
        } => {
            generate_redemption_transaction(client, account_manager, &issuer_pubkey, &recipient_pubkey, amount, output_file, emergency).await
        }
    }
}

async fn generate_redemption_transaction(
    client: &TrackerClient,
    account_manager: &crate::account::AccountManager,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    output_file: Option<String>,
    emergency: bool,
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
    let tracker_nft_id = &reserve_box.base_info.tracker_nft_id;

    println!("🔍 Retrieving latest tracker box...");
    let tracker_box_response = client.get_latest_tracker_box_id().await;
    let tracker_box_id = match tracker_box_response {
        Ok(response) => {
            println!("✅ Found tracker box: {}", &response.tracker_box_id[..16]);
            response.tracker_box_id
        },
        Err(e) => {
            return Err(anyhow::anyhow!(
                "No tracker box found: {}. Cannot generate redemption transaction without a tracker box.",
                e
            ));
        }
    };

    println!("🔗 Converting public keys to addresses...");
    let recipient_address = pubkey_to_address(recipient_pubkey)?;

    // Get tracker lookup proof for context var #8 from server
    println!("🔍 Retrieving tracker lookup proof from server...");
    let tracker_proof = client.get_tracker_proof(issuer_pubkey, recipient_pubkey).await?;
    let total_debt = tracker_proof.total_debt;
    let tracker_lookup_proof = hex::decode(&tracker_proof.proof)
        .map_err(|e| anyhow::anyhow!("Invalid tracker proof hex: {}", e))?;
    let tracker_state_digest = tracker_proof.tracker_state_digest;

    // Build serialized SAvlTree for R5 register from tracker state digest
    // Format: type_byte (0x64) + root_digest (33 bytes) + flags (0x01) + key_len (32) + value_len (0)
    let r5_bytes = build_savl_tree_from_digest(&tracker_state_digest);
    let r5_hex = hex::encode(&r5_bytes);

    // Get the reserve contract P2S address from the server configuration
    println!("🔍 Retrieving reserve contract P2S address from server configuration...");
    let reserve_contract_p2s = client.get_basis_reserve_contract_p2s().await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve reserve contract P2S address from server: {}", e))?;

    // Calculate remaining collateral after redemption
    let remaining_collateral = reserve_box.base_info.collateral_amount - amount;
    let transaction_fee = 1_000_000; // 0.001 ERG

    // Determine if this is the first redemption
    let is_first_redemption = note.amount_redeemed == 0;
    let (reserve_lookup_proof, reserve_insert_proof) = if is_first_redemption {
        // For first redemption, no lookup proof needed, but we still need insert proof
        println!("🔍 First redemption - generating reserve insert proof...");
        // For first redemption, insert proof can be generated (lookup proof is omitted)
        let reserve_proof = client.get_reserve_proof(issuer_pubkey, recipient_pubkey).await
            .map_err(|e| anyhow::anyhow!("Failed to get reserve proof for first redemption: {}", e))?;
        let insert_proof = hex::decode(&reserve_proof.insert_proof)
            .map_err(|e| anyhow::anyhow!("Invalid reserve insert proof hex: {}", e))?;
        (None, insert_proof)
    } else {
        // For subsequent redemptions, get reserve lookup proof and insert proof from server
        println!("🔍 Retrieving reserve proofs from server...");
        let reserve_proof = client.get_reserve_proof(issuer_pubkey, recipient_pubkey).await
            .map_err(|e| anyhow::anyhow!("Failed to get reserve proof for subsequent redemption: {}", e))?;
        println!("✅ Got reserve proof: already_redeemed={} nanoERG, is_first={}",
            reserve_proof.already_redeemed, reserve_proof.is_first_redemption);
        // Decode the hex-encoded proofs
        let lookup_proof = if let Some(proof_hex) = &reserve_proof.proof {
            Some(hex::decode(proof_hex)
                .map_err(|e| anyhow::anyhow!("Invalid reserve lookup proof hex: {}", e))?)
        } else {
            return Err(anyhow::anyhow!("Reserve lookup proof is required for subsequent redemption"));
        };
        let insert_proof = hex::decode(&reserve_proof.insert_proof)
            .map_err(|e| anyhow::anyhow!("Invalid reserve insert proof hex: {}", e))?;
        (lookup_proof, insert_proof)
    };

    // Retrieve the actual tracker box from the Ergo node
    println!("🔍 Retrieving tracker box from Ergo node...");
    let tracker_box_details = client.get_box_from_node(&tracker_box_id, "http://159.89.116.15:11088", Some("hello")).await
        .unwrap_or_else(|_| {
            println!("⚠️  Tracker box not found, using placeholder.");
            ErgoBoxDetails {
                box_id: tracker_box_id.clone(),
                value: 0,
                ergo_tree: String::new(),
                assets: vec![],
                additional_registers: HashMap::new(),
                creation_height: 0,
                transaction_id: String::new(),
                index: 0,
            }
        });

    // Retrieve the actual reserve box from the Ergo node
    println!("🔍 Retrieving reserve box from Ergo node...");
    let reserve_box_details = client.get_box_from_node(reserve_box_id, "http://159.89.116.15:11088", Some("hello")).await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve reserve box from Ergo node: {}", e))?;

    // Serialize boxes to hex-encoded bytes for Ergo node API
    // The Ergo node expects inputsRaw and dataInputsRaw to contain hex-encoded box IDs
    // The node will fetch the full box details internally
    println!("📦 Preparing box IDs for transaction...");
    let tracker_box_bytes = tracker_box_details.box_id.clone();
    let reserve_box_bytes = reserve_box_details.box_id.clone();

    // Get issuer signature from CLI wallet
    println!("🔑 Signing redemption with issuer key...");
    let current_account = account_manager.get_current()
        .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

    // Create signing message: key || totalDebt || timestamp (48 bytes)
    // Both issuer and tracker sign the SAME message
    let issuer_pubkey_bytes = hex::decode(issuer_pubkey)?;
    let recipient_pubkey_bytes = hex::decode(recipient_pubkey)?;

    let mut key_hash_input = Vec::new();
    key_hash_input.extend_from_slice(&issuer_pubkey_bytes);
    key_hash_input.extend_from_slice(&recipient_pubkey_bytes);
    let key_hash = blake2b256_hash(&key_hash_input);

    let note_timestamp = note.timestamp;
    let mut message = Vec::with_capacity(48);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&total_debt.to_be_bytes());
    message.extend_from_slice(&note_timestamp.to_be_bytes());

    let issuer_signature = current_account.sign_message(&message)?;

    // Get tracker signature from server
    let tracker_signature_response = client.request_tracker_signature(
        issuer_pubkey,
        recipient_pubkey,
        total_debt,
        note_timestamp,
        emergency,
    ).await?;
    let tracker_signature = hex::decode(&tracker_signature_response.tracker_signature)
        .map_err(|e| anyhow::anyhow!("Invalid tracker signature hex: {}", e))?;

    // Use the reserve insert proof fetched from server
    // This is the correct proof for inserting into the reserve AVL tree
    let insert_proof = reserve_insert_proof.clone();

    // Build context extension map with properly serialized Ergo constants
    // Ergo constant serialization format:
    // - Byte (02): prefix 02 + 1-byte hex value
    // - GroupElement (07): prefix 07 + 33-byte compressed pubkey hex
    // - Coll[Byte] (0e): prefix 0e + 2-byte length + data hex
    // - Long (05): prefix 05 + VLQ encoded value (simplified: 8-byte big-endian)
    let mut context_extension = HashMap::new();
    
    // #0: Action byte (Byte constant)
    context_extension.insert("0".to_string(), json!(format!("02{:02x}", 0))); // action byte = 0
    
    // #1: Receiver pubkey (GroupElement constant)
    context_extension.insert("1".to_string(), json!(format!("07{}", recipient_pubkey)));
    
    // #2: Reserve signature (Coll[Byte] constant, 65 bytes)
    context_extension.insert("2".to_string(), json!(format!("0e{:04x}{}", issuer_signature.len(), hex::encode(&issuer_signature))));
    
    // #3: Total debt (Long constant)
    context_extension.insert("3".to_string(), json!(format!("05{:016x}", total_debt)));
    
    // #4: Timestamp (Long constant)
    context_extension.insert("4".to_string(), json!(format!("05{:016x}", note_timestamp)));
    
    // #5: Insert proof (Coll[Byte] constant)
    context_extension.insert("5".to_string(), json!(format!("0e{:04x}{}", insert_proof.len(), hex::encode(&insert_proof))));
    
    // #6: Tracker signature (Coll[Byte] constant, 65 bytes)
    context_extension.insert("6".to_string(), json!(format!("0e{:04x}{}", tracker_signature.len(), hex::encode(&tracker_signature))));
    
    // #7: Reserve lookup proof (optional, Coll[Byte] constant)
    if let Some(ref proof) = reserve_lookup_proof {
        context_extension.insert("7".to_string(), json!(format!("0e{:04x}{}", proof.len(), hex::encode(proof))));
    }
    
    // #8: Tracker lookup proof (Coll[Byte] constant)
    context_extension.insert("8".to_string(), json!(format!("0e{:04x}{}", tracker_lookup_proof.len(), hex::encode(&tracker_lookup_proof))));

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
                "address": reserve_contract_p2s,
                "value": remaining_collateral - transaction_fee,
                "assets": [
                    {
                        "tokenId": tracker_nft_id,
                        "amount": 1
                    }
                ],
                "registers": {
                    "R4": issuer_pubkey,
                    "R5": r5_hex,
                    "R6": tracker_nft_id
                }
            }
        ],
        "fee": transaction_fee,
        "inputsRaw": [
            reserve_box_bytes
        ],
        "dataInputsRaw": [
            tracker_box_bytes
        ],
        "contextExtension": context_extension
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
    println!("   Total debt: {} nanoERG", total_debt);
    println!("   Already redeemed: {} nanoERG", note.amount_redeemed);
    println!("   Reserve box ID: {}", reserve_box_id);
    println!("   Tracker box ID: {}", tracker_box_id);
    println!("   Transaction fee: {} nanoERG", transaction_fee);
    println!("   Emergency redemption: {}", emergency);
    println!("   First redemption: {}", is_first_redemption);
    println!("📝 Context Extension Variables:");
    println!("   #0 (action): 0x00 (redemption)");
    println!("   #1 (receiver): {}", recipient_pubkey);
    println!("   #2 (reserveSig): {} bytes", issuer_signature.len());
    println!("   #3 (totalDebt): {}", total_debt);
    println!("   #5 (insertProof): {} bytes", insert_proof.len());
    println!("   #6 (trackerSig): {} bytes", tracker_signature.len());
    if let Some(ref proof) = reserve_lookup_proof {
        println!("   #7 (reserveLookupProof): {} bytes", proof.as_slice().len());
    } else {
        println!("   #7 (reserveLookupProof): omitted (first redemption)");
    }
    println!("   #8 (trackerLookupProof): {} bytes", tracker_lookup_proof.len());

    Ok(())
}

/// Helper function to build serialized SAvlTree from tracker state digest
    /// Format: type_byte (0x64) + root_digest (33 bytes) + flags (0x01) + key_len (32) + value_len (0)
fn build_savl_tree_from_digest(digest_hex: &str) -> Vec<u8> {
    // Decode the hex-encoded digest (should be 66 hex chars = 33 bytes)
    let digest_bytes = hex::decode(digest_hex)
        .unwrap_or_else(|_| vec![0u8; 33]);
    
    // Ensure we have exactly 33 bytes
    if digest_bytes.len() != 33 {
        eprintln!("⚠️  Warning: Tracker state digest is not 33 bytes (got {}), using padding", digest_bytes.len());
    }
    
    let mut root_digest = [0u8; 33];
    root_digest.copy_from_slice(&digest_bytes[..33.min(digest_bytes.len())]);

    // Build the serialized SAvlTree (43 bytes total)
    let mut r5_bytes = Vec::with_capacity(43);
    r5_bytes.push(0x64u8);                        // Type byte: SAvlTree
    r5_bytes.extend_from_slice(&root_digest);     // 33-byte root digest
    r5_bytes.push(0x01u8);                        // Flags: insert-only allowed
    r5_bytes.extend_from_slice(&32u32.to_be_bytes()); // Key length: 32 bytes
    r5_bytes.extend_from_slice(&0u32.to_be_bytes());  // Value length: 0 (variable)
    
    r5_bytes
}

// Helper function for Blake2b256 hashing
fn blake2b256_hash(data: &[u8]) -> [u8; 32] {
    use blake2::{Blake2b, Digest};
    use generic_array::typenum::U32;
    
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    hasher.finalize().to_vec().try_into().unwrap()
}

// Helper function to convert public key to a P2PK address using ergo-lib
fn pubkey_to_address(pubkey_hex: &str) -> Result<String> {
    use ergo_lib::ergotree_ir::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let pubkey_bytes = hex::decode(pubkey_hex)
        .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;

    if pubkey_bytes.len() != 33 {
        return Err(anyhow::anyhow!("Public key must be 33 bytes"));
    }

    // Parse public key as EcPoint (compressed secp256k1 point)
    let ec_point = EcPoint::sigma_parse_bytes(&pubkey_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid public key format: {}", e))?;

    // Create P2PK address from EcPoint
    let prove_dlog = ProveDlog::new(ec_point);
    let address = Address::P2Pk(prove_dlog);

    // Encode address as base58 string (using mainnet prefix by default)
    let encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}
