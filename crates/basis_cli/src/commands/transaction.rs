use crate::api::TrackerClient;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use std::collections::HashMap;
use std::fs;

const NODE_URL: &str = "http://127.0.0.1:9053";
const API_KEY: &str = "hello";
const TRANSACTION_FEE: u64 = 1_000_000;

/// Encode an unsigned integer using Ergo's VLQ (Variable-Length Quantity) encoding.
/// Bytes are emitted least-significant group first (little-endian VLQ).
fn vlq_encode(mut value: usize) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    let mut bytes = Vec::new();
    while value > 0 {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
    }
    bytes
}

/// Serialize bytes as Ergo `Coll[Byte]` constant: prefix `0x0e` + 1-byte length + data.
fn serialize_coll_bytes(data: &[u8]) -> String {
    assert!(
        data.len() <= 255,
        "Coll[Byte] length {} exceeds 255-byte prefix",
        data.len()
    );
    let mut bytes = vec![0x0e, data.len() as u8];
    bytes.extend_from_slice(data);
    hex::encode(bytes)
}

/// Serialize a long value as Ergo `Long` constant: prefix `0x05` + zigzag(VLQ),
/// using Ergo's little-endian VLQ byte order.
fn serialize_ergo_long(value: i64) -> String {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    let mut vlq = Vec::new();
    let mut n = zigzag;
    loop {
        let mut byte = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        vlq.push(byte);
        if n == 0 {
            break;
        }
    }
    format!("05{}", hex::encode(vlq))
}

/// Serialize a byte value as Ergo constant (prefix 02).
fn serialize_ergo_byte(value: u8) -> String {
    format!("02{:02x}", value)
}

/// Decode a server-returned box id that may be double hex-encoded.
fn decode_box_id(raw: &str) -> String {
    if raw.len() == 96 {
        if let Ok(bytes) = hex::decode(raw) {
            if bytes.len() == 48 && bytes.iter().all(|b| b.is_ascii_hexdigit()) {
                return String::from_utf8(bytes).unwrap_or_else(|_| raw.to_string());
            }
        }
    }
    raw.to_string()
}

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
        /// Tracker box ID to use as data input (optional; fetched from server if omitted)
        #[arg(long)]
        tracker_box_id: Option<String>,
        /// Wallet change address for fee-input change output (optional; defaults to recipient address)
        #[arg(long)]
        change_address: Option<String>,
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
            tracker_box_id,
            change_address,
        } => {
            generate_redemption_transaction(
                client,
                account_manager,
                &issuer_pubkey,
                &recipient_pubkey,
                amount,
                output_file,
                emergency,
                tracker_box_id,
                change_address,
            )
            .await
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
    tracker_box_id: Option<String>,
    change_address: Option<String>,
) -> Result<()> {
    // Validate public keys
    if hex::decode(issuer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid issuer public key: {}", e))?
        .len()
        != 33
    {
        return Err(anyhow::anyhow!(
            "Issuer public key must be 33 bytes (66 hex characters)"
        ));
    }

    if hex::decode(recipient_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid recipient public key: {}", e))?
        .len()
        != 33
    {
        return Err(anyhow::anyhow!(
            "Recipient public key must be 33 bytes (66 hex characters)"
        ));
    }

    println!("🔍 Retrieving note information...");
    let note = client
        .get_note(issuer_pubkey, recipient_pubkey)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Note not found for issuer {} and recipient {}",
                issuer_pubkey,
                recipient_pubkey
            )
        })?;

    // Verify that the redemption amount does not exceed the note's outstanding debt
    if note.outstanding_debt() < amount {
        return Err(anyhow::anyhow!(
            "Insufficient outstanding debt: {} nanoERG available, {} nanoERG requested",
            note.outstanding_debt(),
            amount
        ));
    }

    println!("🔍 Retrieving issuer's reserve box...");
    let reserves_response = client.get_reserves_by_issuer(issuer_pubkey).await?;
    let reserve_box = reserves_response
        .iter()
        .filter(|r| r.base_info.collateral_amount >= amount)
        .max_by_key(|r| r.base_info.last_updated_height)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No reserve box with sufficient collateral found for issuer {}",
                issuer_pubkey
            )
        })?;

    println!(
        "✅ Selected reserve box: {} (collateral: {} nanoERG, height: {})",
        decode_box_id(&reserve_box.box_id),
        reserve_box.base_info.collateral_amount,
        reserve_box.base_info.last_updated_height
    );

    let reserve_box_id = decode_box_id(&reserve_box.box_id);
    let tracker_nft_id = &reserve_box.base_info.tracker_nft_id;

    let tracker_box_id = if let Some(id) = tracker_box_id {
        println!("✅ Using provided tracker box: {}", &id[..16]);
        id
    } else {
        println!("🔍 Retrieving latest tracker box...");
        let tracker_box_response = client.get_latest_tracker_box_id().await;
        match tracker_box_response {
            Ok(response) => {
                println!("✅ Found tracker box: {}", &response.tracker_box_id[..16]);
                response.tracker_box_id
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "No tracker box found: {}. Cannot generate redemption transaction without a tracker box.",
                    e
                ));
            }
        }
    };

    println!("🔗 Converting public keys to addresses...");
    let recipient_address = pubkey_to_address(recipient_pubkey)?;

    // Fetch the recipient's private key from the node wallet so the node can satisfy
    // the `proveDlog(receiver)` condition in the Basis reserve contract when signing.
    println!("🔍 Fetching recipient private key from node wallet...");
    let recipient_private_key = client.get_private_key(NODE_URL, Some(API_KEY), &recipient_address).await
        .map_err(|e| anyhow::anyhow!("Failed to fetch recipient private key from node wallet. Ensure the recipient address {} is in the wallet: {}", recipient_address, e))?;
    println!("✅ Fetched recipient private key");

    // Get tracker lookup proof for context var #8 from server
    println!("🔍 Retrieving tracker lookup proof from server...");
    let tracker_proof = client
        .get_tracker_proof(issuer_pubkey, recipient_pubkey)
        .await?;
    let total_debt = tracker_proof.total_debt;
    let tracker_lookup_proof = hex::decode(&tracker_proof.proof)
        .map_err(|e| anyhow::anyhow!("Invalid tracker proof hex: {}", e))?;
    let note_timestamp = note.timestamp;

    // Get the reserve proof from the server. For a first redemption the server's
    // in-memory reserve tree is empty, so the insert proof is generated against the
    // empty tree and matches the on-chain reserve R5 register.
    println!("🔍 Retrieving reserve insert proof from server...");
    let reserve_proof = client
        .get_reserve_proof(issuer_pubkey, recipient_pubkey, amount, note_timestamp)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get reserve proof: {}", e))?;

    // Build serialized SAvlTree for R5 register from the updated reserve state digest.
    // Scala's ValueSerializer.serialize(AvlTreeConstant(tree)) produces:
    //   0x64 || 33-byte digest || flags || VLQ key_length || VLQ value_length
    let r5_bytes = build_savl_tree_from_digest(&reserve_proof.new_reserve_state_digest);
    let r5_hex = hex::encode(&r5_bytes);

    // Get the reserve contract P2S address from the server configuration
    println!("🔍 Retrieving reserve contract P2S address from server configuration...");
    let reserve_contract_p2s = client.get_basis_reserve_contract_p2s().await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to retrieve reserve contract P2S address from server: {}",
            e
        )
    })?;

    // The reserve output keeps the full original collateral minus the redeemed amount.
    // The transaction fee is paid by explicit wallet-owned fee inputs, matching the
    // Scala reference implementation. Recipient receives the full redemption amount.
    let reserve_output_value = reserve_box
        .base_info
        .collateral_amount
        .saturating_sub(amount);
    if reserve_output_value == 0 {
        return Err(anyhow::anyhow!(
            "Reserve output value would be zero after redemption"
        ));
    }
    let recipient_output_value = amount;

    // Determine if this is the first redemption
    let is_first_redemption = note.amount_redeemed == 0;
    let (reserve_lookup_proof, reserve_insert_proof) = if is_first_redemption {
        println!("🔍 First redemption - using reserve insert proof...");
        let insert_proof = hex::decode(&reserve_proof.insert_proof)
            .map_err(|e| anyhow::anyhow!("Invalid reserve insert proof hex: {}", e))?;
        (None, insert_proof)
    } else {
        println!("🔍 Using reserve lookup and insert proofs...");
        println!(
            "✅ Got reserve proof: already_redeemed={} nanoERG, is_first={}",
            reserve_proof.already_redeemed, reserve_proof.is_first_redemption
        );
        let lookup_proof = if let Some(proof_hex) = &reserve_proof.proof {
            Some(
                hex::decode(proof_hex)
                    .map_err(|e| anyhow::anyhow!("Invalid reserve lookup proof hex: {}", e))?,
            )
        } else {
            return Err(anyhow::anyhow!(
                "Reserve lookup proof is required for subsequent redemption"
            ));
        };
        let insert_proof = hex::decode(&reserve_proof.insert_proof)
            .map_err(|e| anyhow::anyhow!("Invalid reserve insert proof hex: {}", e))?;
        (lookup_proof, insert_proof)
    };

    // Verify tracker box exists on Ergo node
    println!("🔍 Verifying tracker box on Ergo node...");
    client.get_box_from_node(&tracker_box_id, NODE_URL, Some(API_KEY)).await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve tracker box {} from Ergo node: {}. Cannot generate redemption transaction.", tracker_box_id, e))?;

    // Retrieve the actual reserve box from the Ergo node
    println!("🔍 Retrieving reserve box from Ergo node...");
    let reserve_box_details = client
        .get_box_from_node(&reserve_box_id, NODE_URL, Some(API_KEY))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve reserve box from Ergo node: {}", e))?;

    // The reserve NFT from the input box must be preserved in the reserve output (contract checks selfOut.tokens == SELF.tokens)
    let reserve_nft_id = reserve_box_details
        .assets
        .first()
        .map(|asset| asset.token_id.clone())
        .unwrap_or_else(|| tracker_nft_id.clone());

    // Fetch wallet-owned fee input boxes from the node.
    println!("🔍 Retrieving wallet fee inputs from Ergo node...");
    let wallet_boxes = client
        .get_wallet_boxes(NODE_URL, Some(API_KEY))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve wallet boxes from Ergo node: {}", e))?;

    // Select boxes covering TRANSACTION_FEE, preferring boxes without tokens to avoid
    // complicating the change output. The wallet may include the reserve box if it owns
    // it, so explicitly exclude the reserve box being spent.
    let (fee_input_ids, fee_input_total) = select_fee_inputs(
        &wallet_boxes,
        TRANSACTION_FEE,
        &reserve_box_id,
    )
    .ok_or_else(|| anyhow::anyhow!(
        "No wallet boxes covering {} nanoERG fee found. Ensure the wallet is synced and has at least {} nanoERG available.",
        TRANSACTION_FEE, TRANSACTION_FEE
    ))?;

    println!(
        "✅ Selected {} fee input box(es) totaling {} nanoERG",
        fee_input_ids.len(),
        fee_input_total
    );

    // Determine change address. Default to the recipient's P2PK address.
    let change_address = change_address.unwrap_or_else(|| recipient_address.clone());

    println!("📦 Preparing box IDs for transaction...");

    // Get issuer signature from CLI wallet
    println!("🔑 Signing redemption with issuer key...");
    let current_account = account_manager
        .get_current()
        .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

    // Create signing message matching the deployed reserve contract:
    // key || totalDebt || timestamp (48 bytes)
    // Both issuer and tracker sign the SAME message
    let issuer_pubkey_bytes = hex::decode(issuer_pubkey)?;
    let recipient_pubkey_bytes = hex::decode(recipient_pubkey)?;

    let mut key_hash_input = Vec::new();
    key_hash_input.extend_from_slice(&issuer_pubkey_bytes);
    key_hash_input.extend_from_slice(&recipient_pubkey_bytes);
    let key_hash = blake2b256_hash(&key_hash_input);

    // Create signing message: key || totalDebt || timestamp (48 bytes)
    let mut message = Vec::with_capacity(48);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&total_debt.to_be_bytes());
    message.extend_from_slice(&note_timestamp.to_be_bytes());

    let issuer_signature = current_account.sign_message(&message)?;

    // Get tracker signature from server
    let tracker_signature_response = client
        .request_tracker_signature(
            issuer_pubkey,
            recipient_pubkey,
            total_debt,
            note_timestamp,
            emergency,
        )
        .await?;
    let tracker_signature = hex::decode(&tracker_signature_response.tracker_signature)
        .map_err(|e| anyhow::anyhow!("Invalid tracker signature hex: {}", e))?;

    // Use the reserve insert proof fetched from server
    let insert_proof = reserve_insert_proof.clone();

    // Build context extension map with properly serialized Ergo constants.
    let mut context_extension = HashMap::new();

    // #0: Action byte (Byte constant)
    context_extension.insert("0".to_string(), json!(serialize_ergo_byte(0))); // action = 0, index = 0

    // #1: Receiver pubkey (GroupElement constant)
    context_extension.insert("1".to_string(), json!(format!("07{}", recipient_pubkey)));

    // #2: Reserve signature (Coll[Byte] constant, 65 bytes)
    context_extension.insert(
        "2".to_string(),
        json!(serialize_coll_bytes(&issuer_signature)),
    );

    // #3: Total debt (Long constant)
    context_extension.insert(
        "3".to_string(),
        json!(serialize_ergo_long(total_debt as i64)),
    );

    // #4: Payment timestamp (Long constant, milliseconds since Unix epoch)
    context_extension.insert(
        "4".to_string(),
        json!(serialize_ergo_long(note_timestamp as i64)),
    );

    // #5: Insert proof (Coll[Byte] constant)
    context_extension.insert("5".to_string(), json!(serialize_coll_bytes(&insert_proof)));

    // #6: Tracker signature (Coll[Byte] constant, 65 bytes)
    context_extension.insert(
        "6".to_string(),
        json!(serialize_coll_bytes(&tracker_signature)),
    );

    // #7: Reserve lookup proof (optional, Coll[Byte] constant)
    if let Some(ref proof) = reserve_lookup_proof {
        context_extension.insert("7".to_string(), json!(serialize_coll_bytes(proof)));
    }

    // #8: Tracker lookup proof (Coll[Byte] constant)
    context_extension.insert(
        "8".to_string(),
        json!(serialize_coll_bytes(&tracker_lookup_proof)),
    );

    // Convert output addresses to ergoTree bytes for the signing request.
    let reserve_ergo_tree = address_to_ergo_tree(&reserve_contract_p2s)?;
    let recipient_ergo_tree = address_to_ergo_tree(&recipient_address)?;
    let change_ergo_tree = address_to_ergo_tree(&change_address)?;

    // Get current blockchain height for output creationHeight.
    let current_height = client.get_node_height(NODE_URL, Some(API_KEY)).await?;

    // Fetch serialized box bytes for signing.
    let reserve_box_binary = client
        .get_box_binary(&reserve_box_id, NODE_URL, Some(API_KEY))
        .await?;
    let tracker_box_binary = client
        .get_box_binary(&tracker_box_id, NODE_URL, Some(API_KEY))
        .await?;

    // Build fee input objects with empty extensions, then include their raw bytes.
    // /wallet/transaction/sign expects UnsignedErgoTransaction inputs, which only
    // contain boxId and a top-level extension map (no spendingProof).
    let mut fee_input_json = Vec::new();
    let mut fee_input_binaries = Vec::new();
    for fee_box_id in &fee_input_ids {
        fee_input_json.push(json!({
            "boxId": fee_box_id,
            "extension": serde_json::json!({})
        }));
        let binary = client
            .get_box_binary(fee_box_id, NODE_URL, Some(API_KEY))
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to get binary for fee input {}: {}", fee_box_id, e)
            })?;
        fee_input_binaries.push(binary);
    }

    let mut inputs = vec![json!({
        "boxId": reserve_box_id,
        "extension": context_extension
    })];
    inputs.extend(fee_input_json);

    let change_amount = fee_input_total.saturating_sub(TRANSACTION_FEE);
    let mut outputs = vec![
        json!({
            "value": reserve_output_value,
            "ergoTree": reserve_ergo_tree,
            "creationHeight": current_height,
            "assets": [
                {
                    "tokenId": reserve_nft_id,
                    "amount": 1
                }
            ],
            "additionalRegisters": {
                "R4": format!("07{}", issuer_pubkey),
                "R5": r5_hex,
                "R6": format!("0e{:02x}{}", tracker_nft_id.len() / 2, tracker_nft_id)
            }
        }),
        json!({
            "value": recipient_output_value,
            "ergoTree": recipient_ergo_tree,
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {}
        }),
        json!({
            "value": TRANSACTION_FEE,
            // Fee recipient contract from Scala reference.
            "ergoTree": "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304",
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {}
        }),
    ];

    if change_amount > 0 {
        outputs.push(json!({
            "value": change_amount,
            "ergoTree": change_ergo_tree,
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {}
        }));
    }

    let mut inputs_raw = vec![reserve_box_binary];
    inputs_raw.extend(fee_input_binaries);

    // Build the unsigned transaction in the format expected by /wallet/transaction/sign.
    // CRITICAL: the reserve output must be at index 0 because the contract uses
    //   index = getVar[Byte](0) % 10
    // and action byte 0x00 gives index 0. The recipient output comes second.
    let transaction_json = json!({
        "tx": {
            "inputs": inputs,
            "dataInputs": [
                {
                    "boxId": tracker_box_id
                }
            ],
            "outputs": outputs
        },
        "inputsRaw": inputs_raw,
        "dataInputsRaw": [
            tracker_box_binary
        ],
        "secrets": {
            "dlog": [recipient_private_key]
        }
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
    println!("   Recipient receives: {} nanoERG", recipient_output_value);
    println!("   Reserve output value: {} nanoERG", reserve_output_value);
    println!("   Total debt: {} nanoERG", total_debt);
    println!("   Already redeemed: {} nanoERG", note.amount_redeemed);
    println!("   Reserve box ID: {}", reserve_box_id);
    println!("   Tracker box ID: {}", tracker_box_id);
    println!("   Transaction fee: {} nanoERG", TRANSACTION_FEE);
    println!(
        "   Fee inputs: {} box(es), {} nanoERG total",
        fee_input_ids.len(),
        fee_input_total
    );
    if change_amount > 0 {
        println!(
            "   Change output: {} nanoERG to {}",
            change_amount, change_address
        );
    }
    println!("   Emergency redemption: {}", emergency);
    println!("   First redemption: {}", is_first_redemption);
    println!("📝 Context Extension Variables:");
    println!("   #0 (action): 0x00 (redemption, reserve output index 0)");
    println!("   #1 (receiver): {}", recipient_pubkey);
    println!("   #2 (reserveSig): {} bytes", issuer_signature.len());
    println!("   #3 (totalDebt): {}", total_debt);
    println!("   #5 (insertProof): {} bytes", insert_proof.len());
    println!("   #6 (trackerSig): {} bytes", tracker_signature.len());
    if let Some(ref proof) = reserve_lookup_proof {
        println!(
            "   #7 (reserveLookupProof): {} bytes",
            proof.as_slice().len()
        );
    } else {
        println!("   #7 (reserveLookupProof): omitted (first redemption)");
    }
    println!(
        "   #8 (trackerLookupProof): {} bytes",
        tracker_lookup_proof.len()
    );

    Ok(())
}

/// Select wallet boxes covering the required fee amount.
///
/// Prefer boxes without tokens and exclude the reserve box being spent (the wallet
/// may report it if it has been added to a scan).
fn select_fee_inputs(
    wallet_boxes: &[crate::api::ErgoBoxDetails],
    required: u64,
    reserve_box_id: &str,
) -> Option<(Vec<String>, u64)> {
    // Filter out the reserve box and any boxes carrying tokens.
    let mut candidates: Vec<_> = wallet_boxes
        .iter()
        .filter(|b| b.box_id != reserve_box_id && b.assets.is_empty())
        .collect();

    // Sort by value ascending so we can try to use the smallest sufficient boxes.
    candidates.sort_by_key(|b| b.value);

    // First try to find a single box that covers the fee exactly or with minimal overage.
    if let Some(box_) = candidates.iter().find(|b| b.value >= required) {
        return Some((vec![box_.box_id.clone()], box_.value));
    }

    // Otherwise accumulate boxes until the fee is covered.
    let mut selected = Vec::new();
    let mut total = 0u64;
    for box_ in candidates {
        total += box_.value;
        selected.push(box_.box_id.clone());
        if total >= required {
            return Some((selected, total));
        }
    }

    None
}

/// Build serialized SAvlTree constant matching Scala's
/// `ValueSerializer.serialize(AvlTreeConstant(tree))`.
/// Format: 0x64 || 33-byte digest || flags || VLQ key_length || VLQ value_length
fn build_savl_tree_from_digest(digest_hex: &str) -> Vec<u8> {
    let digest_bytes = hex::decode(digest_hex).unwrap_or_else(|_| vec![0u8; 33]);

    // The server returns a 33-byte root digest: 32-byte AVL digest + 1-byte flags.
    // Scala's SAvlTree serialization is: type byte 0x64 || 32-byte digest || flags
    // || VLQ key_length || VLQ value_length.
    let root_digest: Vec<u8> = if digest_bytes.len() >= 33 {
        digest_bytes[..33].to_vec()
    } else {
        // Pad short digests (should not happen in practice)
        let mut padded = vec![0u8; 33];
        padded[..digest_bytes.len()].copy_from_slice(&digest_bytes);
        padded
    };

    let mut r5_bytes = Vec::with_capacity(38);
    r5_bytes.push(0x64u8); // SAvlTree type byte
    r5_bytes.extend_from_slice(&root_digest); // 33-byte digest from the AVL prover
    r5_bytes.push(0x01u8); // flags: insertions allowed
    r5_bytes.extend_from_slice(&vlq_encode(32)); // key length
    r5_bytes.extend_from_slice(&vlq_encode(0)); // value length: variable (0)

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
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

    let pubkey_bytes =
        hex::decode(pubkey_hex).map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;

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

// Helper function to convert a P2S or P2PK address string to its hex-encoded ergoTree.
fn address_to_ergo_tree(address_str: &str) -> Result<String> {
    use ergo_lib::ergotree_ir::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let address = encoder
        .parse_address_from_str(address_str)
        .map_err(|e| anyhow::anyhow!("Invalid address '{}': {}", address_str, e))?;
    let tree = address.script().map_err(|e| {
        anyhow::anyhow!("Failed to get script for address '{}': {}", address_str, e)
    })?;
    Ok(hex::encode(tree.sigma_serialize_bytes()))
}
