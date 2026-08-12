use crate::api::{CompleteRedemptionRequest, TrackerClient};
use crate::output::progress;
use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;
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

/// Result of a redemption that was signed locally and broadcast to the network.
#[derive(Debug, Serialize)]
pub struct RedemptionBroadcastResult {
    pub tx_id: String,
}

/// Result of generating an unsigned redemption transaction for the node wallet.
#[derive(Debug, Serialize)]
pub struct UnsignedRedemptionResult {
    /// The unsigned transaction payload (node wallet format).
    pub transaction: serde_json::Value,
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub recipient_output_value: u64,
    pub reserve_output_value: u64,
    pub total_debt: u64,
    pub already_redeemed: u64,
    pub reserve_box_id: String,
    pub tracker_box_id: String,
    pub fee: u64,
    pub fee_input_count: usize,
    pub fee_input_total: u64,
    pub change_amount: u64,
    pub change_address: String,
    pub emergency: bool,
    pub first_redemption: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
}

/// Typed result of `transaction generate-redemption`: either a broadcast tx id
/// (`--local-sign`) or an unsigned transaction payload for the node wallet.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum GenerateRedemptionResult {
    Broadcast(RedemptionBroadcastResult),
    Unsigned(Box<UnsignedRedemptionResult>),
}

pub async fn handle_transaction_command(
    cmd: TransactionCommands,
    client: &TrackerClient,
    account_manager: &crate::account::AccountManager,
    json: bool,
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
            let result = generate_redemption_transaction(
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
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            Ok(())
        }
        TransactionCommands::RedeemAssisted {
            issuer_pubkey,
            recipient_pubkey,
            amount,
            recipient_secret,
        } => {
            let result = redeem_tracker_assisted(
                client,
                account_manager,
                &issuer_pubkey,
                &recipient_pubkey,
                amount,
                recipient_secret,
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            Ok(())
        }
    }
}

/// Intermediate results from building an unsigned redemption transaction.
struct RedemptionBuildResult {
    transaction_json: serde_json::Value,
    reserve_box_binary: String,
    fee_input_binaries: Vec<String>,
    tracker_box_binary: String,
    reserve_box_id: String,
    tracker_box_id: String,
    reserve_output_value: u64,
    recipient_output_value: u64,
    change_amount: u64,
    total_debt: u64,
    note_timestamp: u64,
    is_first_redemption: bool,
    fee_input_count: usize,
    fee_input_total: u64,
    change_address: String,
    recipient_address: String,
    issuer_signature_len: usize,
    tracker_signature_len: usize,
    insert_proof_len: usize,
    reserve_lookup_proof_len: Option<usize>,
    tracker_lookup_proof_len: usize,
    already_redeemed: u64,
}

/// Build the unsigned redemption transaction JSON and collect all metadata needed to either
/// sign+broadcast locally or emit JSON for the node wallet.
#[allow(clippy::too_many_arguments)]
async fn build_redemption_tx(
    client: &TrackerClient,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    emergency: bool,
    tracker_box_id: Option<String>,
    change_address: Option<String>,
    issuer_signature_hex: &str,
    local_sign: bool,
) -> Result<RedemptionBuildResult> {
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

    progress!("🔍 Retrieving note information...");
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

    // Resolve the current tracker box early so we can match reserves by tracker NFT ID.
    let tracker_box_id = if let Some(id) = tracker_box_id {
        progress!("✅ Using provided tracker box: {}", &id[..16]);
        id
    } else {
        progress!("🔍 Retrieving latest tracker box...");
        let tracker_box_response = client.get_latest_tracker_box_id().await;
        match tracker_box_response {
            Ok(response) => {
                progress!("✅ Found tracker box: {}", &response.tracker_box_id[..16]);
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

    // The tracker NFT ID is the first asset in the tracker box. Reserves whose R6
    // tracker NFT ID does not match this cannot be redeemed against the current
    // tracker box (the contract checks tracker.tokens(0)._1 == SELF.R6).
    progress!("🔍 Retrieving tracker NFT ID from tracker box...");
    let tracker_box_details = client
        .get_box_from_node(&tracker_box_id, NODE_URL, Some(API_KEY))
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to retrieve tracker box {} from Ergo node: {}",
                tracker_box_id,
                e
            )
        })?;
    let expected_tracker_nft_id = tracker_box_details
        .assets
        .first()
        .map(|a| a.token_id.clone())
        .ok_or_else(|| anyhow::anyhow!("Tracker box {} contains no assets", tracker_box_id))?;
    progress!("✅ Tracker NFT ID: {}", &expected_tracker_nft_id);

    progress!("🔍 Retrieving issuer's reserve box...");
    let reserves_response = client.get_reserves_by_issuer(issuer_pubkey).await?;
    const MIN_RESERVE_REMAINDER: u64 = 1_000_000; // 0.001 ERG min box value
    let required = amount.saturating_add(MIN_RESERVE_REMAINDER);
    let reserve_box = reserves_response
        .iter()
        .filter(|r| r.base_info.collateral_amount >= required)
        .filter(|r| r.base_info.tracker_nft_id == expected_tracker_nft_id)
        .min_by(|a, b| {
            a.base_info
                .collateral_amount
                .cmp(&b.base_info.collateral_amount)
                .then_with(|| b.base_info.last_updated_height.cmp(&a.base_info.last_updated_height))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No reserve box with sufficient collateral and matching tracker NFT {} found for issuer {} (need >= {} nanoERG)",
                expected_tracker_nft_id,
                issuer_pubkey,
                required
            )
        })?;

    progress!(
        "✅ Selected reserve box: {} (collateral: {} nanoERG, height: {}, tracker NFT: {})",
        decode_box_id(&reserve_box.box_id),
        reserve_box.base_info.collateral_amount,
        reserve_box.base_info.last_updated_height,
        &reserve_box.base_info.tracker_nft_id
    );

    let reserve_box_id = decode_box_id(&reserve_box.box_id);
    let tracker_nft_id = &reserve_box.base_info.tracker_nft_id;

    progress!("🔗 Converting public keys to addresses...");
    let recipient_address = pubkey_to_address(recipient_pubkey)?;

    // Fetch the recipient's private key for the node-wallet JSON path.
    progress!("🔍 Resolving recipient private key...");
    let recipient_private_key = client
        .get_private_key(NODE_URL, Some(API_KEY), &recipient_address)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch recipient private key from node wallet. Ensure the recipient address {} is in the wallet: {}", recipient_address, e))?;
    progress!("✅ Fetched recipient private key");

    // Get tracker lookup proof for context var #8 from server
    progress!("🔍 Retrieving tracker lookup proof from server...");
    let tracker_proof = client
        .get_tracker_proof(issuer_pubkey, recipient_pubkey)
        .await?;
    let total_debt = tracker_proof.total_debt;
    let tracker_lookup_proof = hex::decode(&tracker_proof.proof)
        .map_err(|e| anyhow::anyhow!("Invalid tracker proof hex: {}", e))?;
    let note_timestamp = note.timestamp;

    // Get the reserve proof from the server.
    progress!("🔍 Retrieving reserve insert proof from server...");
    let reserve_proof = client
        .get_reserve_proof(issuer_pubkey, recipient_pubkey, amount, note_timestamp)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get reserve proof: {}", e))?;

    // Build serialized SAvlTree for R5 register.
    let r5_bytes = build_savl_tree_from_digest(&reserve_proof.new_reserve_state_digest);
    let r5_hex = hex::encode(&r5_bytes);

    // Get the reserve contract P2S address from the server configuration
    progress!("🔍 Retrieving reserve contract P2S address from server configuration...");
    let reserve_contract_p2s = client.get_basis_reserve_contract_p2s().await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to retrieve reserve contract P2S address from server: {}",
            e
        )
    })?;

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

    let is_first_redemption = note.amount_redeemed == 0;
    let (reserve_lookup_proof, reserve_insert_proof) = if is_first_redemption {
        progress!("🔍 First redemption - using reserve insert proof...");
        let insert_proof = hex::decode(&reserve_proof.insert_proof)
            .map_err(|e| anyhow::anyhow!("Invalid reserve insert proof hex: {}", e))?;
        (None, insert_proof)
    } else {
        progress!("🔍 Using reserve lookup and insert proofs...");
        progress!(
            "✅ Got reserve proof: already_redeemed={} nanoERG, is_first={}",
            reserve_proof.already_redeemed,
            reserve_proof.is_first_redemption
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
    progress!("🔍 Verifying tracker box on Ergo node...");
    client.get_box_from_node(&tracker_box_id, NODE_URL, Some(API_KEY)).await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve tracker box {} from Ergo node: {}. Cannot generate redemption transaction.", tracker_box_id, e))?;

    // Retrieve the actual reserve box from the Ergo node
    progress!("🔍 Retrieving reserve box from Ergo node...");
    let reserve_box_details = client
        .get_box_from_node(&reserve_box_id, NODE_URL, Some(API_KEY))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve reserve box from Ergo node: {}", e))?;

    let reserve_nft_id = reserve_box_details
        .assets
        .first()
        .map(|asset| asset.token_id.clone())
        .unwrap_or_else(|| tracker_nft_id.clone());

    let refund_initiation_height = basis_store::ergo_scanner::decode_ergo_long_register(
        reserve_box_details.additional_registers.get("R7"),
    );

    // Fetch wallet-owned fee input boxes from the node.
    progress!("🔍 Retrieving wallet fee inputs from Ergo node...");
    let wallet_boxes = client
        .get_wallet_boxes(NODE_URL, Some(API_KEY))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve wallet boxes from Ergo node: {}", e))?;

    // For local signing every fee input must be a plain P2PK box and all fee inputs must share
    // the same ergoTree.
    let target_tree = if local_sign {
        if let Some(ref addr) = change_address {
            Some(address_to_ergo_tree(addr)?)
        } else {
            wallet_boxes
                .iter()
                .find(|b| b.assets.is_empty() && ergo_tree_to_p2pk_address(&b.ergo_tree).is_ok())
                .map(|b| b.ergo_tree.clone())
        }
    } else {
        None
    };

    let (fee_inputs, fee_input_total) = select_fee_inputs(
        &wallet_boxes,
        TRANSACTION_FEE,
        &reserve_box_id,
        &tracker_box_id,
        target_tree.as_deref(),
    )
    .ok_or_else(|| anyhow::anyhow!(
        "No wallet boxes covering {} nanoERG fee found. Ensure the wallet is synced and has at least {} nanoERG available.",
        TRANSACTION_FEE, TRANSACTION_FEE
    ))?;

    progress!(
        "✅ Selected {} fee input box(es) totaling {} nanoERG",
        fee_inputs.len(),
        fee_input_total
    );

    let change_address = if let Some(addr) = change_address {
        addr
    } else if let Ok(addr) = ergo_tree_to_p2pk_address(&fee_inputs[0].ergo_tree) {
        addr
    } else {
        recipient_address.clone()
    };

    progress!("📦 Preparing box IDs for transaction...");

    let issuer_signature = hex::decode(issuer_signature_hex)
        .map_err(|e| anyhow::anyhow!("Invalid issuer signature hex: {}", e))?;

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

    let insert_proof = reserve_insert_proof.clone();

    // Build context extension map
    let mut context_extension: HashMap<String, String> = HashMap::new();
    context_extension.insert("0".to_string(), serialize_ergo_byte(0));
    context_extension.insert("1".to_string(), format!("07{}", recipient_pubkey));
    context_extension.insert("2".to_string(), serialize_coll_bytes(&issuer_signature));
    context_extension.insert("3".to_string(), serialize_ergo_long(total_debt as i64));
    context_extension.insert("4".to_string(), serialize_ergo_long(note_timestamp as i64));
    context_extension.insert("5".to_string(), serialize_coll_bytes(&insert_proof));
    context_extension.insert("6".to_string(), serialize_coll_bytes(&tracker_signature));
    if let Some(ref proof) = reserve_lookup_proof {
        context_extension.insert("7".to_string(), serialize_coll_bytes(proof));
    }
    context_extension.insert("8".to_string(), serialize_coll_bytes(&tracker_lookup_proof));

    let reserve_ergo_tree = address_to_ergo_tree(&reserve_contract_p2s)?;
    let recipient_ergo_tree = address_to_ergo_tree(&recipient_address)?;
    let change_ergo_tree = address_to_ergo_tree(&change_address)?;

    let current_height = client.get_node_height(NODE_URL, Some(API_KEY)).await?;

    let reserve_box_binary = client
        .get_box_binary(&reserve_box_id, NODE_URL, Some(API_KEY))
        .await?;
    let tracker_box_binary = client
        .get_box_binary(&tracker_box_id, NODE_URL, Some(API_KEY))
        .await?;

    let mut fee_input_json = Vec::new();
    let mut fee_input_binaries = Vec::new();
    for fee_box in &fee_inputs {
        fee_input_json.push(json!({
            "boxId": fee_box.box_id,
            "extension": serde_json::json!({})
        }));
        let binary = client
            .get_box_binary(&fee_box.box_id, NODE_URL, Some(API_KEY))
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to get binary for fee input {}: {}",
                    fee_box.box_id,
                    e
                )
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
            "ergoTree": "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304",
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {}
        }),
    ];

    if change_amount > 0 {
        let change_assets: Vec<serde_json::Value> = fee_inputs
            .iter()
            .flat_map(|b| {
                b.assets.iter().map(|a| {
                    json!({
                        "tokenId": a.token_id,
                        "amount": a.amount
                    })
                })
            })
            .collect();
        outputs.push(json!({
            "value": change_amount,
            "ergoTree": change_ergo_tree,
            "creationHeight": current_height,
            "assets": change_assets,
            "additionalRegisters": {}
        }));
    }

    let mut inputs_raw = vec![reserve_box_binary.clone()];
    inputs_raw.extend(fee_input_binaries.clone());

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

    Ok(RedemptionBuildResult {
        transaction_json,
        reserve_box_binary,
        fee_input_binaries,
        tracker_box_binary,
        reserve_box_id,
        tracker_box_id,
        reserve_output_value,
        recipient_output_value,
        change_amount,
        total_debt,
        note_timestamp,
        is_first_redemption,
        fee_input_count: fee_inputs.len(),
        fee_input_total,
        change_address,
        recipient_address,
        issuer_signature_len: issuer_signature.len(),
        tracker_signature_len: tracker_signature.len(),
        insert_proof_len: insert_proof.len(),
        reserve_lookup_proof_len: reserve_lookup_proof.as_ref().map(|p| p.len()),
        tracker_lookup_proof_len: tracker_lookup_proof.len(),
        already_redeemed: reserve_proof.already_redeemed,
    })
}

/// Execute a full local-sign redemption: fetch the note and on-chain data, build the unsigned
/// transaction, request the tracker signature, sign the reserve and fee inputs locally, and
/// broadcast the transaction to the local Ergo node.
#[allow(clippy::too_many_arguments)]
pub async fn execute_local_redemption(
    client: &TrackerClient,
    account_manager: &crate::account::AccountManager,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    emergency: bool,
    tracker_box_id: Option<String>,
    change_address: Option<String>,
    recipient_secret: Option<String>,
    fee_secret: Option<String>,
) -> Result<String> {
    progress!("🔍 Retrieving note information...");
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

    // The reserve signature must be freshly generated for the current redemption because the
    // contract requires `timestamp > storedTimestamp`. After a prior redemption the note's
    // timestamp is refreshed, so the signature stored on the note is stale.
    progress!("🔍 Fetching tracker proof for total debt...");
    let tracker_proof = client
        .get_tracker_proof(issuer_pubkey, recipient_pubkey)
        .await?;
    let total_debt = tracker_proof.total_debt;

    progress!("🔑 Signing redemption message with the current issuer account...");
    let current = account_manager
        .get_current()
        .ok_or_else(|| anyhow::anyhow!("No current account selected; issuer account is required to sign the redemption message"))?;
    let issuer_pk: [u8; 33] = hex::decode(issuer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid issuer public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Issuer public key must be 33 bytes"))?;
    let recipient_pk: [u8; 33] = hex::decode(recipient_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid recipient public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Recipient public key must be 33 bytes"))?;
    if current.get_pubkey_hex() != issuer_pubkey {
        return Err(anyhow::anyhow!(
            "Current account {} is not the issuer {}; only the issuer can sign the redemption message",
            current.get_pubkey_hex(),
            issuer_pubkey
        ));
    }
    let message = redemption_signing_message(&issuer_pk, &recipient_pk, total_debt, note.timestamp);
    let issuer_signature = current.sign_message(&message)?;
    let issuer_signature_hex = hex::encode(issuer_signature);

    let build = build_redemption_tx(
        client,
        issuer_pubkey,
        recipient_pubkey,
        amount,
        emergency,
        tracker_box_id,
        change_address,
        &issuer_signature_hex,
        true,
    )
    .await?;

    let tx_id = sign_and_broadcast_local(SignLocalParams {
        client,
        issuer_pubkey,
        recipient_pubkey,
        amount,
        total_debt: build.total_debt,
        note_timestamp: build.note_timestamp,
        is_first_redemption: build.is_first_redemption,
        emergency,
        reserve_box_id: &build.reserve_box_id,
        tracker_box_id: &build.tracker_box_id,
        reserve_output_value: build.reserve_output_value,
        recipient_output_value: build.recipient_output_value,
        change_amount: build.change_amount,
        unsigned_tx: build.transaction_json["tx"].clone(),
        reserve_box_binary: &build.reserve_box_binary,
        fee_input_binaries: &build.fee_input_binaries,
        tracker_box_binary: &build.tracker_box_binary,
        issuer_signature_len: build.issuer_signature_len,
        tracker_signature_len: build.tracker_signature_len,
        insert_proof_len: build.insert_proof_len,
        reserve_lookup_proof_len: build.reserve_lookup_proof_len,
        tracker_lookup_proof_len: build.tracker_lookup_proof_len,
        fee_input_count: build.fee_input_count,
        fee_input_total: build.fee_input_total,
        change_address: &build.change_address,
        recipient_address: &build.recipient_address,
        recipient_secret,
        fee_secret,
    })
    .await?;

    // Sync the tracker's local state so subsequent redemptions can generate a reserve lookup
    // proof against the updated reserve tree.
    let new_already_redeemed = build.already_redeemed.saturating_add(amount);
    if let Err(e) = client
        .complete_redemption(CompleteRedemptionRequest {
            redemption_id: tx_id.clone(),
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            redeemed_amount: amount,
            new_already_redeemed: Some(new_already_redeemed),
        })
        .await
    {
        eprintln!(
            "⚠️ Redemption broadcast succeeded, but tracker state sync failed: {}. Subsequent redemptions may fail until the tracker state is repaired.",
            e
        );
    } else {
        progress!("✅ Tracker state synced for next redemption.");
    }

    progress!("✅ Redemption broadcast with LOCAL proveDlog signatures.");
    progress!("📋 Transaction ID: {}", tx_id);

    Ok(tx_id)
}

#[allow(clippy::too_many_arguments)]
pub async fn generate_redemption_transaction(
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
) -> Result<GenerateRedemptionResult> {
    if local_sign {
        let tx_id = execute_local_redemption(
            client,
            account_manager,
            issuer_pubkey,
            recipient_pubkey,
            amount,
            emergency,
            tracker_box_id,
            change_address,
            recipient_secret,
            fee_secret,
        )
        .await?;
        progress!("✅ Redemption broadcast. Transaction ID: {}", tx_id);
        return Ok(GenerateRedemptionResult::Broadcast(
            RedemptionBroadcastResult { tx_id },
        ));
    }

    // Fetch the note so we can compute the issuer signature with the current account.
    progress!("🔍 Retrieving note information...");
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

    // Get issuer signature from CLI wallet
    progress!("🔑 Signing redemption with issuer key...");
    let current_account = account_manager
        .get_current()
        .ok_or_else(|| anyhow::anyhow!("No current account selected"))?;

    let issuer_pk: [u8; 33] = hex::decode(issuer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid issuer public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Issuer public key must be 33 bytes"))?;
    let recipient_pk: [u8; 33] = hex::decode(recipient_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid recipient public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Recipient public key must be 33 bytes"))?;

    let message = redemption_signing_message(
        &issuer_pk,
        &recipient_pk,
        note.amount_collected,
        note.timestamp,
    );
    let issuer_signature = current_account.sign_message(&message)?;
    let issuer_signature_hex = hex::encode(issuer_signature);

    let build = build_redemption_tx(
        client,
        issuer_pubkey,
        recipient_pubkey,
        amount,
        emergency,
        tracker_box_id,
        change_address,
        &issuer_signature_hex,
        false,
    )
    .await?;

    let transaction_json = build.transaction_json.clone();

    let json_string = serde_json::to_string_pretty(&transaction_json)?;

    match &output_file {
        Some(file_path) => {
            fs::write(file_path, &json_string)?;
            progress!("✅ Transaction JSON written to: {}", file_path);
        }
        None => {
            progress!("{}", json_string);
        }
    }

    progress!("✅ Redemption transaction generated successfully!");
    progress!("📋 Transaction details:");
    progress!("   Issuer: {}", issuer_pubkey);
    progress!("   Recipient: {}", recipient_pubkey);
    progress!("   Redemption amount: {} nanoERG", amount);
    progress!(
        "   Recipient receives: {} nanoERG",
        build.recipient_output_value
    );
    progress!(
        "   Reserve output value: {} nanoERG",
        build.reserve_output_value
    );
    progress!("   Total debt: {} nanoERG", build.total_debt);
    progress!("   Already redeemed: {} nanoERG", note.amount_redeemed);
    progress!("   Reserve box ID: {}", build.reserve_box_id);
    progress!("   Tracker box ID: {}", build.tracker_box_id);
    progress!("   Transaction fee: {} nanoERG", TRANSACTION_FEE);
    progress!(
        "   Fee inputs: {} box(es), {} nanoERG total",
        build.fee_input_count,
        build.fee_input_total
    );
    if build.change_amount > 0 {
        progress!(
            "   Change output: {} nanoERG to {}",
            build.change_amount,
            build.change_address
        );
    }
    progress!("   Emergency redemption: {}", emergency);
    progress!("   First redemption: {}", build.is_first_redemption);
    progress!("📝 Context Extension Variables:");
    progress!("   #0 (action): 0x00 (redemption, reserve output index 0)");
    progress!("   #1 (receiver): {}", recipient_pubkey);
    progress!("   #2 (reserveSig): {} bytes", build.issuer_signature_len);
    progress!("   #3 (totalDebt): {}", build.total_debt);
    progress!("   #5 (insertProof): {} bytes", build.insert_proof_len);
    progress!("   #6 (trackerSig): {} bytes", build.tracker_signature_len);
    if let Some(len) = build.reserve_lookup_proof_len {
        progress!("   #7 (reserveLookupProof): {} bytes", len);
    } else {
        progress!("   #7 (reserveLookupProof): omitted (first redemption)");
    }
    progress!(
        "   #8 (trackerLookupProof): {} bytes",
        build.tracker_lookup_proof_len
    );

    Ok(GenerateRedemptionResult::Unsigned(Box::new(
        UnsignedRedemptionResult {
            transaction: transaction_json,
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            amount,
            recipient_output_value: build.recipient_output_value,
            reserve_output_value: build.reserve_output_value,
            total_debt: build.total_debt,
            already_redeemed: note.amount_redeemed,
            reserve_box_id: build.reserve_box_id,
            tracker_box_id: build.tracker_box_id,
            fee: TRANSACTION_FEE,
            fee_input_count: build.fee_input_count,
            fee_input_total: build.fee_input_total,
            change_amount: build.change_amount,
            change_address: build.change_address,
            emergency,
            first_redemption: build.is_first_redemption,
            output_file,
        },
    )))
}

/// Tracker-assisted redemption driver (mirrors the TUI flow). The tracker builds the unsigned
/// transaction and signs the fee input(s); the CLI signs the issuer message with the current
/// account and adds the reserve input's proveDlog(recipient) proof, then submits via the tracker.
pub async fn redeem_tracker_assisted(
    client: &TrackerClient,
    account_manager: &crate::account::AccountManager,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    recipient_secret: Option<String>,
) -> Result<RedemptionBroadcastResult> {
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

    progress!("🔍 Fetching note and tracker proof...");
    let note = client
        .get_note(issuer_pubkey, recipient_pubkey)
        .await?
        .ok_or_else(|| anyhow::anyhow!("note not found for issuer/recipient"))?;
    let total_debt = client
        .get_tracker_proof(issuer_pubkey, recipient_pubkey)
        .await?
        .total_debt;
    let timestamp = note.timestamp;
    progress!("✅ total_debt={} timestamp={}", total_debt, timestamp);

    // Issuer signs the redemption message with the current account.
    let current = account_manager
        .get_current()
        .ok_or_else(|| anyhow::anyhow!("no current account (should be the issuer)"))?;
    if current.get_pubkey_hex() != issuer_pubkey {
        progress!(
            "⚠️  current account {}... != issuer {}...; signature may be rejected",
            &current.get_pubkey_hex()[..16],
            &issuer_pubkey[..16]
        );
    }
    let message = redemption_signing_message(&issuer_pk, &recipient_pk, total_debt, timestamp);
    let issuer_sig = current.sign_message(&message)?;
    progress!("✅ issuer signature ({} bytes)", issuer_sig.len());

    // Tracker builds the unsigned tx and signs the fee input(s).
    progress!("🔍 Requesting tracker build (POST /redemption/build)...");
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
    progress!(
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
    progress!("🖊️  Adding reserve proveDlog(recipient) proof...");
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
    progress!("✅ signed tx id: {}", local_id);

    progress!("📡 Submitting via tracker (POST /redemption/submit)...");
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
    progress!("✅ Redemption broadcast. Transaction ID: {}", tx_id);
    Ok(RedemptionBroadcastResult { tx_id })
}

/// Select wallet boxes covering the required fee amount.
///
/// Prefer boxes without tokens and exclude the reserve box being spent and the
/// tracker box used as a data input (the wallet may report both if they have
/// been added to scans). If `target_tree` is provided, only boxes whose
/// `ergoTree` exactly matches it are considered; this is required for local
/// signing where every fee input must be provable with the same dlog secret.
/// If the wallet only has token-bearing boxes, fall back to them and preserve
/// their tokens in the change output.
fn select_fee_inputs(
    wallet_boxes: &[crate::api::ErgoBoxDetails],
    required: u64,
    reserve_box_id: &str,
    tracker_box_id: &str,
    target_tree: Option<&str>,
) -> Option<(Vec<crate::api::ErgoBoxDetails>, u64)> {
    // Work with owned clones so we can return them without borrow issues.
    let candidates: Vec<crate::api::ErgoBoxDetails> = wallet_boxes
        .iter()
        .filter(|b| b.box_id != reserve_box_id && b.box_id != tracker_box_id)
        .filter(|b| target_tree.is_none_or(|t| b.ergo_tree == t))
        .cloned()
        .collect();

    // First try to use only boxes without tokens.
    let mut token_free: Vec<_> = candidates
        .iter()
        .filter(|b| b.assets.is_empty())
        .cloned()
        .collect();
    token_free.sort_by_key(|b| b.value);

    // Try a single box.
    if let Some(box_) = token_free.iter().find(|b| b.value >= required) {
        return Some((vec![box_.clone()], box_.value));
    }

    // Try accumulating token-free boxes.
    let mut selected = Vec::new();
    let mut total = 0u64;
    for box_ in token_free {
        total += box_.value;
        selected.push(box_);
        if total >= required {
            return Some((selected, total));
        }
    }

    // Fall back to token-bearing boxes if no token-free combination works.
    let mut token_boxes: Vec<_> = candidates
        .iter()
        .filter(|b| !b.assets.is_empty())
        .cloned()
        .collect();
    token_boxes.sort_by_key(|b| b.value);

    // Try a single token-bearing box.
    if let Some(box_) = token_boxes.iter().find(|b| b.value >= required) {
        return Some((vec![box_.clone()], box_.value));
    }

    // Try accumulating token-bearing boxes.
    let mut selected = Vec::new();
    let mut total = 0u64;
    for box_ in token_boxes {
        total += box_.value;
        selected.push(box_);
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
            progress!(
                "🔑 Fetching {} private key from node wallet ({})...",
                label,
                address
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
    let parameters = Parameters::default();

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
async fn sign_and_broadcast_local(p: SignLocalParams<'_>) -> Result<String> {
    progress!("🖊️  Signing redemption locally (client-side proveDlog for both inputs)...");

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
    progress!("✅ Signed locally. tx id: {}", local_id);
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

    progress!("✅ Redemption broadcast with LOCAL proveDlog signatures.");
    progress!("📋 Transaction details:");
    progress!("   Transaction ID: {}", tx_id);
    progress!("   Issuer: {}", p.issuer_pubkey);
    progress!("   Recipient: {}", p.recipient_pubkey);
    progress!("   Redemption amount: {} nanoERG", p.amount);
    progress!(
        "   Recipient receives: {} nanoERG",
        p.recipient_output_value
    );
    progress!(
        "   Reserve output value: {} nanoERG",
        p.reserve_output_value
    );
    progress!("   Total debt: {} nanoERG", p.total_debt);
    progress!("   First redemption: {}", p.is_first_redemption);
    progress!("   Emergency redemption: {}", p.emergency);
    progress!("   Reserve box ID: {}", p.reserve_box_id);
    progress!("   Tracker box ID: {}", p.tracker_box_id);
    progress!("   Transaction fee: {} nanoERG", TRANSACTION_FEE);
    progress!(
        "   Fee inputs: {} box(es), {} nanoERG total",
        p.fee_input_count,
        p.fee_input_total
    );
    if p.change_amount > 0 {
        progress!(
            "   Change output: {} nanoERG to {}",
            p.change_amount,
            p.change_address
        );
    }
    progress!("📝 Context Extension Variables:");
    progress!("   #0 (action): 0x00 (redemption, reserve output index 0)");
    progress!("   #1 (receiver): {}", p.recipient_pubkey);
    progress!("   #2 (reserveSig): {} bytes", p.issuer_signature_len);
    progress!("   #3 (totalDebt): {}", p.total_debt);
    progress!("   #4 (timestamp): {}", p.note_timestamp);
    progress!("   #5 (insertProof): {} bytes", p.insert_proof_len);
    progress!("   #6 (trackerSig): {} bytes", p.tracker_signature_len);
    match p.reserve_lookup_proof_len {
        Some(len) => progress!("   #7 (reserveLookupProof): {} bytes", len),
        None => progress!("   #7 (reserveLookupProof): omitted (first redemption)"),
    }
    progress!(
        "   #8 (trackerLookupProof): {} bytes",
        p.tracker_lookup_proof_len
    );

    Ok(tx_id)
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
