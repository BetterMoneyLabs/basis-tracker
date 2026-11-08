use crate::api::TrackerClient;
use crate::output::progress;
use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;

use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::wallet::secret_key::SecretKey;

use basis_offchain::signing::{add_input_proof, redemption_signing_message};

const NODE_URL: &str = "http://127.0.0.1:9053";
const API_KEY: &str = "hello";
const TRANSACTION_FEE: u64 = 1_000_000;
/// Minimum ERG value for the recipient output box when redeeming token-backed reserves.
const TOKEN_RECIPIENT_MIN_VALUE_NANOERG: u64 = 1_000_000;

/// Encode an unsigned integer using Ergo's VLQ (Variable-Length Quantity) encoding.
/// Bytes are emitted least-significant group first (little-endian VLQ).
/// Parse the denomination recorded on a stored note (defaults to ERG when absent).
pub(crate) fn note_denom(note: &crate::api::SerializableIouNote) -> basis_store::DenomKey {
    basis_store::DenomKey::parse(&note.denomination).unwrap_or(basis_store::DenomKey::Erg)
}

/// True when a reserve reports itself as belonging to `denomination`:
/// ERG reserves report an empty `reserve_token_id`; token reserves report the
/// collateral token's hex id (case-insensitive). Without this, a redemption
/// against an issuer holding both an ERG and a token reserve could pick the
/// wrong currency's reserve box and redeem token debt against ERG collateral.
fn denom_matches_reserve(reserve_token_id: &str, denom: &basis_store::DenomKey) -> bool {
    match denom {
        basis_store::DenomKey::Erg => reserve_token_id.is_empty(),
        basis_store::DenomKey::Token(id) => {
            !reserve_token_id.is_empty() && reserve_token_id.eq_ignore_ascii_case(&hex::encode(id))
        }
    }
}

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

/// Decode a server-returned box id that may be (double) hex-encoded.
///
/// The tracker's reserve DB sometimes stores the 64-char box id itself hex-encoded (a 128-char
/// string whose hex bytes are ASCII hex chars). This unwraps up to two levels of such encoding and
/// returns the canonical box id; a plain box id is returned unchanged.
fn decode_box_id(raw: &str) -> String {
    let mut cur = raw.to_string();
    for _ in 0..2 {
        if !cur.len().is_multiple_of(2) {
            break;
        }
        match hex::decode(&cur) {
            Ok(bytes) if bytes.iter().all(|b| b.is_ascii_hexdigit()) => {
                match String::from_utf8(bytes) {
                    Ok(s) if s.len() < cur.len() => {
                        cur = s;
                        continue;
                    }
                    _ => break,
                }
            }
            _ => break,
        }
    }
    cur
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
        /// Denomination of the note ("erg" or "token:<64-hex>"). Required to select the
        /// right reserve when the pair also holds notes in another denomination; omitted
        /// means ERG / legacy notes.
        #[arg(long)]
        denomination: Option<String>,
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
        /// Denomination of the note ("erg" or "token:<64-hex>"). Required to select the
        /// right reserve when the pair also holds notes in another denomination; omitted
        /// means ERG / legacy notes.
        #[arg(long)]
        denomination: Option<String>,
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
            denomination,
        } => {
            let result = generate_redemption_transaction(
                client,
                &issuer_pubkey,
                &recipient_pubkey,
                amount,
                output_file,
                emergency,
                tracker_box_id,
                change_address,
                local_sign,
                recipient_secret,
                denomination.as_deref(),
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
            denomination,
        } => {
            let result = redeem_tracker_assisted(
                client,
                &issuer_pubkey,
                &recipient_pubkey,
                amount,
                recipient_secret,
                denomination.as_deref(),
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
    reserve_box_id: String,
    tracker_box_id: String,
    reserve_output_value: u64,
    recipient_output_value: u64,
    change_amount: u64,
    total_debt: u64,
    is_first_redemption: bool,
    fee_input_count: usize,
    fee_input_total: u64,
    change_address: String,
    issuer_signature_len: usize,
    tracker_signature_len: usize,
    insert_proof_len: usize,
    reserve_lookup_proof_len: Option<usize>,
    tracker_lookup_proof_len: usize,
}

/// Build the unsigned redemption transaction JSON (node-wallet signing payload) and
/// collect the metadata needed to print the transaction summary.
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
    denomination: &basis_store::DenomKey,
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
        .get_note_denominated(
            issuer_pubkey,
            recipient_pubkey,
            Some(&denomination.denomination_id()),
        )
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
    const MIN_RESERVE_REMAINDER_NANOERG: u64 = 1_000_000; // 0.001 ERG min box value
    let reserve_box = reserves_response
        .iter()
        .filter(|r| {
            // Token-backed reserves report collateral in token units; ERG-backed
            // reserves report collateral in nanoERG.
            let is_token_reserve = !r.base_info.reserve_token_id.is_empty();
            if is_token_reserve {
                r.base_info.collateral_amount >= amount
            } else {
                r.base_info.collateral_amount >= amount.saturating_add(MIN_RESERVE_REMAINDER_NANOERG)
            }
        })
        .filter(|r| r.base_info.tracker_nft_id == expected_tracker_nft_id)
        .filter(|r| denom_matches_reserve(&r.base_info.reserve_token_id, denomination))
        .max_by(|a, b| {
            a.base_info
                .last_updated_height
                .cmp(&b.base_info.last_updated_height)
                .then_with(|| a.base_info.collateral_amount.cmp(&b.base_info.collateral_amount))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No reserve box with sufficient collateral and matching tracker NFT {} found for issuer {} (need >= {} units)",
                expected_tracker_nft_id,
                issuer_pubkey,
                amount
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
        .get_tracker_proof(
            issuer_pubkey,
            recipient_pubkey,
            Some(&denomination.denomination_id()),
        )
        .await?;
    let total_debt = tracker_proof.total_debt;
    let tracker_lookup_proof = hex::decode(&tracker_proof.proof)
        .map_err(|e| anyhow::anyhow!("Invalid tracker proof hex: {}", e))?;
    let note_timestamp = note.timestamp;

    // Get the reserve proof from the server.
    progress!("🔍 Retrieving reserve insert proof from server...");
    let reserve_proof = client
        .get_reserve_proof(
            issuer_pubkey,
            recipient_pubkey,
            amount,
            note_timestamp,
            Some(&denomination.denomination_id()),
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get reserve proof: {}", e))?;

    // Build serialized SAvlTree for R5 register.
    let r5_bytes = build_savl_tree_from_digest(&reserve_proof.new_reserve_state_digest);
    let r5_hex = hex::encode(&r5_bytes);

    let is_token_reserve = !reserve_box.base_info.reserve_token_id.is_empty();

    // Get the reserve contract P2S address from the server configuration.
    // Token-backed reserves must use the token reserve contract; ERG-backed
    // reserves use the standard ERG reserve contract.
    progress!("🔍 Retrieving reserve contract P2S address from server configuration...");
    let reserve_contract_p2s = if is_token_reserve {
        client
            .get_reserve_token_config()
            .await
            .map(|cfg| cfg.basis_token_reserve_contract_p2s)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to retrieve token reserve contract P2S address from server: {}",
                    e
                )
            })?
    } else {
        client.get_basis_reserve_contract_p2s().await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to retrieve reserve contract P2S address from server: {}",
                e
            )
        })?
    };

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

    // For token-backed reserves the collateral is held in the reserve token.
    // The reserve output keeps the original ERG value and reduces the token
    // amount; the recipient receives the redeemed tokens in a new box funded
    // by the fee inputs. ERG-backed reserves reduce the reserve's ERG value
    // and send it to the recipient as before.
    let (
        reserve_output_value,
        reserve_output_assets,
        recipient_output_value,
        recipient_output_assets,
        fee_input_required,
    ) = if is_token_reserve {
        let reserve_token_id = reserve_box.base_info.reserve_token_id.clone();
        let token_amount_in = reserve_box_details
            .assets
            .iter()
            .find(|a| a.token_id == *reserve_token_id)
            .map(|a| a.amount)
            .ok_or_else(|| anyhow::anyhow!("Reserve box missing configured reserve token"))?;
        let token_amount_out = token_amount_in.saturating_sub(amount);
        if token_amount_out == 0 {
            return Err(anyhow::anyhow!(
                "Reserve token amount would be zero after redemption"
            ));
        }
        let reserve_assets = json!([
            { "tokenId": reserve_nft_id, "amount": 1 },
            { "tokenId": reserve_token_id, "amount": token_amount_out }
        ]);
        let recipient_assets = json!([{ "tokenId": reserve_token_id, "amount": amount }]);
        (
            reserve_box_details.value,
            reserve_assets,
            TOKEN_RECIPIENT_MIN_VALUE_NANOERG,
            recipient_assets,
            TRANSACTION_FEE.saturating_add(TOKEN_RECIPIENT_MIN_VALUE_NANOERG),
        )
    } else {
        let reserve_output_value = reserve_box_details.value.saturating_sub(amount);
        if reserve_output_value == 0 {
            return Err(anyhow::anyhow!(
                "Reserve output value would be zero after redemption"
            ));
        }
        let reserve_assets = json!([{ "tokenId": reserve_nft_id, "amount": 1 }]);
        (
            reserve_output_value,
            reserve_assets,
            amount,
            json!([]),
            TRANSACTION_FEE,
        )
    };

    let refund_initiation_height = basis_store::ergo_scanner::decode_ergo_long_register(
        reserve_box_details.additional_registers.get("R7"),
    );

    // Fetch wallet-owned fee input boxes from the node.
    progress!("🔍 Retrieving wallet fee inputs from Ergo node...");
    let wallet_boxes = client
        .get_wallet_boxes(NODE_URL, Some(API_KEY))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve wallet boxes from Ergo node: {}", e))?;

    let (fee_inputs, fee_input_total) = select_fee_inputs(
        &wallet_boxes,
        fee_input_required,
        &reserve_box_id,
        &tracker_box_id,
        None,
    )
    .ok_or_else(|| anyhow::anyhow!(
        "No wallet boxes covering {} nanoERG fee found. Ensure the wallet is synced and has at least {} nanoERG available.",
        fee_input_required, fee_input_required
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
            Some(&denomination.denomination_id()),
        )
        .await?;
    let tracker_signature = hex::decode(&tracker_signature_response.tracker_signature)
        .map_err(|e| anyhow::anyhow!("Invalid tracker signature hex: {}", e))?;

    let insert_proof = reserve_insert_proof.clone();

    // Build context extension map
    // Debt and timestamp serialize as SIGNED Longs on-chain; reject values above
    // i64::MAX instead of wrapping negative (defense in depth — stored notes are
    // already capped at creation).
    let total_debt_i64 = i64::try_from(total_debt)
        .map_err(|_| anyhow::anyhow!("totalDebt exceeds the on-chain signed-Long range"))?;
    let note_timestamp_i64 = i64::try_from(note_timestamp)
        .map_err(|_| anyhow::anyhow!("note timestamp exceeds the on-chain signed-Long range"))?;
    let mut context_extension: HashMap<String, String> = HashMap::new();
    context_extension.insert("0".to_string(), serialize_ergo_byte(0));
    context_extension.insert("1".to_string(), format!("07{}", recipient_pubkey));
    context_extension.insert("2".to_string(), serialize_coll_bytes(&issuer_signature));
    context_extension.insert("3".to_string(), serialize_ergo_long(total_debt_i64));
    context_extension.insert("4".to_string(), serialize_ergo_long(note_timestamp_i64));
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
    // The reserve box id came from the tracker, and input 0 is signed with the recipient's key:
    // it must be the issuer's reserve under the compiled contract, not e.g. a recipient P2PK box
    // that the "reserve output" would then drain.
    let reserve_box = ErgoBox::sigma_parse_bytes(&hex::decode(&reserve_box_binary)?)
        .map_err(|e| anyhow::anyhow!("failed to parse reserve box {}: {:?}", reserve_box_id, e))?;
    let issuer_pk: [u8; 33] = hex::decode(issuer_pubkey)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Issuer public key must be 33 bytes"))?;
    crate::verify::verify_reserve_input(&reserve_box, &issuer_pk, denomination)?;
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

    let change_amount = if is_token_reserve {
        fee_input_total
            .saturating_sub(TRANSACTION_FEE)
            .saturating_sub(recipient_output_value)
    } else {
        fee_input_total.saturating_sub(TRANSACTION_FEE)
    };
    let mut outputs = vec![
        json!({
            "value": reserve_output_value,
            "ergoTree": reserve_ergo_tree,
            "creationHeight": current_height,
            "assets": reserve_output_assets,
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
            "assets": recipient_output_assets,
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
        reserve_box_id,
        tracker_box_id,
        reserve_output_value,
        recipient_output_value,
        change_amount,
        total_debt,
        is_first_redemption,
        fee_input_count: fee_inputs.len(),
        fee_input_total,
        change_address,
        issuer_signature_len: issuer_signature.len(),
        tracker_signature_len: tracker_signature.len(),
        insert_proof_len: insert_proof.len(),
        reserve_lookup_proof_len: reserve_lookup_proof.as_ref().map(|p| p.len()),
        tracker_lookup_proof_len: tracker_lookup_proof.len(),
    })
}

/// Execute a redemption with client-side signing: the tracker builds the unsigned
/// transaction and signs the fee input(s) (`POST /redemption/build`); the CLI verifies
/// the build pays this note's recipient, adds the reserve input's `proveDlog(recipient)`
/// proof locally, and submits the fully-signed transaction back (`POST
/// /redemption/submit`), which broadcasts it and syncs tracker state from the stored
/// build entry. Only the recipient's secret ever leaves this process.
#[allow(clippy::too_many_arguments)]
pub async fn execute_local_redemption(
    client: &TrackerClient,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    emergency: bool,
    tracker_box_id: Option<String>,
    change_address: Option<String>,
    recipient_secret: Option<String>,
    denomination: Option<&str>,
) -> Result<String> {
    // Resolve the denomination up front: an explicit --denomination hint is
    // validated against the stored note, and the note's own denomination then
    // drives every downstream lookup (proofs, reserve selection, state sync).
    let denom_hint = denomination
        .map(basis_store::DenomKey::parse)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid denomination: {}", e))?;
    let hint_id = denom_hint.as_ref().map(|d| d.denomination_id());

    progress!("🔍 Retrieving note information...");
    let note = client
        .get_note_denominated(issuer_pubkey, recipient_pubkey, hint_id.as_deref())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Note not found for issuer {} and recipient {}",
                issuer_pubkey,
                recipient_pubkey
            )
        })?;
    let denom = note_denom(&note);
    if let Some(hint) = &denom_hint {
        if *hint != denom {
            return Err(anyhow::anyhow!(
                "denomination mismatch: --denomination {} but the stored note is {}",
                hint.denomination_id(),
                denom.denomination_id()
            ));
        }
    }

    // The issuer signature (context var #2) covers the tracker's committed total debt and the
    // note's timestamp: the issuer signs it when it is the current account, otherwise the
    // stored note's signature is reused if it covers exactly that message.
    progress!("🔍 Fetching tracker proof for total debt...");
    let tracker_proof = client
        .get_tracker_proof(
            issuer_pubkey,
            recipient_pubkey,
            Some(&denom.denomination_id()),
        )
        .await?;
    let total_debt = tracker_proof.total_debt;

    progress!("🔑 Resolving the issuer's redemption signature...");
    let issuer_pk: [u8; 33] = hex::decode(issuer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid issuer public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Issuer public key must be 33 bytes"))?;
    let recipient_pk: [u8; 33] = hex::decode(recipient_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid recipient public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Recipient public key must be 33 bytes"))?;
    let tracker_nft_id: [u8; 32] = hex::decode(&note.tracker_nft_id)
        .map_err(|e| anyhow::anyhow!("Invalid tracker NFT ID on note: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Tracker NFT ID must be 32 bytes"))?;
    let message = redemption_signing_message(
        &issuer_pk,
        &recipient_pk,
        &note_denom(&note),
        total_debt,
        note.timestamp,
        &tracker_nft_id,
    );
    let issuer_signature =
        crate::verify::redemption_issuer_signature(&issuer_pk, &note.signature, &message)?;
    let issuer_signature_hex = hex::encode(issuer_signature);

    // The tracker builds the unsigned transaction and signs the fee input(s).
    progress!("🔍 Requesting tracker build (POST /redemption/build)...");
    let build = client
        .redemption_build(crate::api::RedemptionBuildRequest {
            issuer_pubkey: issuer_pubkey.to_string(),
            recipient_pubkey: recipient_pubkey.to_string(),
            amount,
            timestamp: note.timestamp,
            issuer_signature: issuer_signature_hex,
            emergency,
            tracker_box_id,
            change_address,
            denomination: denom.denomination_id(),
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

    // Reconstruct the signing material the tracker produced.
    let unsigned: UnsignedTransaction = serde_json::from_value(build.unsigned_tx.clone())
        .map_err(|e| anyhow::anyhow!("parse unsigned tx: {}", e))?;
    let partial: ergo_lib::chain::transaction::Transaction =
        serde_json::from_value(build.partial_tx)
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

    // Nothing the tracker built is signed with the recipient's key until it is checked
    // to be this note's redemption.
    crate::verify::verify_redemption_tx(
        &unsigned,
        &input_boxes,
        &data_boxes,
        &issuer_pk,
        &recipient_pk,
        &denom,
        amount,
    )
    .map_err(|e| anyhow::anyhow!("refusing to sign the tracker's redemption: {}", e))?;

    // Recipient (receiver) secret for the reserve input's proveDlog(recipient).
    let recipient_address = pubkey_to_address(recipient_pubkey)?;
    let recipient_secret =
        resolve_dlog_secret(&recipient_secret, client, &recipient_address, "recipient").await?;
    let recipient_sk = SecretKey::dlog_from_bytes(&recipient_secret)
        .ok_or_else(|| anyhow::anyhow!("invalid recipient dlog secret"))?;

    // Add the reserve input (index 0) proof over the same bytes_to_sign.
    progress!("🖊️  Signing the reserve input locally (proveDlog(recipient))...");
    let signed = add_input_proof(
        &unsigned,
        Some(&partial),
        &input_boxes,
        &data_boxes,
        &pre_header,
        &headers,
        0,
        &recipient_sk,
    )
    .map_err(|e| anyhow::anyhow!("reserve proof failed: {:?}", e))?;

    // Submit through the tracker: it broadcasts the fully-signed transaction and syncs
    // its note and reserve-tree state from the stored build entry (single-use).
    let tx_json = serde_json::to_value(&signed)?;
    progress!("📡 Submitting via tracker (POST /redemption/submit)...");
    let tx_id = client
        .redemption_submit(
            tx_json,
            build.unsigned_tx,
            issuer_pubkey,
            recipient_pubkey,
            amount,
            build.new_already_redeemed,
        )
        .await
        .map_err(|e| anyhow::anyhow!("submit failed: {}", e))?;

    progress!("✅ Redemption broadcast. Transaction ID: {}", tx_id);
    Ok(tx_id)
}

#[allow(clippy::too_many_arguments)]
pub async fn generate_redemption_transaction(
    client: &TrackerClient,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    output_file: Option<String>,
    emergency: bool,
    tracker_box_id: Option<String>,
    change_address: Option<String>,
    local_sign: bool,
    recipient_secret: Option<String>,
    denomination: Option<&str>,
) -> Result<GenerateRedemptionResult> {
    if local_sign {
        let tx_id = execute_local_redemption(
            client,
            issuer_pubkey,
            recipient_pubkey,
            amount,
            emergency,
            tracker_box_id,
            change_address,
            recipient_secret,
            denomination,
        )
        .await?;
        progress!("✅ Redemption broadcast. Transaction ID: {}", tx_id);
        return Ok(GenerateRedemptionResult::Broadcast(
            RedemptionBroadcastResult { tx_id },
        ));
    }

    // Resolve the denomination same as the local path: validate an explicit hint
    // against the stored note, then drive everything from the note's own denomination.
    let denom_hint = denomination
        .map(basis_store::DenomKey::parse)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid denomination: {}", e))?;
    let hint_id = denom_hint.as_ref().map(|d| d.denomination_id());

    // Fetch the note so we can compute the issuer signature with the current account.
    progress!("🔍 Retrieving note information...");
    let note = client
        .get_note_denominated(issuer_pubkey, recipient_pubkey, hint_id.as_deref())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Note not found for issuer {} and recipient {}",
                issuer_pubkey,
                recipient_pubkey
            )
        })?;
    let denom = note_denom(&note);
    if let Some(hint) = &denom_hint {
        if *hint != denom {
            return Err(anyhow::anyhow!(
                "denomination mismatch: --denomination {} but the stored note is {}",
                hint.denomination_id(),
                denom.denomination_id()
            ));
        }
    }

    // Verify that the redemption amount does not exceed the note's outstanding debt
    if note.outstanding_debt() < amount {
        return Err(anyhow::anyhow!(
            "Insufficient outstanding debt: {} nanoERG available, {} nanoERG requested",
            note.outstanding_debt(),
            amount
        ));
    }

    // Issuer signature: the stored note's, verified locally.
    progress!("🔑 Resolving the issuer's redemption signature...");

    let issuer_pk: [u8; 33] = hex::decode(issuer_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid issuer public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Issuer public key must be 33 bytes"))?;
    let recipient_pk: [u8; 33] = hex::decode(recipient_pubkey)
        .map_err(|e| anyhow::anyhow!("Invalid recipient public key: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Recipient public key must be 33 bytes"))?;

    let tracker_nft_id: [u8; 32] = hex::decode(&note.tracker_nft_id)
        .map_err(|e| anyhow::anyhow!("Invalid tracker NFT ID on note: {}", e))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Tracker NFT ID must be 32 bytes"))?;
    let message = redemption_signing_message(
        &issuer_pk,
        &recipient_pk,
        &note_denom(&note),
        note.amount_collected,
        note.timestamp,
        &tracker_nft_id,
    );
    let issuer_signature =
        crate::verify::redemption_issuer_signature(&issuer_pk, &note.signature, &message)?;
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
        &denom,
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

/// Tracker-assisted redemption driver (same flow as the TUI and `note redeem`): the
/// tracker builds the unsigned transaction and signs the fee input(s); the CLI supplies
/// the issuer signature (the stored note's, verified locally), checks the build, and adds
/// the reserve input's proveDlog(recipient) proof, then submits via the tracker.
pub async fn redeem_tracker_assisted(
    client: &TrackerClient,
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    recipient_secret: Option<String>,
    denomination: Option<&str>,
) -> Result<RedemptionBroadcastResult> {
    let tx_id = execute_local_redemption(
        client,
        issuer_pubkey,
        recipient_pubkey,
        amount,
        false, // emergency
        None,  // tracker_box_id
        None,  // change_address
        recipient_secret,
        denomination,
    )
    .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Reserve-denomination matching for local reserve selection
    // ------------------------------------------------------------------

    const TOKEN_ID_A_HEX: &str = "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669";

    fn token_denom() -> basis_store::DenomKey {
        basis_store::DenomKey::Token(hex::decode(TOKEN_ID_A_HEX).unwrap().try_into().unwrap())
    }

    #[test]
    fn erg_denomination_matches_only_erg_reserves() {
        assert!(denom_matches_reserve("", &basis_store::DenomKey::Erg));
        assert!(!denom_matches_reserve(
            TOKEN_ID_A_HEX,
            &basis_store::DenomKey::Erg
        ));
        assert!(!denom_matches_reserve("", &token_denom()));
    }

    #[test]
    fn token_denomination_matches_only_its_token() {
        // Address computed case-insensitively: scanner may store either case.
        assert!(denom_matches_reserve(TOKEN_ID_A_HEX, &token_denom()));
        assert!(denom_matches_reserve(
            &TOKEN_ID_A_HEX.to_ascii_uppercase(),
            &token_denom()
        ));
        // A different token (or empty = ERG reserve) is not this denomination.
        let other = "b8".repeat(32);
        assert!(!denom_matches_reserve(&other, &token_denom()));
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
    fn address_round_trip_yields_p2pk_ergo_tree() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let address = pubkey_to_address(pubkey).unwrap();
        let tree = address_to_ergo_tree(&address).unwrap();
        assert_eq!(tree, format!("0008cd{}", pubkey));
    }
}
