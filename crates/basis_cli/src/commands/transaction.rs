use crate::api::TrackerClient;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use std::collections::HashMap;
use std::fs;

use ergo_lib::chain::ergo_state_context::ErgoStateContext;
use ergo_lib::chain::parameters::Parameters;
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::wallet::secret_key::SecretKey;

use basis_offchain::signing::{add_input_proof, redemption_signing_message};

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

/// Serialize bytes as an Ergo `Coll[Byte]` constant: type prefix `0x0e` + VLQ(length) + data.
/// The length is VLQ-encoded (not a single byte), so collections of 128 bytes or more — e.g. larger
/// AVL lookup/insert proofs — are encoded correctly.
fn serialize_coll_bytes(data: &[u8]) -> String {
    let mut bytes = vec![0x0e];
    bytes.extend_from_slice(&vlq_encode(data.len()));
    bytes.extend_from_slice(data);
    hex::encode(bytes)
}

/// Serialize a long value as an Ergo `Long` constant: type prefix `0x05` + zigzag(VLQ),
/// using Ergo's little-endian VLQ byte order.
fn serialize_ergo_long(value: i64) -> String {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    format!("05{}", hex::encode(vlq_encode(zigzag as usize)))
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
        /// Sign the redemption locally with ergo-lib (client-side proveDlog for both inputs)
        /// and broadcast it, instead of emitting an unsigned transaction for the node wallet.
        #[arg(long, default_value = "false")]
        local_sign: bool,
        /// Recipient (receiver) dlog secret as 32-byte hex. Required for local signing of the
        /// reserve input's proveDlog(receiver). If omitted, fetched from the node wallet.
        #[arg(long)]
        recipient_secret: Option<String>,
        /// Fee-payer dlog secret as 32-byte hex. Used to sign the fee input locally. If omitted,
        /// fetched from the node wallet for the change/fee address.
        #[arg(long)]
        fee_secret: Option<String>,
    },
    /// Tracker-assisted redemption: the tracker builds the unsigned transaction and signs the fee
    /// input(s) (POST /redemption/build); the CLI signs the issuer message with the current account
    /// and adds the reserve input's proveDlog(recipient) proof, then submits (POST
    /// /redemption/submit). This exercises the new 2-phase server endpoints end-to-end.
    RedeemAssisted {
        /// Issuer public key (hex). The current account should be this issuer.
        #[arg(long)]
        issuer_pubkey: String,
        /// Recipient (receiver) public key (hex).
        #[arg(long)]
        recipient_pubkey: String,
        /// Redemption amount in nanoERG
        #[arg(long)]
        amount: u64,
        /// Recipient (receiver) dlog secret as 32-byte hex for the reserve input's
        /// proveDlog(receiver). If omitted, fetched from the node wallet.
        #[arg(long)]
        recipient_secret: Option<String>,
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
            local_sign,
            recipient_secret,
            fee_secret,
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
                local_sign,
                recipient_secret,
                fee_secret,
            )
            .await
        }
        TransactionCommands::RedeemAssisted {
            issuer_pubkey,
            recipient_pubkey,
            amount,
            recipient_secret,
        } => {
            redeem_tracker_assisted(
                client,
                account_manager,
                &issuer_pubkey,
                &recipient_pubkey,
                amount,
                recipient_secret,
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
    local_sign: bool,
    recipient_secret: Option<String>,
    fee_secret: Option<String>,
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
    // Pick the smallest reserve that can cover the redemption while leaving a valid (>= min box
    // value) post-redemption reserve output, so we don't drain a larger reserve or leave a zero box.
    const MIN_RESERVE_REMAINDER: u64 = 1_000_000; // 0.001 ERG min box value
    let required = amount.saturating_add(MIN_RESERVE_REMAINDER);
    let reserve_box = reserves_response
        .iter()
        .filter(|r| r.base_info.collateral_amount >= required)
        .min_by(|a, b| {
            a.base_info
                .collateral_amount
                .cmp(&b.base_info.collateral_amount)
                .then_with(|| b.base_info.last_updated_height.cmp(&a.base_info.last_updated_height))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No reserve box with sufficient collateral found for issuer {} (need >= {} nanoERG)",
                issuer_pubkey,
                required
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

    // Fetch the recipient's private key so the node can satisfy the `proveDlog(receiver)`
    // condition in the Basis reserve contract when signing. If the caller supplied the secret
    // explicitly (e.g. for local-signing with an external recipient), use it instead of querying
    // the node wallet.
    println!("🔍 Resolving recipient private key...");
    let recipient_private_key = match recipient_secret.as_ref() {
        Some(secret) => {
            println!("✅ Using provided recipient secret");
            secret.clone()
        }
        None => {
            println!("🔍 Fetching recipient private key from node wallet...");
            let key = client.get_private_key(NODE_URL, Some(API_KEY), &recipient_address).await
                .map_err(|e| anyhow::anyhow!("Failed to fetch recipient private key from node wallet. Ensure the recipient address {} is in the wallet: {}", recipient_address, e))?;
            println!("✅ Fetched recipient private key");
            key
        }
    };

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

    // Preserve the refund initiation height from the spent reserve box's R7 register.
    let refund_initiation_height = basis_store::ergo_scanner::decode_ergo_long_register(
        reserve_box_details.additional_registers.get("R7"),
    );

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

    // Determine change address. Default to the address that owns the first selected fee-input box
    // (so the locally-fetched fee-payer secret matches the box we actually sign); fall back to the
    // recipient's P2PK address if it can't be derived.
    let change_address = change_address.unwrap_or_else(|| {
        wallet_boxes
            .iter()
            .find(|b| b.box_id == fee_input_ids[0])
            .and_then(|b| ergo_tree_to_p2pk_address(&b.ergo_tree).ok())
            .unwrap_or_else(|| recipient_address.clone())
    });

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

    // Build context extension map with properly serialized Ergo constants, as hex-encoded constant
    // strings embedded into `inputs[0].extension` of the unsigned transaction JSON.
    let mut context_extension: HashMap<String, String> = HashMap::new();

    // #0: Action byte (Byte constant)
    context_extension.insert("0".to_string(), serialize_ergo_byte(0)); // action = 0, index = 0

    // #1: Receiver pubkey (GroupElement constant)
    context_extension.insert("1".to_string(), format!("07{}", recipient_pubkey));

    // #2: Reserve signature (Coll[Byte] constant, 65 bytes)
    context_extension.insert("2".to_string(), serialize_coll_bytes(&issuer_signature));

    // #3: Total debt (Long constant)
    context_extension.insert("3".to_string(), serialize_ergo_long(total_debt as i64));

    // #4: Payment timestamp (Long constant, milliseconds since Unix epoch)
    context_extension.insert("4".to_string(), serialize_ergo_long(note_timestamp as i64));

    // #5: Insert proof (Coll[Byte] constant)
    context_extension.insert("5".to_string(), serialize_coll_bytes(&insert_proof));

    // #6: Tracker signature (Coll[Byte] constant, 65 bytes)
    context_extension.insert("6".to_string(), serialize_coll_bytes(&tracker_signature));

    // #7: Reserve lookup proof (optional, Coll[Byte] constant)
    if let Some(ref proof) = reserve_lookup_proof {
        context_extension.insert("7".to_string(), serialize_coll_bytes(proof));
    }

    // #8: Tracker lookup proof (Coll[Byte] constant)
    context_extension.insert("8".to_string(), serialize_coll_bytes(&tracker_lookup_proof));

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
                "R6": format!("0e{:02x}{}", tracker_nft_id.len() / 2, tracker_nft_id),
                "R7": serialize_ergo_long(refund_initiation_height as i64)
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

    let mut inputs_raw = vec![reserve_box_binary.clone()];
    inputs_raw.extend(fee_input_binaries.clone());

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

    if local_sign {
        return sign_and_broadcast_local(SignLocalParams {
            client,
            issuer_pubkey,
            recipient_pubkey,
            amount,
            total_debt,
            note_timestamp,
            is_first_redemption,
            emergency,
            reserve_box_id: &reserve_box_id,
            tracker_box_id: &tracker_box_id,
            reserve_output_value,
            recipient_output_value,
            change_amount,
            unsigned_tx: transaction_json["tx"].clone(),
            reserve_box_binary: &reserve_box_binary,
            fee_input_binaries: &fee_input_binaries,
            tracker_box_binary: &tracker_box_binary,
            issuer_signature_len: issuer_signature.len(),
            tracker_signature_len: tracker_signature.len(),
            insert_proof_len: insert_proof.len(),
            reserve_lookup_proof_len: reserve_lookup_proof.as_ref().map(|p| p.len()),
            tracker_lookup_proof_len: tracker_lookup_proof.len(),
            fee_input_count: fee_input_ids.len(),
            fee_input_total,
            change_address: &change_address,
            recipient_address: &recipient_address,
            recipient_secret,
            fee_secret,
        })
        .await;
    }

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

/// Tracker-assisted redemption driver (mirrors the TUI flow). The tracker builds the unsigned
/// transaction and signs the fee input(s); the CLI signs the issuer message with the current
/// account and adds the reserve input's proveDlog(recipient) proof, then submits via the tracker.
async fn redeem_tracker_assisted(
    client: &TrackerClient,
    account_manager: &crate::account::AccountManager,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    recipient_secret: Option<String>,
) -> Result<()> {
    use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
    use ergo_lib::chain::transaction::Transaction;
    use ergo_lib::ergo_chain_types::{Header, PreHeader};

    let issuer_pk: [u8; 33] = hex::decode(issuer_pubkey)
        .map_err(|e| anyhow::anyhow!("issuer hex: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("issuer pubkey must be 33 bytes"))?;
    let recipient_pk: [u8; 33] = hex::decode(recipient_pubkey)
        .map_err(|e| anyhow::anyhow!("recipient hex: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("recipient pubkey must be 33 bytes"))?;

    println!("🔍 Fetching note and tracker proof...");
    let note = client
        .get_note(issuer_pubkey, recipient_pubkey)
        .await?
        .ok_or_else(|| anyhow::anyhow!("note not found for issuer/recipient"))?;
    let total_debt = client
        .get_tracker_proof(issuer_pubkey, recipient_pubkey)
        .await?
        .total_debt;
    let timestamp = note.timestamp;
    println!("✅ total_debt={} timestamp={}", total_debt, timestamp);

    // Issuer signs the redemption message with the current account.
    let current = account_manager
        .get_current()
        .ok_or_else(|| anyhow::anyhow!("no current account (should be the issuer)"))?;
    if current.get_pubkey_hex() != issuer_pubkey {
        println!(
            "⚠️  current account {}... != issuer {}...; signature may be rejected",
            &current.get_pubkey_hex()[..16],
            &issuer_pubkey[..16]
        );
    }
    let message = redemption_signing_message(&issuer_pk, &recipient_pk, total_debt, timestamp);
    let issuer_sig = current.sign_message(&message)?;
    println!("✅ issuer signature ({} bytes)", issuer_sig.len());

    // Tracker builds the unsigned tx and signs the fee input(s).
    println!("🔍 Requesting tracker build (POST /redemption/build)...");
    let build = client
        .redemption_build(crate::api::RedemptionBuildRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            amount,
            timestamp,
            issuer_signature: hex::encode(issuer_sig),
            emergency: false,
            tracker_box_id: None,
            change_address: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("tracker build failed: {}", e))?;
    println!(
        "✅ build ok: reserve {} (out {} nanoERG), fee {} nanoERG, change {} to {}",
        &build.reserve_box_id[..16.min(build.reserve_box_id.len())],
        build.reserve_output_value,
        build.fee,
        build.change_amount,
        build.change_address
    );

    // Reconstruct signing material.
    let unsigned: UnsignedTransaction = serde_json::from_value(build.unsigned_tx)
        .map_err(|e| anyhow::anyhow!("parse unsigned tx: {}", e))?;
    let partial: Transaction = serde_json::from_value(build.partial_tx)
        .map_err(|e| anyhow::anyhow!("parse partial tx: {}", e))?;
    let parse_box = |h: &str| -> Result<ErgoBox> {
        let bytes = hex::decode(h).map_err(|e| anyhow::anyhow!("box hex: {}", e))?;
        ErgoBox::sigma_parse_bytes(&bytes).map_err(|e| anyhow::anyhow!("box parse: {:?}", e))
    };
    let mut input_boxes = Vec::with_capacity(build.input_box_binaries.len());
    for h in &build.input_box_binaries {
        input_boxes.push(parse_box(h)?);
    }
    let mut data_boxes = Vec::with_capacity(build.data_box_binaries.len());
    for h in &build.data_box_binaries {
        data_boxes.push(parse_box(h)?);
    }
    if build.headers.len() < 10 {
        return Err(anyhow::anyhow!(
            "tracker returned {} headers (need 10)",
            build.headers.len()
        ));
    }
    let pre_header = PreHeader::from(build.headers[0].clone());
    let headers: [Header; 10] = build.headers[..10]
        .to_vec()
        .try_into()
        .map_err(|_| anyhow::anyhow!("headers array"))?;

    // Recipient (receiver) secret for the reserve input's proveDlog(recipient).
    let recipient_address = pubkey_to_address(recipient_pubkey)?;
    let receiver_secret =
        resolve_dlog_secret(&recipient_secret, client, &recipient_address, "recipient").await?;
    let receiver_sk = SecretKey::dlog_from_bytes(&receiver_secret)
        .ok_or_else(|| anyhow::anyhow!("invalid recipient dlog secret"))?;

    // Add the reserve input (index 0) proof over the same bytes_to_sign.
    println!("🖊️  Adding reserve proveDlog(recipient) proof...");
    let signed = add_input_proof(
        &unsigned,
        Some(&partial),
        &input_boxes,
        &data_boxes,
        &pre_header,
        &headers,
        0,
        &receiver_sk,
    )
    .map_err(|e| anyhow::anyhow!("reserve proof failed: {:?}", e))?;

    let tx_json = serde_json::to_value(&signed)?;
    let local_id = tx_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>")
        .to_string();
    println!("✅ signed tx id: {}", local_id);

    println!("📡 Submitting via tracker (POST /redemption/submit)...");
    let tx_id = client
        .redemption_submit(
            tx_json,
            issuer_pubkey,
            recipient_pubkey,
            amount,
            build.new_already_redeemed,
        )
        .await
        .map_err(|e| anyhow::anyhow!("submit failed: {}", e))?;
    println!("✅ Redemption broadcast. Transaction ID: {}", tx_id);
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
    r5_bytes.push(0x03u8); // flags: insertions and updates allowed (insertOrUpdate contract)
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
    use ergo_lib::ergo_chain_types::EcPoint;
    use ergo_lib::ergotree_ir::chain::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
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
    let encoder =
        ergo_lib::ergotree_ir::chain::address::AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}

// Helper function to convert a P2S or P2PK address string to its hex-encoded ergoTree.
fn address_to_ergo_tree(address_str: &str) -> Result<String> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let address = encoder
        .parse_address_from_str(address_str)
        .map_err(|e| anyhow::anyhow!("Invalid address '{}': {}", address_str, e))?;
    let tree = address.script().map_err(|e| {
        anyhow::anyhow!("Failed to get script for address '{}': {}", address_str, e)
    })?;
    Ok(hex::encode(tree.sigma_serialize_bytes().map_err(|e| {
        anyhow::anyhow!("Failed to serialize ergoTree: {:?}", e)
    })?))
}

/// Derive the P2PK address from a P2PK (proveDlog) ergoTree (hex). Used to find the address that
/// owns a fee-input box so the correct local secret is used to sign it. A P2PK ergoTree serializes
/// as `00 08 cd <33-byte compressed point>`, so the public key is bytes `[3..36]`.
fn ergo_tree_to_p2pk_address(ergo_tree_hex: &str) -> Result<String> {
    use ergo_lib::ergo_chain_types::EcPoint;
    use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;

    let bytes = hex::decode(ergo_tree_hex)?;
    if bytes.len() != 36 || bytes[0] != 0x00 || bytes[1] != 0x08 || bytes[2] != 0xcd {
        return Err(anyhow::anyhow!("ergoTree is not a P2PK (proveDlog) script"));
    }
    let point = EcPoint::sigma_parse_bytes(&bytes[3..])
        .map_err(|e| anyhow::anyhow!("failed to parse P2PK point: {:?}", e))?;
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&Address::P2Pk(ProveDlog::from(point))))
}

/// Parameters for the local (client-side) redemption signing path.
struct SignLocalParams<'a> {
    client: &'a TrackerClient,
    issuer_pubkey: &'a str,
    recipient_pubkey: &'a str,
    amount: u64,
    total_debt: u64,
    note_timestamp: u64,
    is_first_redemption: bool,
    emergency: bool,
    reserve_box_id: &'a str,
    tracker_box_id: &'a str,
    reserve_output_value: u64,
    recipient_output_value: u64,
    change_amount: u64,
    /// Node-canonical unsigned transaction JSON (the `tx` object from the proven builder).
    /// Parsed into an ergo-lib `UnsignedTransaction` so `bytes_to_sign` matches the node exactly.
    unsigned_tx: serde_json::Value,
    reserve_box_binary: &'a str,
    fee_input_binaries: &'a [String],
    tracker_box_binary: &'a str,
    issuer_signature_len: usize,
    tracker_signature_len: usize,
    insert_proof_len: usize,
    reserve_lookup_proof_len: Option<usize>,
    tracker_lookup_proof_len: usize,
    fee_input_count: usize,
    fee_input_total: u64,
    change_address: &'a str,
    recipient_address: &'a str,
    recipient_secret: Option<String>,
    fee_secret: Option<String>,
}

async fn resolve_dlog_secret(
    provided: &Option<String>,
    client: &TrackerClient,
    address: &str,
    label: &str,
) -> Result<[u8; 32]> {
    let hexstr = match provided {
        Some(h) => h.clone(),
        None => {
            println!(
                "🔑 Fetching {} private key from node wallet ({})...",
                label, address
            );
            client
                .get_private_key(NODE_URL, Some(API_KEY), address)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "failed to fetch {} private key for {} (provide --{}-secret to override): {}",
                        label,
                        address,
                        label.replace(' ', "-"),
                        e
                    )
                })?
        }
    };
    let bytes = hex::decode(hexstr.trim())
        .map_err(|e| anyhow::anyhow!("{} secret is not valid hex: {}", label, e))?;
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "{} secret must be 32 bytes, got {}",
            label,
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_broadcast_tx_id(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        match value {
            serde_json::Value::String(s) => return s,
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(id)) = map.get("id") {
                    return id.clone();
                }
            }
            _ => {}
        }
    }
    body.trim().trim_matches('"').to_string()
}

/// Build an [`ErgoStateContext`] for local signing from the node's current view of the chain.
///
/// ergo-lib 0.28 requires the last 10 block headers plus chain parameters to construct a state
/// context. The Basis reserve contract does not read `CONTEXT.headers` or chain parameters, so the
/// parameters are left empty and the headers are only used to derive a valid `PreHeader` (height).
async fn fetch_state_context() -> Result<(ErgoStateContext, PreHeader, [Header; 10])> {
    let headers_url = format!("{}/blocks/lastHeaders/10", NODE_URL);
    let headers_resp = ureq::get(&headers_url)
        .set("api_key", API_KEY)
        .call()
        .map_err(|e| anyhow::anyhow!("failed to fetch last headers: {}", e))?;
    let headers_json: serde_json::Value = headers_resp
        .into_json()
        .map_err(|e| anyhow::anyhow!("failed to read headers response: {}", e))?;
    let headers: Vec<Header> = serde_json::from_value(headers_json)
        .map_err(|e| anyhow::anyhow!("failed to parse headers from node: {}", e))?;
    if headers.len() < 10 {
        return Err(anyhow::anyhow!(
            "need at least 10 headers for state context, got {}",
            headers.len()
        ));
    }

    let pre_header = PreHeader::from(headers[0].clone());
    let headers_array: [Header; 10] = headers
        .into_iter()
        .take(10)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| anyhow::anyhow!("failed to collect headers into [Header; 10]"))?;
    let parameters = Parameters {
        parameters_table: HashMap::new(),
    };

    let ctx = ErgoStateContext::new(pre_header.clone(), headers_array.clone(), parameters);
    Ok((ctx, pre_header, headers_array))
}

/// Scala `immutable.HashMap` (HashTrieMap, Scala 2.12) iteration order for the given byte keys.
/// This is exactly the order `sigma.interpreter.ContextExtension` (a `Map[Byte, Constant]`) is
/// serialized in (`obj.values.foreach`). `UnsignedInput` is `boxId ++ extension`, so the extension
/// byte order is part of `bytes_to_sign`; ergo-lib serializes its insertion-ordered map as-is, hence
/// the keys must be inserted in this order or the local `bytes_to_sign` diverges from the node and
/// `proveDlog(receiver)` verification fails. Validated against a node-signed redemption for the
/// first-redemption set `{0,1,2,3,4,5,6,8}` (order `0,5,1,6,2,3,8,4`); the same model yields the
/// order for the full set `{0..8}` used by subsequent redemptions (which add `#7`).
fn scala_context_extension_order(keys: &[u8]) -> Vec<u8> {
    // `Byte.hashCode` widens the byte to Int; `improve` is Scala HashMap's hash mixing (Murmur3-style).
    fn improve(h: u32) -> u32 {
        let h = h.wrapping_add(!(h.wrapping_shl(9)));
        let h = h ^ (h >> 14);
        let h = h.wrapping_add(h.wrapping_shl(4));
        h ^ (h >> 10)
    }
    fn build(level: u32, keys: &[u8], out: &mut Vec<u8>) {
        use std::collections::BTreeMap;
        // Group by the 5-bit trie index at this level; BTreeMap yields ascending bucket order,
        // matching the bitmap/elems array iteration of HashTrieMap.
        let mut groups: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
        for &k in keys {
            let idx = (improve(k as u32) >> level) & 0x1f;
            groups.entry(idx).or_default().push(k);
        }
        for (_idx, group) in groups {
            if group.len() == 1 {
                out.push(group[0]);
            } else if level >= 30 {
                // HashMapCollision1: iterates its list in insertion order.
                out.extend(group);
            } else {
                build(level + 5, &group, out);
            }
        }
    }
    let mut out = Vec::new();
    build(0, keys, &mut out);
    out
}

/// Reorder the reserve input's (`inputs[0]`) context-extension keys in the JSON so that ergo-lib
/// parses them into its insertion-ordered map in Scala's `ContextExtension` serialization order,
/// for whichever variable indices are present (first-redemption set without `#7`, or the full set
/// `{0..8}` for subsequent redemptions).
fn reorder_reserve_extension_scala(tx: &mut serde_json::Value) {
    use serde_json::Map;
    let ext = match tx
        .get_mut("inputs")
        .and_then(|i| i.get_mut(0))
        .and_then(|inp| inp.get_mut("extension"))
        .and_then(|e| e.as_object_mut())
    {
        Some(e) => e,
        None => return,
    };
    let present: Vec<u8> = ext.keys().filter_map(|k| k.parse::<u8>().ok()).collect();
    let order = scala_context_extension_order(&present);
    let mut reordered: Map<String, serde_json::Value> = Map::new();
    for k in &order {
        let ks = k.to_string();
        if let Some(v) = ext.remove(&ks) {
            reordered.insert(ks, v);
        }
    }
    let remaining: Vec<String> = ext.keys().cloned().collect();
    for k in remaining {
        if let Some(v) = ext.remove(&k) {
            reordered.insert(k, v);
        }
    }
    *ext = reordered;
}

/// Build the redemption transaction with ergo-lib and sign it locally (client-side), producing
/// the `proveDlog` proofs for BOTH the reserve input (receiver) and the fee input (fee payer)
/// in-process via a `Wallet`/`TestProver`, then broadcast the fully-signed transaction to the
/// node. This mirrors what a TUI/client does without delegating signing to the node wallet.
#[allow(clippy::too_many_arguments)]
async fn sign_and_broadcast_local(p: SignLocalParams<'_>) -> Result<()> {
    println!("🖊️  Signing redemption locally (client-side proveDlog for both inputs)...");

    // Parse the proven builder's unsigned transaction. Before parsing, reorder the reserve input's
    // context-extension keys into Scala's `ContextExtension` serialization order: the extension is
    // part of `bytes_to_sign` and Scala serializes its map in index (not insertion) order, so signing
    // requires the exact same order or `proveDlog(receiver)` verification fails on the node.
    let mut unsigned_tx = p.unsigned_tx;
    reorder_reserve_extension_scala(&mut unsigned_tx);
    let unsigned: UnsignedTransaction = serde_json::from_value(unsigned_tx)
        .map_err(|e| anyhow::anyhow!("failed to parse unsigned transaction: {}", e))?;

    // Parse input boxes from their sigma-serialized bytes (fetched via /utxo/byIdBinary).
    let reserve_box =
        ErgoBox::sigma_parse_bytes(&hex::decode(p.reserve_box_binary)?).map_err(|e| {
            anyhow::anyhow!("failed to parse reserve box {}: {:?}", p.reserve_box_id, e)
        })?;
    let tracker_box =
        ErgoBox::sigma_parse_bytes(&hex::decode(p.tracker_box_binary)?).map_err(|e| {
            anyhow::anyhow!("failed to parse tracker box {}: {:?}", p.tracker_box_id, e)
        })?;
    let mut fee_boxes = Vec::with_capacity(p.fee_input_binaries.len());
    for (i, fb) in p.fee_input_binaries.iter().enumerate() {
        let b = ErgoBox::sigma_parse_bytes(&hex::decode(fb)?)
            .map_err(|e| anyhow::anyhow!("failed to parse fee input box {}: {:?}", i, e))?;
        fee_boxes.push(b);
    }

    let mut boxes_to_spend = vec![reserve_box];
    boxes_to_spend.extend(fee_boxes);
    let data_boxes = vec![tracker_box];

    // Resolve the two dlog secrets (receiver + fee payer) and sign locally.
    let recipient_secret = resolve_dlog_secret(
        &p.recipient_secret,
        p.client,
        p.recipient_address,
        "recipient",
    )
    .await?;
    let fee_secret =
        resolve_dlog_secret(&p.fee_secret, p.client, p.change_address, "fee-payer").await?;
    let recipient_sk = SecretKey::dlog_from_bytes(&recipient_secret)
        .ok_or_else(|| anyhow::anyhow!("invalid recipient dlog secret"))?;
    let fee_sk = SecretKey::dlog_from_bytes(&fee_secret)
        .ok_or_else(|| anyhow::anyhow!("invalid fee-payer dlog secret"))?;

    let (_state_context, pre_header, headers) = fetch_state_context().await?;

    // Two-phase single-input signing (tracker-assisted pattern): sign every fee input first
    // (indices 1..N), then the reserve input (index 0), chaining `base_signed` so each previously
    // produced proof is preserved. This mirrors the TUI flow where the tracker (fee) and the
    // receiver (reserve) sign independently over the same `bytes_to_sign`.
    let n_inputs = unsigned.inputs.len();
    let mut partial: Option<ergo_lib::chain::transaction::Transaction> = None;
    for fee_idx in 1..n_inputs {
        partial = Some(
            add_input_proof(
                &unsigned,
                partial.as_ref(),
                &boxes_to_spend,
                &data_boxes,
                &pre_header,
                &headers,
                fee_idx,
                &fee_sk,
            )
            .map_err(|e| anyhow::anyhow!("fee input {} signing failed: {:?}", fee_idx, e))?,
        );
    }
    let signed = add_input_proof(
        &unsigned,
        partial.as_ref(),
        &boxes_to_spend,
        &data_boxes,
        &pre_header,
        &headers,
        0,
        &recipient_sk,
    )
    .map_err(|e| anyhow::anyhow!("reserve input signing failed: {:?}", e))?;

    // Broadcast the fully-signed transaction to the node.
    let signed_json = serde_json::to_value(&signed)?;
    let local_id = signed_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>")
        .to_string();
    println!("✅ Signed locally. tx id: {}", local_id);
    let _ = fs::write(
        "/tmp/last_signed_tx.json",
        serde_json::to_string_pretty(&signed_json)?,
    );
    let url = format!("{}/transactions", NODE_URL);
    let result = ureq::post(&url)
        .set("api_key", API_KEY)
        .send_json(signed_json);
    let body = match result {
        Ok(resp) => resp
            .into_string()
            .map_err(|e| anyhow::anyhow!("failed to read broadcast response: {}", e))?,
        Err(ureq::Error::Status(code, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            return Err(anyhow::anyhow!(
                "broadcast rejected by node (HTTP {}): {}",
                code,
                err_body
            ));
        }
        Err(e) => return Err(anyhow::anyhow!("broadcast to {} failed: {}", url, e)),
    };
    let tx_id = parse_broadcast_tx_id(&body);

    println!("✅ Redemption broadcast with LOCAL proveDlog signatures.");
    println!("📋 Transaction details:");
    println!("   Transaction ID: {}", tx_id);
    println!("   Issuer: {}", p.issuer_pubkey);
    println!("   Recipient: {}", p.recipient_pubkey);
    println!("   Redemption amount: {} nanoERG", p.amount);
    println!(
        "   Recipient receives: {} nanoERG",
        p.recipient_output_value
    );
    println!(
        "   Reserve output value: {} nanoERG",
        p.reserve_output_value
    );
    println!("   Total debt: {} nanoERG", p.total_debt);
    println!("   First redemption: {}", p.is_first_redemption);
    println!("   Emergency redemption: {}", p.emergency);
    println!("   Reserve box ID: {}", p.reserve_box_id);
    println!("   Tracker box ID: {}", p.tracker_box_id);
    println!("   Transaction fee: {} nanoERG", TRANSACTION_FEE);
    println!(
        "   Fee inputs: {} box(es), {} nanoERG total",
        p.fee_input_count, p.fee_input_total
    );
    if p.change_amount > 0 {
        println!(
            "   Change output: {} nanoERG to {}",
            p.change_amount, p.change_address
        );
    }
    println!("📝 Context Extension Variables:");
    println!("   #0 (action): 0x00 (redemption, reserve output index 0)");
    println!("   #1 (receiver): {}", p.recipient_pubkey);
    println!("   #2 (reserveSig): {} bytes", p.issuer_signature_len);
    println!("   #3 (totalDebt): {}", p.total_debt);
    println!("   #4 (timestamp): {}", p.note_timestamp);
    println!("   #5 (insertProof): {} bytes", p.insert_proof_len);
    println!("   #6 (trackerSig): {} bytes", p.tracker_signature_len);
    match p.reserve_lookup_proof_len {
        Some(len) => println!("   #7 (reserveLookupProof): {} bytes", len),
        None => println!("   #7 (reserveLookupProof): omitted (first redemption)"),
    }
    println!(
        "   #8 (trackerLookupProof): {} bytes",
        p.tracker_lookup_proof_len
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    /// A valid P2PK (proveDlog) ergoTree as returned by the node for a wallet address.
    const P2PK_TREE: &str =
        "0008cd02725e8878d5198ca7f5853dddf35560ddab05ab0a26adae7e664b84162c9962e5";
    const GENERATOR_GE: &str =
        "070279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    /// Expected Scala `ContextExtension` order for the first-redemption set `{0,1,2,3,4,5,6,8}`,
    /// confirmed on-chain. Kept as a hardcoded regression guard for `scala_context_extension_order`.
    const SCALA_EXT_ORDER_NO_R7: &[&str] = &["0", "5", "1", "6", "2", "3", "8", "4"];

    /// Build a minimal, ergo-lib-parseable unsigned redemption tx whose reserve-input extension
    /// keys are inserted in the given order (with valid, type-correct constant values).
    fn tx_json_with_ext_keys(keys_in_order: &[&str]) -> Value {
        let mut ext: Map<String, Value> = Map::new();
        for k in keys_in_order {
            let v = match *k {
                "0" => serialize_ergo_byte(0),
                "1" => GENERATOR_GE.to_string(),
                "2" => serialize_coll_bytes(&[0u8; 65]),
                "3" => serialize_ergo_long(400_000_000),
                "4" => serialize_ergo_long(1_783_612_740_170),
                "5" => serialize_coll_bytes(&[]),
                "6" => serialize_coll_bytes(&[0u8; 65]),
                "7" => serialize_coll_bytes(&[0u8; 32]),
                "8" => serialize_coll_bytes(&[]),
                _ => serialize_coll_bytes(&[]),
            };
            ext.insert((*k).to_string(), json!(v));
        }
        json!({
            "inputs": [
                { "boxId": "01".repeat(32), "extension": Value::Object(ext) }
            ],
            "dataInputs": [],
            "outputs": [
                {
                    "value": 1_000_000,
                    "ergoTree": P2PK_TREE,
                    "creationHeight": 1,
                    "assets": [],
                    "additionalRegisters": {}
                }
            ]
        })
    }

    fn ext_json_keys(tx: &Value) -> Vec<String> {
        tx["inputs"][0]["extension"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn reorder_produces_scala_order_for_first_redemption_set() {
        let mut tx = tx_json_with_ext_keys(&["6", "1", "3", "4", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(ext_json_keys(&tx), SCALA_EXT_ORDER_NO_R7);
    }

    #[test]
    fn reorder_is_idempotent_when_already_in_scala_order() {
        let mut tx = tx_json_with_ext_keys(SCALA_EXT_ORDER_NO_R7);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(ext_json_keys(&tx), SCALA_EXT_ORDER_NO_R7);
    }

    #[test]
    fn reorder_produces_scala_order_for_subsequent_redemption_set() {
        // Subsequent redemptions add #7 (reserveLookupProof): the full set {0..8} orders as below.
        let mut tx = tx_json_with_ext_keys(&["6", "1", "3", "4", "7", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(
            ext_json_keys(&tx),
            &["0", "5", "1", "6", "2", "7", "3", "8", "4"]
        );
    }

    #[test]
    fn reorder_orders_arbitrary_key_set_in_scala_order() {
        // Full {0..9} set: keys are ordered by Scala HashTrieMap iteration, not by insertion.
        let mut tx = tx_json_with_ext_keys(&["9", "6", "1", "7", "3", "4", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        assert_eq!(
            ext_json_keys(&tx),
            &["0", "5", "1", "6", "9", "2", "7", "3", "8", "4"]
        );
    }

    #[test]
    fn parsed_unsigned_tx_extension_iterates_in_scala_order() {
        let mut tx = tx_json_with_ext_keys(&["6", "1", "3", "4", "8", "2", "0", "5"]);
        reorder_reserve_extension_scala(&mut tx);
        let unsigned: UnsignedTransaction = serde_json::from_value(tx).unwrap();
        let order: Vec<u8> = unsigned.inputs.as_slice()[0]
            .extension
            .values
            .keys()
            .copied()
            .collect();
        let expected: Vec<u8> = SCALA_EXT_ORDER_NO_R7
            .iter()
            .map(|s| s.parse::<u8>().unwrap())
            .collect();
        assert_eq!(order, expected);
    }

    #[test]
    fn scala_context_extension_order_matches_known_sets() {
        assert_eq!(
            scala_context_extension_order(&[0, 1, 2, 3, 4, 5, 6, 8]),
            vec![0, 5, 1, 6, 2, 3, 8, 4]
        );
        assert_eq!(
            scala_context_extension_order(&[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            vec![0, 5, 1, 6, 2, 7, 3, 8, 4]
        );
        // Order is independent of the input slice order.
        assert_eq!(
            scala_context_extension_order(&[8, 6, 5, 4, 3, 2, 1, 0]),
            vec![0, 5, 1, 6, 2, 3, 8, 4]
        );
    }

    #[test]
    fn vlq_encode_matches_ergo_encoding() {
        assert_eq!(vlq_encode(0), vec![0x00]);
        assert_eq!(vlq_encode(127), vec![0x7f]);
        assert_eq!(vlq_encode(128), vec![0x80, 0x01]);
        assert_eq!(vlq_encode(200), vec![0xc8, 0x01]);
        assert_eq!(vlq_encode(16384), vec![0x80, 0x80, 0x01]);
    }

    #[test]
    fn serialize_ergo_byte_encodes_type_and_value() {
        assert_eq!(serialize_ergo_byte(0), "0200");
        assert_eq!(serialize_ergo_byte(255), "02ff");
    }

    #[test]
    fn serialize_ergo_long_encodes_zigzag_vlq() {
        assert_eq!(serialize_ergo_long(0), "0500");
        // -1 zigzags to 1.
        assert_eq!(serialize_ergo_long(-1), "0501");
        // 1 zigzags to 2.
        assert_eq!(serialize_ergo_long(1), "0502");
    }

    #[test]
    fn serialize_coll_bytes_uses_vlq_length() {
        // Empty: type 0x0e + length 0.
        assert_eq!(serialize_coll_bytes(&[]), "0e00");
        // 65 bytes (< 128): single-byte length 0x41.
        let sig = serialize_coll_bytes(&[0u8; 65]);
        assert!(sig.starts_with("0e41"));
        assert_eq!(sig.len(), 2 + 2 + 65 * 2);
        // 200 bytes (>= 128): length is VLQ 200 -> [0xc8, 0x01].
        let long = serialize_coll_bytes(&[0u8; 200]);
        assert!(long.starts_with("0ec801"));
        assert_eq!(long.len(), 2 + 4 + 200 * 2);
    }

    #[test]
    fn parse_broadcast_tx_id_handles_node_shapes() {
        assert_eq!(parse_broadcast_tx_id("\"abc123\""), "abc123");
        assert_eq!(parse_broadcast_tx_id("{\"id\":\"def456\"}"), "def456");
        assert_eq!(parse_broadcast_tx_id("plainid"), "plainid");
    }

    #[test]
    fn address_round_trip_yields_p2pk_ergo_tree() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let address = pubkey_to_address(pubkey).unwrap();
        let tree = address_to_ergo_tree(&address).unwrap();
        assert_eq!(tree, format!("0008cd{}", pubkey));
    }
}
