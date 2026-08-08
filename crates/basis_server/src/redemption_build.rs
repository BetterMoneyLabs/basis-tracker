//! Tracker-assisted 2-phase redemption build/submit endpoints.
//!
//! This mirrors the proven CLI local-sign flow (`basis_cli::commands::transaction`), split across
//! two parties:
//!   * `POST /redemption/build`  — the tracker constructs the unsigned redemption transaction
//!     (same byte layout the node and reserve contract accept, context extension in Scala order),
//!     selects wallet fee inputs, and signs ONLY the fee input(s) locally with its own secret.
//!     It returns the unsigned tx, the fee-signed partial tx, and the sigma-serialized input/data
//!     boxes the client needs to finish signing.
//!   * the client (TUI) adds ONLY the reserve input's `proveDlog(receiver)` proof over the same
//!     `bytes_to_sign`, then calls `POST /redemption/submit` with the fully-signed tx.
//!   * `POST /redemption/submit` — the tracker broadcasts the fully-signed transaction.
//!
//! Neither end reorders the reserve-input context extension: the tracker emits it in Scala
//! `ContextExtension` order, so both ends sign an identical `bytes_to_sign`.

// Axum handlers returning `(StatusCode, Json<ApiResponse<..>>)` have a large `Err` variant by
// construction; boxing the error would add noise without benefit here.
#![allow(clippy::result_large_err)]

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use ergo_lib::chain::ergo_state_context::ErgoStateContext;
use ergo_lib::chain::parameters::Parameters;
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::wallet::secret_key::SecretKey;

use basis_offchain::ergo_tx::{
    address_to_ergo_tree, build_savl_tree_from_digest, decode_box_id, ergo_tree_to_p2pk_address,
    pubkey_to_address, reorder_reserve_extension_scala, serialize_coll_bytes, serialize_ergo_byte,
    serialize_ergo_long,
};
use basis_offchain::signing::add_input_proof;

use crate::models::{error_response, success_response, ApiResponse};
use crate::{AppState, TrackerCommand};

/// Minimum post-redemption reserve output value (0.001 ERG min box value).
const MIN_RESERVE_REMAINDER: u64 = 1_000_000;
/// Miner-fee recipient contract from the Scala reference implementation.
const FEE_CONTRACT_TREE: &str = "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304";

// ----- request / response models -----

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedemptionBuildRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    /// Note payment timestamp (ms since Unix epoch), from the note.
    pub timestamp: u64,
    /// Issuer's 65-byte Schnorr signature (hex) over
    /// `signing_message(issuer || receiver || totalDebt || timestamp)`.
    pub issuer_signature: String,
    #[serde(default)]
    pub emergency: bool,
    /// Optional tracker box id; fetched from storage if omitted.
    #[serde(default)]
    pub tracker_box_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionBuildResponse {
    /// Node-canonical unsigned transaction (reserve-input extension in Scala order).
    pub unsigned_tx: Value,
    /// Fee-signed transaction (reserve input proof empty); base for the client to splice the
    /// reserve proof into.
    pub partial_tx: Value,
    /// Sigma-serialized input boxes (hex), in transaction-input order.
    pub input_box_binaries: Vec<String>,
    /// Sigma-serialized data-input boxes (hex).
    pub data_box_binaries: Vec<String>,
    /// Last 10 block headers; the client derives the `PreHeader` and `ErgoStateContext` from these
    /// to finish signing without needing its own node connection.
    pub headers: Vec<Header>,
    pub reserve_box_id: String,
    pub tracker_box_id: String,
    pub reserve_output_value: u64,
    pub recipient_output_value: u64,
    pub total_debt: u64,
    pub change_amount: u64,
    pub change_address: String,
    pub recipient_address: String,
    pub is_first_redemption: bool,
    pub fee: u64,
    /// Cumulative `already_redeemed` value this redemption inserts into the reserve tree
    /// (previous reserve-tree value + redeemed amount). The client round-trips it to
    /// `/redemption/submit` so the tracker can sync its local reserve tree with the value
    /// that was actually proven on-chain.
    pub new_already_redeemed: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedemptionSubmitRequest {
    pub signed_tx: Value,
}

#[derive(Debug, Serialize)]
pub struct RedemptionSubmitResponse {
    pub tx_id: String,
}

// ----- node client -----

#[derive(Debug, Clone, Deserialize)]
struct NodeAsset {
    #[serde(alias = "tokenId")]
    token_id: String,
    amount: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct NodeBox {
    #[serde(alias = "boxId")]
    box_id: String,
    value: u64,
    #[serde(alias = "ergoTree")]
    ergo_tree: String,
    #[serde(default)]
    assets: Vec<NodeAsset>,
    #[serde(default, alias = "additionalRegisters")]
    additional_registers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct VerifiedFeeBox {
    box_id: String,
    value: u64,
    ergo_tree: String,
    assets: Vec<NodeAsset>,
    binary: String,
    ergo_box: ErgoBox,
}

impl NodeBox {
    /// The 33-byte reserve AVL tree digest held in R5, if present. R5 serializes as
    /// `0x64 || 33-byte digest || flags || keylen || valuelen`, so the digest is hex chars [2..68].
    fn r5_digest_hex(&self) -> Option<String> {
        let r5 = self.additional_registers.get("R5")?;
        if r5.len() >= 68 && r5.starts_with("64") {
            Some(r5[2..68].to_string())
        } else {
            None
        }
    }
}

struct NodeClient {
    url: String,
    api_key: Option<String>,
}

impl NodeClient {
    fn from_state(state: &AppState) -> Self {
        NodeClient {
            url: state
                .config
                .ergo
                .node
                .node_url
                .trim_end_matches('/')
                .to_string(),
            api_key: state
                .config
                .ergo
                .node
                .api_key
                .clone()
                .filter(|k| !k.is_empty()),
        }
    }

    async fn get_json(&self, path: &str) -> Result<Value, String> {
        let client = reqwest::Client::new();
        let url = format!("{}{}", self.url, path);
        let mut req = client.get(&url);
        if let Some(ref k) = self.api_key {
            req = req.header("api_key", k);
        }
        let resp = req.send().await.map_err(|e| format!("GET {url}: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("GET {url} HTTP {status}: {body}"));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| format!("GET {url} json: {e}"))
    }

    async fn box_binary(&self, box_id: &str) -> Result<String, String> {
        let v = self.get_json(&format!("/utxo/byIdBinary/{box_id}")).await?;
        v["bytes"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| format!("missing 'bytes' for box {box_id}"))
    }

    async fn box_details(&self, box_id: &str) -> Result<NodeBox, String> {
        let v = self.get_json(&format!("/utxo/byId/{box_id}")).await?;
        serde_json::from_value(v).map_err(|e| format!("parse box {box_id}: {e}"))
    }

    async fn wallet_boxes(&self) -> Result<Vec<NodeBox>, String> {
        let v = self
            .get_json("/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1")
            .await?;
        #[derive(Deserialize)]
        struct Entry {
            #[serde(rename = "box")]
            details: NodeBox,
        }
        let entries: Vec<Entry> =
            serde_json::from_value(v).map_err(|e| format!("parse wallet boxes: {e}"))?;
        Ok(entries.into_iter().map(|e| e.details).collect())
    }

    async fn last_headers(&self) -> Result<Vec<Header>, String> {
        let v = self.get_json("/blocks/lastHeaders/10").await?;
        serde_json::from_value(v).map_err(|e| format!("parse last headers: {e}"))
    }

    async fn broadcast(&self, tx: &Value) -> Result<String, String> {
        let client = reqwest::Client::new();
        let url = format!("{}/transactions", self.url);
        let mut req = client.post(&url).json(tx);
        if let Some(ref k) = self.api_key {
            req = req.header("api_key", k);
        }
        let resp = req.send().await.map_err(|e| format!("broadcast: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("broadcast rejected HTTP {status}: {body}"));
        }
        Ok(parse_broadcast_tx_id(&body))
    }
}

fn parse_broadcast_tx_id(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        match value {
            Value::String(s) => return s,
            Value::Object(map) => {
                if let Some(Value::String(id)) = map.get("id") {
                    return id.clone();
                }
            }
            _ => {}
        }
    }
    body.trim().trim_matches('"').to_string()
}

/// Select wallet boxes covering the required fee, excluding the reserve box and token-bearing boxes.
fn select_fee_inputs(
    wallet_boxes: &[NodeBox],
    required: u64,
    reserve_box_id: &str,
) -> Option<(Vec<NodeBox>, u64)> {
    let mut candidates: Vec<&NodeBox> = wallet_boxes
        .iter()
        .filter(|b| b.box_id != reserve_box_id && b.assets.is_empty())
        .collect();
    candidates.sort_by_key(|b| b.value);

    if let Some(b) = candidates.iter().find(|b| b.value >= required) {
        return Some((vec![(*b).clone()], b.value));
    }
    let mut selected = Vec::new();
    let mut total = 0u64;
    for b in candidates {
        total += b.value;
        selected.push(b.clone());
        if total >= required {
            return Some((selected, total));
        }
    }
    None
}

fn decoded_hex_eq(left: &str, right: &str) -> bool {
    match (hex::decode(left), hex::decode(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn assets_match(claimed: &[NodeAsset], canonical: &[NodeAsset]) -> bool {
    claimed.len() == canonical.len()
        && claimed.iter().zip(canonical).all(|(left, right)| {
            decoded_hex_eq(&left.token_id, &right.token_id) && left.amount == right.amount
        })
}

/// Parse the exact Sigma bytes later supplied to the prover and reject any
/// disagreement with the wallet-list JSON that was used only for selection.
fn verify_fee_box_claim(claimed: &NodeBox, binary: String) -> Result<VerifiedFeeBox, String> {
    let bytes =
        hex::decode(&binary).map_err(|e| format!("fee box {} binary hex: {e}", claimed.box_id))?;
    let ergo_box = ErgoBox::sigma_parse_bytes(&bytes)
        .map_err(|e| format!("fee box {} Sigma parse: {e:?}", claimed.box_id))?;

    let box_id = ergo_box.box_id().to_string();
    let value = *ergo_box.value.as_u64();
    let ergo_tree = hex::encode(
        ergo_box
            .ergo_tree
            .sigma_serialize_bytes()
            .map_err(|e| format!("fee box {} script serialize: {e:?}", claimed.box_id))?,
    );
    let assets: Vec<NodeAsset> = ergo_box
        .tokens
        .as_ref()
        .map(|tokens| {
            tokens
                .iter()
                .map(|token| NodeAsset {
                    token_id: hex::encode(token.token_id.as_ref()),
                    amount: *token.amount.as_u64(),
                })
                .collect()
        })
        .unwrap_or_default();

    if !decoded_hex_eq(&claimed.box_id, &box_id) {
        return Err(format!(
            "fee box id mismatch: wallet list claimed {}, exact Sigma box is {}",
            claimed.box_id, box_id
        ));
    }
    if claimed.value != value {
        return Err(format!(
            "fee box {} value mismatch: wallet list claimed {}, exact Sigma box is {}",
            box_id, claimed.value, value
        ));
    }
    if !decoded_hex_eq(&claimed.ergo_tree, &ergo_tree) {
        return Err(format!(
            "fee box {} script mismatch between wallet list and exact Sigma box",
            box_id
        ));
    }
    if !assets_match(&claimed.assets, &assets) {
        return Err(format!(
            "fee box {} assets mismatch between wallet list and exact Sigma box",
            box_id
        ));
    }

    Ok(VerifiedFeeBox {
        box_id,
        value,
        ergo_tree,
        assets,
        binary,
        ergo_box,
    })
}

fn authoritative_fee_total(fee_boxes: &[VerifiedFeeBox]) -> Result<u64, String> {
    fee_boxes.iter().try_fold(0u64, |total, box_| {
        total
            .checked_add(box_.value)
            .ok_or_else(|| "fee input value overflow".to_string())
    })
}

/// Derive fee-input change from the exact script whose inputs the tracker signs.
/// Mixed-script funding is rejected because one shared change output would have
/// ambiguous ownership even if global value conservation still held.
fn fee_change_address(fee_boxes: &[VerifiedFeeBox]) -> Result<String, String> {
    let first = fee_boxes
        .first()
        .ok_or_else(|| "no selected fee inputs".to_string())?;
    if fee_boxes
        .iter()
        .any(|fee_box| fee_box.ergo_tree != first.ergo_tree)
    {
        return Err("selected fee inputs do not share one owner script".to_string());
    }
    ergo_tree_to_p2pk_address(&first.ergo_tree)
}

type ApiResult<T> = Result<T, (StatusCode, Json<ApiResponse<RedemptionBuildResponse>>)>;

fn api_err<T>(status: StatusCode, msg: impl Into<String>) -> ApiResult<T> {
    Err((status, Json(error_response(msg.into()))))
}

// ----- handlers -----

/// Build the unsigned redemption transaction and sign the fee input(s) locally.
#[axum::debug_handler]
pub async fn build_redemption(
    State(state): State<AppState>,
    Json(payload): Json<RedemptionBuildRequest>,
) -> (StatusCode, Json<ApiResponse<RedemptionBuildResponse>>) {
    match build_redemption_inner(&state, &payload).await {
        Ok(resp) => (StatusCode::OK, Json(success_response(resp))),
        Err(e) => e,
    }
}

async fn build_redemption_inner(
    state: &AppState,
    payload: &RedemptionBuildRequest,
) -> ApiResult<RedemptionBuildResponse> {
    if let Err(e) = state.config.reject_unsupported_reserve_builder() {
        return api_err(StatusCode::SERVICE_UNAVAILABLE, e);
    }

    // Validate public keys.
    let issuer_pubkey_bytes = match hex::decode(&payload.issuer_pubkey) {
        Ok(b) if b.len() == 33 => b,
        _ => return api_err(StatusCode::BAD_REQUEST, "issuer_pubkey must be 33-byte hex"),
    };
    let recipient_pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
        Ok(b) if b.len() == 33 => b,
        _ => {
            return api_err(
                StatusCode::BAD_REQUEST,
                "recipient_pubkey must be 33-byte hex",
            )
        }
    };
    let issuer_pk: [u8; 33] = issuer_pubkey_bytes.clone().try_into().unwrap();
    let recipient_pk: [u8; 33] = recipient_pubkey_bytes.clone().try_into().unwrap();
    let issuer_pubkey: basis_store::PubKey = issuer_pk;
    let recipient_pubkey: basis_store::PubKey = recipient_pk;

    let recipient_address = match pubkey_to_address(&payload.recipient_pubkey) {
        Ok(a) => a,
        Err(e) => return api_err(StatusCode::BAD_REQUEST, format!("recipient address: {e}")),
    };

    let node = NodeClient::from_state(state);

    // Look up the note to confirm it exists (timestamp/total debt are authoritative from proofs).
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetNoteByIssuerAndRecipient {
                issuer_pubkey,
                recipient_pubkey,
                response_tx: tx,
            })
            .await
            .is_err()
        {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tracker thread unavailable",
            );
        }
        match rx.await {
            Ok(Ok(Some(_note))) => {}
            Ok(Ok(None)) => {
                return api_err(
                    StatusCode::BAD_REQUEST,
                    "note not found for issuer/recipient",
                )
            }
            Ok(Err(e)) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("note lookup failed: {e:?}"),
                )
            }
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        }
    }

    // Select the smallest reserve that covers the redemption, leaves a valid remainder, is actually
    // unspent on-chain, AND whose on-chain R5 (reserve AVL tree) matches the tracker's current
    // reserve tree digest — otherwise the insert proof cannot verify on-chain.
    let (reserve_box_id, selected_reserve_digest) = {
        // The tracker's current reserve tree root digest; the spent reserve's R5 must equal this.
        let tracker_reserve_digest = {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if state
                .tx
                .send(TrackerCommand::GetReserveStateDigest { response_tx: tx })
                .await
                .is_err()
            {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "tracker thread unavailable",
                );
            }
            match rx.await {
                Ok(Ok(d)) => d,
                Ok(Err(e)) => {
                    return api_err(
                        StatusCode::SERVICE_UNAVAILABLE,
                        format!("tracker reserve state unavailable: {e:?}"),
                    )
                }
                Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
            }
        };
        let tracker_reserve_digest_hex = hex::encode(&tracker_reserve_digest);

        let scanner = state.ergo_scanner.lock().await;
        let reserves = match scanner.reserve_storage().get_all_reserves() {
            Ok(r) => r,
            Err(e) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("read reserves: {e:?}"),
                )
            }
        };
        drop(scanner);
        let normalized_issuer = basis_store::normalize_public_key(&payload.issuer_pubkey);
        let required = payload.amount.saturating_add(MIN_RESERVE_REMAINDER);

        // Gather matching candidates (issuer owner, sufficient collateral), smallest first.
        let mut candidates: Vec<(String, u64)> = Vec::new();
        for reserve in &reserves {
            // Owner key may be double-hex-encoded in the DB.
            let owner = if let Ok(b) = hex::decode(&reserve.owner_pubkey) {
                if let Ok(s) = String::from_utf8(b.clone()) {
                    if s.chars().all(|c| c.is_ascii_hexdigit()) {
                        s
                    } else {
                        reserve.owner_pubkey.clone()
                    }
                } else {
                    reserve.owner_pubkey.clone()
                }
            } else {
                reserve.owner_pubkey.clone()
            };
            if basis_store::normalize_public_key(&owner) != normalized_issuer {
                continue;
            }
            let collateral = reserve.base_info.collateral_amount;
            if collateral < required {
                continue;
            }
            candidates.push((decode_box_id(&reserve.box_id), collateral));
        }
        candidates.sort_by_key(|(_, c)| *c);

        // Pick the smallest candidate that is unspent on-chain and whose R5 matches the tracker's
        // current reserve tree digest.
        let mut found: Option<String> = None;
        for (box_id, _) in &candidates {
            match node.box_details(box_id).await {
                Ok(b) if b.value >= required => match b.r5_digest_hex() {
                    Some(d) if d == tracker_reserve_digest_hex => {
                        found = Some(box_id.clone());
                        break;
                    }
                    Some(d) => {
                        tracing::warn!(
                            "reserve {} R5 {} != tracker tree {}; skipping",
                            box_id,
                            &d[..16.min(d.len())],
                            &tracker_reserve_digest_hex[..16.min(tracker_reserve_digest_hex.len())]
                        );
                    }
                    None => {
                        tracing::warn!("reserve {} has no R5 digest; skipping", box_id);
                    }
                },
                _ => {
                    tracing::warn!("skipping unavailable/insufficient reserve box {}", box_id);
                }
            }
        }
        match found {
            Some(id) => (id, tracker_reserve_digest),
            None => {
                return api_err(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "no unspent reserve with >= {required} nanoERG collateral and a reserve tree matching the tracker (digest {}) for issuer {}",
                        &tracker_reserve_digest_hex
                            [..16.min(tracker_reserve_digest_hex.len())],
                        payload.issuer_pubkey
                    ),
                )
            }
        }
    };

    // Tracker box id (data input). Prefer the live shared state over the static
    // tracker storage, which can contain spent boxes.
    let tracker_box_id = match &payload.tracker_box_id {
        Some(id) => id.clone(),
        None => {
            let shared = state.shared_tracker_state.lock().await;
            match shared
                .get_tracker_box_id()
                .or_else(|| shared.get_confirmed().box_id)
            {
                Some(id) => id,
                None => {
                    return api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "no tracker box in live state",
                    )
                }
            }
        }
    };

    let tracker_nft_id = match &state.config.ergo.tracker_nft_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tracker NFT id not configured",
            )
        }
    };

    // Tracker lookup proof (context var #8) + authoritative total debt.
    let (tracker_lookup_proof, total_debt) = {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetTrackerLookupProof {
                issuer_pubkey,
                recipient_pubkey,
                response_tx: tx,
            })
            .await
            .is_err()
        {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tracker thread unavailable",
            );
        }
        match rx.await {
            Ok(Ok((proof, _state))) => {
                let total_debt = if proof.value.len() == 8 {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&proof.value);
                    u64::from_be_bytes(b)
                } else {
                    0
                };
                (proof.proof, total_debt)
            }
            Ok(Err(e)) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("tracker proof: {e:?}"),
                )
            }
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        }
    };

    // Reserve proofs (insert #5, lookup #7, new R5 digest) plus the cumulative
    // `already_redeemed` value placed into the reserve tree by this redemption.
    let (
        reserve_lookup_proof,
        insert_proof,
        new_reserve_digest,
        is_first_redemption,
        new_already_redeemed,
    ) = {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetReserveLookupProof {
                issuer_pubkey,
                recipient_pubkey,
                response_tx: tx,
            })
            .await
            .is_err()
        {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tracker thread unavailable",
            );
        }
        let (lookup, lookup_root) = match rx.await {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("reserve lookup proof: {e:?}"),
                )
            }
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        };
        let already_redeemed = if lookup.value.len() == 16 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&lookup.value[8..16]);
            u64::from_be_bytes(b)
        } else if lookup.value.len() == 8 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&lookup.value);
            u64::from_be_bytes(b)
        } else {
            0
        };
        let is_first = lookup.proof.is_none();
        let new_already_redeemed = already_redeemed + payload.amount;

        let (itx, irx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetReserveInsertProof {
                issuer_pubkey,
                recipient_pubkey,
                timestamp: payload.timestamp,
                new_already_redeemed,
                response_tx: itx,
            })
            .await
            .is_err()
        {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tracker thread unavailable",
            );
        }
        let (insert_bytes, new_digest, insert_current_root) = match irx.await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("reserve insert proof: {e:?}"),
                )
            }
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        };
        if lookup_root != insert_current_root || lookup_root != selected_reserve_digest {
            return api_err(
                StatusCode::CONFLICT,
                "reserve state changed while building proofs; retry from a fresh snapshot",
            );
        }
        (
            lookup.proof,
            insert_bytes,
            new_digest,
            is_first,
            new_already_redeemed,
        )
    };

    // Issuer signature (context var #2) supplied by the caller.
    let issuer_signature = match hex::decode(payload.issuer_signature.trim()) {
        Ok(b) if b.len() == 65 => b,
        _ => {
            return api_err(
                StatusCode::BAD_REQUEST,
                "issuer_signature must be 65-byte hex",
            )
        }
    };

    // Tracker signature (context var #6), signed locally with the configured tracker secret.
    let tracker_signature: Vec<u8> = if !payload.emergency {
        let tracker_secret = match state.config.tracker_secret_key_bytes() {
            Some(s) => s,
            None => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "tracker secret key not configured for local signing",
                )
            }
        };
        let tracker_pubkey_bytes = match state.config.tracker_public_key_bytes() {
            Ok(Some(p)) => p,
            _ => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "tracker public key not configured",
                )
            }
        };
        let message = basis_store::schnorr::signing_message(
            &issuer_pk,
            &recipient_pk,
            total_debt,
            payload.timestamp,
        );
        match basis_store::schnorr::schnorr_sign(&message, &tracker_secret, &tracker_pubkey_bytes) {
            Ok(sig) => sig.to_vec(),
            Err(e) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("tracker schnorr sign: {e:?}"),
                )
            }
        }
    } else {
        Vec::new()
    };

    // Fetch node data (height) — node client created above.
    let current_height = {
        let scanner = state.ergo_scanner.lock().await;
        match scanner.get_current_height().await {
            Ok(h) => h,
            Err(e) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("current height: {e}"),
                )
            }
        }
    };

    let reserve_box = match node.box_details(&reserve_box_id).await {
        Ok(b) => b,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("reserve box: {e}"),
            )
        }
    };
    let reserve_nft_id = reserve_box
        .assets
        .first()
        .map(|a| a.token_id.clone())
        .unwrap_or_else(|| tracker_nft_id.clone());

    let wallet_boxes = match node.wallet_boxes().await {
        Ok(b) => b,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("wallet boxes: {e}"),
            )
        }
    };
    let fee = state.config.transaction_fee();
    let (fee_box_claims, _) = match select_fee_inputs(&wallet_boxes, fee, &reserve_box_id) {
        Some(v) => v,
        None => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("no wallet boxes covering {fee} nanoERG fee"),
            )
        }
    };

    // `/wallet/boxes/unspent` is selection metadata only. Fetch and parse the
    // exact bytes that will be supplied to the prover, then bind every
    // transaction-relevant field to those bytes.
    let mut fee_boxes = Vec::with_capacity(fee_box_claims.len());
    for claim in &fee_box_claims {
        let binary = match node.box_binary(&claim.box_id).await {
            Ok(binary) => binary,
            Err(e) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("fee bin {}: {e}", claim.box_id),
                )
            }
        };
        let verified = match verify_fee_box_claim(claim, binary) {
            Ok(verified) => verified,
            Err(e) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, e),
        };
        fee_boxes.push(verified);
    }
    let fee_total = match authoritative_fee_total(&fee_boxes) {
        Ok(total) if total >= fee => total,
        Ok(total) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("exact fee inputs provide {total} nanoERG, below required {fee} nanoERG"),
            )
        }
        Err(e) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    if fee_boxes.iter().any(|fee_box| !fee_box.assets.is_empty()) {
        return api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "exact fee inputs unexpectedly contain tokens",
        );
    }

    // Change belongs to the authenticated owner of every selected fee input.
    let change_address = match fee_change_address(&fee_boxes) {
        Ok(address) => address,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("fee change: {e}"),
            )
        }
    };

    // Build the reserve R5 register (updated reserve state as SAvlTree constant).
    let r5_bytes = build_savl_tree_from_digest(&hex::encode(&new_reserve_digest));
    let r5_hex = hex::encode(&r5_bytes);

    // Preserve the refund initiation height from the spent reserve box's R7 register.
    let refund_initiation_height = basis_store::ergo_scanner::decode_ergo_long_register(
        reserve_box.additional_registers.get("R7"),
    );

    // Context extension (Scala order applied below, before parsing).
    let mut context_extension: HashMap<String, String> = HashMap::new();
    context_extension.insert("0".to_string(), serialize_ergo_byte(0));
    context_extension.insert("1".to_string(), format!("07{}", payload.recipient_pubkey));
    context_extension.insert("2".to_string(), serialize_coll_bytes(&issuer_signature));
    context_extension.insert("3".to_string(), serialize_ergo_long(total_debt as i64));
    context_extension.insert(
        "4".to_string(),
        serialize_ergo_long(payload.timestamp as i64),
    );
    context_extension.insert("5".to_string(), serialize_coll_bytes(&insert_proof));
    if !payload.emergency {
        context_extension.insert("6".to_string(), serialize_coll_bytes(&tracker_signature));
    }
    if let Some(ref lp) = reserve_lookup_proof {
        context_extension.insert("7".to_string(), serialize_coll_bytes(lp));
    }
    context_extension.insert("8".to_string(), serialize_coll_bytes(&tracker_lookup_proof));

    // Output ergoTrees.
    let reserve_ergo_tree = match address_to_ergo_tree(state.config.basis_reserve_contract_p2s()) {
        Ok(t) => t,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("reserve contract P2S: {e}"),
            )
        }
    };
    let recipient_ergo_tree = match address_to_ergo_tree(&recipient_address) {
        Ok(t) => t,
        Err(e) => return api_err(StatusCode::BAD_REQUEST, format!("recipient tree: {e}")),
    };
    let change_ergo_tree = match address_to_ergo_tree(&change_address) {
        Ok(t) => t,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("change tree: {e}"),
            )
        }
    };

    let reserve_output_value = reserve_box.value.saturating_sub(payload.amount);
    if reserve_output_value == 0 {
        return api_err(
            StatusCode::BAD_REQUEST,
            "reserve output value would be zero after redemption",
        );
    }
    let recipient_output_value = payload.amount;
    let change_amount = fee_total.saturating_sub(fee);

    // Inputs: reserve (index 0) then fee inputs.
    let mut inputs = vec![json!({ "boxId": reserve_box_id, "extension": context_extension })];
    for fb in &fee_boxes {
        inputs.push(json!({ "boxId": fb.box_id, "extension": json!({}) }));
    }

    let mut outputs = vec![
        json!({
            "value": reserve_output_value,
            "ergoTree": reserve_ergo_tree,
            "creationHeight": current_height,
            "assets": [ { "tokenId": reserve_nft_id, "amount": 1 } ],
            "additionalRegisters": {
                "R4": format!("07{}", payload.issuer_pubkey),
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
            "value": fee,
            "ergoTree": FEE_CONTRACT_TREE,
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

    let mut unsigned_tx = json!({
        "inputs": inputs,
        "dataInputs": [ { "boxId": tracker_box_id } ],
        "outputs": outputs
    });

    // Reorder reserve-input extension into Scala ContextExtension order before parsing.
    reorder_reserve_extension_scala(&mut unsigned_tx);

    // Parse boxes for local fee signing.
    let reserve_box_binary = match node.box_binary(&reserve_box_id).await {
        Ok(b) => b,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("reserve bin: {e}"),
            )
        }
    };
    let tracker_box_binary = match node.box_binary(&tracker_box_id).await {
        Ok(b) => b,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("tracker bin: {e}"),
            )
        }
    };
    let unsigned: UnsignedTransaction = match serde_json::from_value(unsigned_tx.clone()) {
        Ok(u) => u,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("parse unsigned tx: {e}"),
            )
        }
    };

    let parse_box = |hexstr: &str, label: &str| -> ApiResult<ErgoBox> {
        let bytes = hex::decode(hexstr).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(format!("{label} hex: {e}"))),
            )
        })?;
        ErgoBox::sigma_parse_bytes(&bytes).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(format!("{label} parse: {e:?}"))),
            )
        })
    };
    let reserve_ebox = parse_box(&reserve_box_binary, "reserve box")?;
    let tracker_ebox = parse_box(&tracker_box_binary, "tracker box")?;
    let fee_eboxes: Vec<ErgoBox> = fee_boxes
        .iter()
        .map(|fee_box| fee_box.ergo_box.clone())
        .collect();

    let mut input_boxes = vec![reserve_ebox];
    input_boxes.extend(fee_eboxes);
    let data_boxes = vec![tracker_ebox];

    // State context for proving (contract does not read headers/params; height from PreHeader).
    let headers = match node.last_headers().await {
        Ok(h) if h.len() >= 10 => h,
        Ok(h) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("need >=10 headers, got {}", h.len()),
            )
        }
        Err(e) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("headers: {e}")),
    };
    let pre_header = PreHeader::from(headers[0].clone());
    let headers_array: [Header; 10] = headers[..10].to_vec().try_into().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("headers array".to_string())),
        )
    })?;
    let _state_context = ErgoStateContext::new(
        pre_header.clone(),
        headers_array.clone(),
        Parameters::default(),
    );

    // Sign the fee input(s) locally with the tracker secret.
    let fee_secret = match state.config.tracker_secret_key_bytes() {
        Some(s) => s,
        None => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tracker secret key not configured",
            )
        }
    };
    let fee_sk = match SecretKey::dlog_from_bytes(&fee_secret) {
        Some(sk) => sk,
        None => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid tracker dlog secret",
            )
        }
    };

    let n_inputs = unsigned.inputs.len();
    let mut partial: Option<ergo_lib::chain::transaction::Transaction> = None;
    for fee_idx in 1..n_inputs {
        match add_input_proof(
            &unsigned,
            partial.as_ref(),
            &input_boxes,
            &data_boxes,
            &pre_header,
            &headers_array,
            fee_idx,
            &fee_sk,
        ) {
            Ok(tx) => partial = Some(tx),
            Err(e) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("fee input {fee_idx} signing: {e:?}"),
                )
            }
        }
    }

    let partial_tx = match partial {
        Some(ref tx) => serde_json::to_value(tx).unwrap_or(Value::Null),
        None => unsigned_tx.clone(), // no fee inputs (fee covered by reserve? not expected)
    };

    let mut input_box_binaries = vec![reserve_box_binary];
    input_box_binaries.extend(fee_boxes.iter().map(|fee_box| fee_box.binary.clone()));

    Ok(RedemptionBuildResponse {
        unsigned_tx,
        partial_tx,
        input_box_binaries,
        data_box_binaries: vec![tracker_box_binary],
        headers: headers_array.to_vec(),
        reserve_box_id,
        tracker_box_id,
        reserve_output_value,
        recipient_output_value,
        total_debt,
        change_amount,
        change_address,
        recipient_address,
        is_first_redemption,
        fee,
        new_already_redeemed,
    })
}

/// Broadcast a fully-signed redemption transaction to the Ergo node.
///
/// Node acceptance is not active-chain confirmation.  This endpoint deliberately
/// accepts no accounting metadata and performs no local settlement mutation;
/// the confirmed-chain reconciler owns that transition.
#[axum::debug_handler]
pub async fn submit_redemption(
    State(state): State<AppState>,
    Json(payload): Json<RedemptionSubmitRequest>,
) -> (StatusCode, Json<ApiResponse<RedemptionSubmitResponse>>) {
    let node = NodeClient::from_state(&state);
    let tx_id = match node.broadcast(&payload.signed_tx).await {
        Ok(tx_id) => tx_id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(format!("broadcast failed: {e}"))),
            )
        }
    };

    (
        StatusCode::ACCEPTED,
        Json(success_response(RedemptionSubmitResponse { tx_id })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::ergotree_ir::chain::ergo_box::{box_value::BoxValue, NonMandatoryRegisters};
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;

    #[test]
    fn submit_request_rejects_unverified_accounting_metadata() {
        let payload = serde_json::json!({
            "signed_tx": {"inputs": [], "dataInputs": [], "outputs": []},
            "issuer_pubkey": "02".repeat(33),
            "recipient_pubkey": "03".repeat(33),
            "redeemed_amount": 1,
            "new_already_redeemed": 1
        });

        assert!(serde_json::from_value::<RedemptionSubmitRequest>(payload).is_err());
    }

    fn wallet_box(id: &str, value: u64, with_token: bool) -> NodeBox {
        let assets = if with_token {
            vec![NodeAsset {
                token_id: "aa".repeat(32),
                amount: 1,
            }]
        } else {
            Vec::new()
        };
        NodeBox {
            box_id: id.to_string(),
            value,
            ergo_tree: "00".to_string(),
            assets,
            additional_registers: Default::default(),
        }
    }

    fn exact_fee_claim(pubkey: &str, value: u64, index: u16) -> (NodeBox, String) {
        let tree_hex = format!("0008cd{pubkey}");
        let tree = ErgoTree::sigma_parse_bytes(&hex::decode(&tree_hex).unwrap()).unwrap();
        let ergo_box = ErgoBox::new(
            BoxValue::try_from(value).unwrap(),
            tree,
            None,
            NonMandatoryRegisters::empty(),
            100,
            TxId::zero(),
            index,
        )
        .unwrap();
        let binary = hex::encode(ergo_box.sigma_serialize_bytes().unwrap());
        let claim = NodeBox {
            box_id: ergo_box.box_id().to_string(),
            value,
            ergo_tree: tree_hex,
            assets: Vec::new(),
            additional_registers: Default::default(),
        };
        (claim, binary)
    }

    #[test]
    fn select_fee_inputs_picks_exact_single_box() {
        let boxes = vec![
            wallet_box("a", 5_000_000, false),
            wallet_box("b", 1_000_000, false),
        ];
        let (selected, total) = select_fee_inputs(&boxes, 1_000_000, "zz").unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].box_id, "b");
        assert_eq!(total, 1_000_000);
    }

    #[test]
    fn select_fee_inputs_prefers_smallest_sufficient() {
        let boxes = vec![
            wallet_box("big", 50_000_000, false),
            wallet_box("mid", 2_000_000, false),
            wallet_box("small", 500_000, false),
        ];
        let (selected, total) = select_fee_inputs(&boxes, 1_000_000, "zz").unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].box_id, "mid");
        assert_eq!(total, 2_000_000);
    }

    #[test]
    fn select_fee_inputs_aggregates_when_no_single_box_suffices() {
        let boxes = vec![
            wallet_box("a", 400_000, false),
            wallet_box("b", 300_000, false),
            wallet_box("c", 500_000, false),
        ];
        let (selected, total) = select_fee_inputs(&boxes, 1_000_000, "zz").unwrap();
        assert_eq!(selected.len(), 3);
        assert_eq!(total, 1_200_000);
    }

    #[test]
    fn select_fee_inputs_excludes_reserve_box() {
        let boxes = vec![wallet_box("reserve", 10_000_000, false)];
        assert!(select_fee_inputs(&boxes, 1_000_000, "reserve").is_none());
    }

    #[test]
    fn select_fee_inputs_excludes_token_boxes() {
        let boxes = vec![
            wallet_box("token", 10_000_000, true),
            wallet_box("free", 2_000_000, false),
        ];
        let (selected, _) = select_fee_inputs(&boxes, 1_000_000, "zz").unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].box_id, "free");
    }

    #[test]
    fn select_fee_inputs_returns_none_when_insufficient() {
        let boxes = vec![wallet_box("a", 100_000, false)];
        assert!(select_fee_inputs(&boxes, 1_000_000, "zz").is_none());
        assert!(select_fee_inputs(&[], 1_000_000, "zz").is_none());
    }

    #[test]
    fn build_request_rejects_caller_selected_change() {
        let payload = serde_json::json!({
            "issuer_pubkey": "02".repeat(33),
            "recipient_pubkey": "03".repeat(33),
            "amount": 1,
            "timestamp": 1,
            "issuer_signature": "00".repeat(65),
            "change_address": "attacker-selected"
        });

        assert!(serde_json::from_value::<RedemptionBuildRequest>(payload).is_err());
    }

    #[test]
    fn fee_change_is_bound_to_one_input_owner_script() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let other_pubkey = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let (first_claim, first_binary) = exact_fee_claim(pubkey, 600_000, 0);
        let (second_claim, second_binary) = exact_fee_claim(pubkey, 600_000, 1);
        let first = verify_fee_box_claim(&first_claim, first_binary).unwrap();
        let second = verify_fee_box_claim(&second_claim, second_binary).unwrap();

        assert_eq!(
            fee_change_address(&[first.clone(), second.clone()]).unwrap(),
            pubkey_to_address(pubkey).unwrap()
        );

        let (other_claim, other_binary) = exact_fee_claim(other_pubkey, 600_000, 2);
        let other = verify_fee_box_claim(&other_claim, other_binary).unwrap();
        assert!(fee_change_address(&[first, other]).is_err());
    }

    #[test]
    fn exact_fee_box_rejects_id_mismatch() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let (mut claim, binary) = exact_fee_claim(pubkey, 1_000_000, 0);
        claim.box_id = "00".repeat(32);

        let error = verify_fee_box_claim(&claim, binary).unwrap_err();
        assert!(error.contains("id mismatch"));
    }

    #[test]
    fn exact_fee_box_rejects_script_mismatch() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let (mut claim, binary) = exact_fee_claim(pubkey, 1_000_000, 0);
        claim.ergo_tree = format!(
            "0008cd{}",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );

        let error = verify_fee_box_claim(&claim, binary).unwrap_err();
        assert!(error.contains("script mismatch"));
    }

    #[test]
    fn exact_fee_box_rejects_value_mismatch() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let (mut claim, binary) = exact_fee_claim(pubkey, 1_000_000, 0);
        claim.value += 1;

        let error = verify_fee_box_claim(&claim, binary).unwrap_err();
        assert!(error.contains("value mismatch"));
    }

    #[test]
    fn exact_fee_box_rejects_asset_mismatch() {
        let pubkey = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
        let (mut claim, binary) = exact_fee_claim(pubkey, 1_000_000, 0);
        claim.assets.push(NodeAsset {
            token_id: "aa".repeat(32),
            amount: 1,
        });

        let error = verify_fee_box_claim(&claim, binary).unwrap_err();
        assert!(error.contains("assets mismatch"));
    }

    fn r5_box(r5: Option<&str>) -> NodeBox {
        let mut registers = std::collections::HashMap::new();
        if let Some(v) = r5 {
            registers.insert("R5".to_string(), v.to_string());
        }
        NodeBox {
            box_id: "x".to_string(),
            value: 0,
            ergo_tree: "00".to_string(),
            assets: Vec::new(),
            additional_registers: registers,
        }
    }

    #[test]
    fn r5_digest_hex_extracts_digest_from_real_onchain_r5() {
        // Real R5 from the empty-tree reserve created on-chain (tx 2a5e653a...).
        // The new reserve uses flags 0x03 (insert+update allowed) for insertOrUpdate.
        let b = r5_box(Some(
            "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000",
        ));
        assert_eq!(
            b.r5_digest_hex().unwrap(),
            "4ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900"
        );
    }

    #[test]
    fn r5_digest_hex_handles_missing_or_malformed_r5() {
        assert!(r5_box(None).r5_digest_hex().is_none());
        // Wrong type prefix (not 0x64).
        assert!(r5_box(Some("0702aaaaaaaa")).r5_digest_hex().is_none());
        // Too short to contain a 33-byte digest.
        assert!(r5_box(Some("64abcd")).r5_digest_hex().is_none());
    }
}
