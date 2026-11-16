//! Tracker-assisted 2-phase redemption build/submit endpoints.
//!
//! This mirrors the proven CLI local-sign flow (`basis_cli::commands::transaction`), split across
//! two parties:
//!   * `POST /redemption/build`  — the tracker constructs the unsigned redemption transaction
//!     (same byte layout the node and reserve contract accept, context extension in Scala order)
//!     around caller-supplied fee inputs: the REDEEMER funds the fee (and, for token reserves,
//!     the recipient's min-ERG box) with their own P2PK boxes, so a build costs the tracker
//!     nothing (issue #15 review N17). The tracker signs nothing in the transaction.
//!     It returns the unsigned tx and the sigma-serialized input/data boxes the client needs
//!     to finish signing.
//!   * the client signs every input locally (the reserve input's `proveDlog(receiver)` proof
//!     and the fee inputs' `proveDlog`, all under the recipient key) over the same
//!     `bytes_to_sign`, then calls `POST /redemption/submit` with the fully-signed tx.
//!   * `POST /redemption/submit` — the tracker broadcasts the fully-signed transaction, but only
//!     when its echoed `unsigned_tx` matches a live entry recorded by `/redemption/build`
//!     (single-use, 30-minute expiry) and the signed transaction has the built transaction's id.
//!     The tracker state advance uses the STORED entry values, never caller-supplied amounts
//!     (PR #16 B6).
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
use std::time::{Duration, Instant};

use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::Transaction;
use ergo_lib::ergo_chain_types::Header;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

use basis_offchain::ergo_tx::{
    address_to_ergo_tree, build_savl_tree_from_digest, decode_box_id, pubkey_to_address,
    reorder_reserve_extension_scala, serialize_coll_bytes, serialize_ergo_byte,
    serialize_ergo_long,
};

use crate::models::{error_response, success_response, ApiResponse};
use crate::{AppState, TrackerCommand};

/// Minimum post-redemption reserve output value (0.001 ERG min box value).
const MIN_RESERVE_REMAINDER: u64 = 1_000_000;
/// Minimum ERG value for the recipient output box when redeeming token-backed reserves.
const TOKEN_RECIPIENT_MIN_VALUE_NANOERG: u64 = 1_000_000;
/// Miner-fee recipient contract from the Scala reference implementation.
const FEE_CONTRACT_TREE: &str = "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304";

// ----- request / response models -----

#[derive(Debug, Deserialize)]
pub struct RedemptionBuildRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    /// Legacy field, ignored by the server: the stored note's payment timestamp
    /// is used for the signature, context extension, and reserve tree value
    /// (issue #15 S4 — never sign caller-supplied values).
    pub timestamp: u64,
    /// Issuer's 65-byte Schnorr signature (hex) over
    /// `signing_message(issuer || receiver || totalDebt || timestamp)`.
    pub issuer_signature: String,
    #[serde(default)]
    pub emergency: bool,
    /// Optional tracker box id; fetched from storage if omitted.
    #[serde(default)]
    pub tracker_box_id: Option<String>,
    /// Address paying the redeemer's change output. Defaults to the recipient address.
    /// Only the redeemer's own fee inputs fund the transaction, so change is always
    /// the redeemer's own money (issue #15 review N17).
    #[serde(default)]
    pub change_address: Option<String>,
    /// Denomination of the note being redeemed ("erg" or "token:<64-hex>"); default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
    /// Box ids funding the miner fee (and, for token reserves, the recipient's
    /// min-ERG box). Every box must be unspent, token-free, and carry the recipient's
    /// P2PK ergoTree — the client signs them with the recipient secret. The tracker
    /// funds nothing (issue #15 review N17).
    pub fee_box_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionBuildResponse {
    /// Node-canonical unsigned transaction (reserve-input extension in Scala order).
    pub unsigned_tx: Value,
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

#[derive(Debug, Clone, Deserialize)]
pub struct RedemptionSubmitRequest {
    pub signed_tx: Value,
    /// The exact `unsigned_tx` value returned by `POST /redemption/build`. Its SHA-256
    /// identifies the pending build entry this submit binds to; without a live entry the
    /// submit is rejected with 409 (PR #16 B6).
    pub unsigned_tx: Value,
    /// DEPRECATED and IGNORED: the issuer is taken from the pending build entry recorded by
    /// `/redemption/build`. Kept on the wire only for client compatibility (PR #16 B6).
    pub issuer_pubkey: String,
    /// DEPRECATED and IGNORED: see `issuer_pubkey`.
    pub recipient_pubkey: String,
    /// DEPRECATED and IGNORED: the redeemed amount is taken from the pending build entry.
    /// Kept on the wire only for client compatibility (PR #16 B6).
    pub redeemed_amount: u64,
    /// DEPRECATED and IGNORED: the cumulative reserve-tree value is taken from the pending
    /// build entry. Kept on the wire only for client compatibility (PR #16 B6).
    pub new_already_redeemed: u64,
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
    #[allow(dead_code)]
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

pub(crate) struct NodeClient {
    url: String,
    api_key: Option<String>,
}

impl NodeClient {
    /// Node used for chain reads (`/utxo/*` on chain boxes, `/blocks/*`): the
    /// scanner node (`[ergo.scanner]`, extra-indexed).
    pub(crate) fn chain(state: &AppState) -> Self {
        NodeClient {
            url: state
                .config
                .ergo
                .scanner
                .node_url
                .trim_end_matches('/')
                .to_string(),
            api_key: state
                .config
                .ergo
                .scanner
                .api_key
                .clone()
                .filter(|k| !k.is_empty()),
        }
    }

    /// Node hosting the tracker wallet: `/wallet/*` (fee inputs), binaries of
    /// wallet-selected fee boxes, and transaction broadcast. Falls back to the
    /// scanner node when `[ergo.wallet]` is not configured.
    fn wallet(state: &AppState) -> Self {
        let (url, api_key) = state.config.ergo_wallet_node();
        NodeClient {
            url: url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    pub(crate) async fn get_json(&self, path: &str) -> Result<Value, String> {
        let client = basis_store::http::bounded_client();
        let url = format!("{}{}", self.url, path);
        let mut req = client.get(&url);
        if let Some(ref k) = self.api_key {
            req = req.header("api_key", k);
        }
        let resp = req.send().await.map_err(|e| format!("GET {url}: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = basis_store::http::read_body_capped(resp)
                .await
                .unwrap_or_default();
            return Err(format!("GET {url} HTTP {status}: {body}"));
        }
        basis_store::http::read_json_capped(resp)
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

    async fn last_headers(&self) -> Result<Vec<Header>, String> {
        let v = self.get_json("/blocks/lastHeaders/10").await?;
        serde_json::from_value(v).map_err(|e| format!("parse last headers: {e}"))
    }

    async fn broadcast(&self, tx: &Value) -> Result<String, String> {
        let client = basis_store::http::bounded_client();
        let url = format!("{}/transactions", self.url);
        let mut req = client.post(&url).json(tx);
        if let Some(ref k) = self.api_key {
            req = req.header("api_key", k);
        }
        let resp = req.send().await.map_err(|e| format!("broadcast: {e}"))?;
        let status = resp.status();
        let body = basis_store::http::read_body_capped(resp)
            .await
            .unwrap_or_default();
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

// ----- pending redemption registry (PR #16 B6) -----

/// How long a pending redemption entry stays usable after `/redemption/build`.
///
/// Generous enough for a client to sign and submit, short enough that a captured build
/// response is not a standing capability.
pub const PENDING_REDEMPTION_TTL: Duration = Duration::from_secs(30 * 60);

/// Largest number of outstanding pending entries. When the cap is reached the OLDEST
/// live entry is evicted to make room (issue #15 review N17): one client filling the
/// registry must not turn every other issuer's build into a 503.
const MAX_PENDING_REDEMPTIONS: usize = 1024;

/// Largest number of outstanding pending entries per issuer. A self-redeemer with many
/// notes can otherwise fill the global registry on their own (issue #15 review N17).
const MAX_PENDING_PER_ISSUER: usize = 64;

/// Why a pending-redemption entry could not be recorded.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordError {
    /// The issuer already holds [`MAX_PENDING_PER_ISSUER`] live entries.
    IssuerFull,
}

/// SHA-256 of the canonical JSON serialization of an `unsigned_tx` value.
pub type UnsignedTxFingerprint = [u8; 32];

/// Fingerprint of an unsigned transaction JSON value, as returned in `unsigned_tx` by
/// `/redemption/build` and echoed back to `/redemption/submit`.
///
/// Both sides hash the same `serde_json::Value` type, so the digest is stable for a value the
/// server itself produced; any modification by the caller changes the digest.
pub fn unsigned_tx_fingerprint(unsigned_tx: &Value) -> Result<UnsignedTxFingerprint, String> {
    use sha2::Digest as _;
    let bytes = serde_json::to_vec(unsigned_tx)
        .map_err(|e| format!("failed to serialize unsigned_tx: {e}"))?;
    Ok(sha2::Sha256::digest(&bytes).into())
}

/// Transaction id (hex) of an unsigned transaction JSON value, as the node computes it:
/// `blake2b256` of the bytes to sign, which cover inputs (with context extensions), data
/// inputs and outputs. A signed transaction has the same id as the unsigned one it signs.
pub fn unsigned_tx_id(unsigned_tx: &Value) -> Result<String, String> {
    let unsigned: UnsignedTransaction = serde_json::from_value(unsigned_tx.clone())
        .map_err(|e| format!("unsigned_tx is not a valid unsigned transaction: {e}"))?;
    Ok(String::from(unsigned.id()))
}

/// Server-side state recorded by `/redemption/build` and consumed by `/redemption/submit`.
///
/// Everything that mutates tracker state after a broadcast comes from this entry, never from
/// caller-supplied request fields (PR #16 B6).
#[derive(Debug, Clone)]
pub struct PendingRedemption {
    /// Id of the unsigned transaction the build returned. Submit only accepts a signed
    /// transaction with exactly this id.
    pub unsigned_tx_id: String,
    /// Reserve box the built transaction spends.
    pub reserve_box_id: String,
    /// Issuer (reserve owner) compressed public key, hex.
    pub issuer_pubkey: String,
    /// Recipient (creditor) compressed public key, hex.
    pub recipient_pubkey: String,
    /// Amount redeemed by the built transaction.
    pub redeemed_amount: u64,
    /// Cumulative reserve-tree `already_redeemed` value the build proved on-chain.
    pub new_already_redeemed: u64,
    /// Reserve AVL tree root digest (33 bytes) the build placed in the reserve output's R5.
    pub reserve_r5_digest: Vec<u8>,
    /// Denomination of the note being redeemed.
    pub denomination: basis_store::DenomKey,
    issued_at: Instant,
}

impl PendingRedemption {
    /// Create an entry stamped with the current time.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        unsigned_tx_id: String,
        reserve_box_id: String,
        issuer_pubkey: String,
        recipient_pubkey: String,
        redeemed_amount: u64,
        new_already_redeemed: u64,
        reserve_r5_digest: Vec<u8>,
        denomination: basis_store::DenomKey,
    ) -> Self {
        Self {
            unsigned_tx_id,
            reserve_box_id,
            issuer_pubkey,
            recipient_pubkey,
            redeemed_amount,
            new_already_redeemed,
            reserve_r5_digest,
            denomination,
            issued_at: Instant::now(),
        }
    }
}

/// Registry of outstanding, single-use pending redemptions.
#[derive(Debug, Default)]
pub struct PendingRedemptionRegistry {
    entries: std::sync::Mutex<HashMap<UnsignedTxFingerprint, PendingRedemption>>,
}

impl PendingRedemptionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a pending redemption for `fingerprint`, valid for [`PENDING_REDEMPTION_TTL`].
    ///
    /// Returns [`RecordError::IssuerFull`] when the issuer already holds the per-issuer
    /// maximum of live entries. When the global cap is reached, the oldest live entry is
    /// evicted to make room rather than rejecting the build (a victim of eviction simply
    /// rebuilds; a global 503 would let one client block every issuer).
    pub fn record(
        &self,
        fingerprint: UnsignedTxFingerprint,
        entry: PendingRedemption,
    ) -> Result<(), RecordError> {
        let mut entries = self.lock();
        Self::prune(&mut entries);
        if let std::collections::hash_map::Entry::Occupied(mut slot) = entries.entry(fingerprint) {
            slot.insert(entry);
            return Ok(());
        }
        let issuer_entries = entries
            .values()
            .filter(|e| e.issuer_pubkey == entry.issuer_pubkey)
            .count();
        if issuer_entries >= MAX_PENDING_PER_ISSUER {
            return Err(RecordError::IssuerFull);
        }
        if entries.len() >= MAX_PENDING_REDEMPTIONS {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, e)| e.issued_at)
                .map(|(k, _)| *k)
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(fingerprint, entry);
        Ok(())
    }

    /// Validate `signed_tx` against the pending entry for `fingerprint` and consume the entry.
    ///
    /// The entry is removed on the FIRST submit attempt (single-use): an unknown or expired
    /// fingerprint yields 409; a signed transaction that is not the built one (different tx
    /// id), does not spend the build's reserve box, or whose reserve-output R5 digest differs
    /// from the build's yields 400. After a failed attempt the client must call
    /// `/redemption/build` again.
    pub fn validate_and_consume(
        &self,
        fingerprint: &UnsignedTxFingerprint,
        signed_tx: &Value,
    ) -> Result<PendingRedemption, (StatusCode, String)> {
        let entry = {
            let mut entries = self.lock();
            Self::prune(&mut entries);
            entries.remove(fingerprint)
        };
        let entry =
            match entry {
                Some(e) => e,
                None => return Err((
                    StatusCode::CONFLICT,
                    "Unknown, expired or already-used unsigned_tx. Call POST /redemption/build \
                     again to obtain a fresh one."
                        .to_string(),
                )),
            };

        // The signed transaction must be exactly the transaction the build produced. Checking
        // only "spends the reserve" and "outputs[0] carries the digest" let a caller submit any
        // other transaction that touches the reserve box (e.g. an unsigned top-up with a decoy
        // box at outputs[0]) and still have the stored redemption applied. ergo-lib recomputes
        // the id from the transaction body when parsing, so a forged `id` field is rejected.
        let signed: Transaction = serde_json::from_value(signed_tx.clone()).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("signed_tx is not a valid signed transaction: {e}"),
            )
        })?;
        let signed_tx_id = String::from(signed.id());
        if signed_tx_id != entry.unsigned_tx_id {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "signed_tx {} is not the transaction {} built by the matching \
                     /redemption/build call",
                    signed_tx_id, entry.unsigned_tx_id
                ),
            ));
        }

        // The signed transaction must spend the reserve box the build selected.
        let spends_reserve = signed_tx
            .get("inputs")
            .and_then(Value::as_array)
            .map(|inputs| {
                inputs.iter().any(|input| {
                    input.get("boxId").and_then(Value::as_str)
                        == Some(entry.reserve_box_id.as_str())
                })
            })
            .unwrap_or(false);
        if !spends_reserve {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "signed_tx does not spend the reserve box {} from the matching \
                     /redemption/build call",
                    entry.reserve_box_id
                ),
            ));
        }

        // The reserve output (index 0 by construction) must carry the reserve-tree digest the
        // build computed; otherwise the tracker's local tree would diverge from what lands
        // on-chain. R5 serializes as `0x64 || 33-byte digest || flags || keylen || valuelen`.
        let expected_digest_hex = hex::encode(&entry.reserve_r5_digest);
        let r5_matches = signed_tx
            .get("outputs")
            .and_then(Value::as_array)
            .and_then(|outputs| outputs.first())
            .and_then(|o| o.get("additionalRegisters"))
            .and_then(|r| r.get("R5"))
            .and_then(Value::as_str)
            .map(|r5| {
                r5.len() >= 68
                    && r5.starts_with("64")
                    && r5[2..68].eq_ignore_ascii_case(&expected_digest_hex)
            })
            .unwrap_or(false);
        if !r5_matches {
            return Err((
                StatusCode::BAD_REQUEST,
                "signed_tx reserve output R5 does not match the reserve tree digest computed by \
                 /redemption/build"
                    .to_string(),
            ));
        }

        Ok(entry)
    }

    /// Number of live entries. Used by tests.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        let mut entries = self.lock();
        Self::prune(&mut entries);
        entries.len()
    }

    /// Drop expired entries. Called on every record/validate, so no timer is needed.
    fn prune(entries: &mut HashMap<UnsignedTxFingerprint, PendingRedemption>) {
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.issued_at) < PENDING_REDEMPTION_TTL);
    }

    /// Lock the map, recovering from a poisoned mutex.
    ///
    /// A panic while holding this lock would only ever mean the map is in a consistent state
    /// (each entry is independent), so refusing all subsequent submissions would be worse than
    /// continuing.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<UnsignedTxFingerprint, PendingRedemption>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ----- build rate limiter (PR #16 M1, reworked for issue #15 review N17) -----

/// Sliding-window rate limiter for `/redemption/build`, keyed by (issuer, recipient) pair
/// AND by issuer alone.
///
/// The per-issuer cap is what makes recipient-key rotation useless to an abuser: fresh
/// recipient keys reset the pair bucket but never the issuer bucket. Every call sweeps
/// expired attempts and drops empty buckets, so the maps stay bounded by ACTUAL recent
/// activity instead of every pair ever seen.
#[derive(Debug, Default)]
pub struct BuildRateLimiter {
    pair_buckets: std::sync::Mutex<HashMap<(String, String), Vec<Instant>>>,
    issuer_buckets: std::sync::Mutex<HashMap<String, Vec<Instant>>>,
}

impl BuildRateLimiter {
    /// Create an empty limiter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop expired attempts and empty buckets, then return the live attempt count for `key`.
    fn sweep_and_count<K: std::hash::Hash + Eq>(
        buckets: &mut HashMap<K, Vec<Instant>>,
        key: &K,
        now: Instant,
        window: Duration,
    ) -> usize {
        for attempts in buckets.values_mut() {
            attempts.retain(|t| now.duration_since(*t) < window);
        }
        buckets.retain(|_, attempts| !attempts.is_empty());
        buckets.get(key).map(Vec::len).unwrap_or(0)
    }

    /// Record a build attempt for `(issuer, recipient)`; returns Err when the pair has
    /// reached `max_per_pair` or the issuer has reached `max_per_issuer` builds within
    /// the last rolling hour.
    pub fn check_and_record(
        &self,
        issuer: &str,
        recipient: &str,
        max_per_pair: u32,
        max_per_issuer: u32,
    ) -> Result<(), String> {
        let now = Instant::now();
        let window = Duration::from_secs(3600);

        {
            let mut buckets = self
                .issuer_buckets
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let count = Self::sweep_and_count(&mut buckets, &issuer.to_string(), now, window);
            if count >= max_per_issuer as usize {
                return Err(format!(
                    "rate limit exceeded: at most {} redemption builds per hour per issuer",
                    max_per_issuer
                ));
            }
        }
        let pair_key = (issuer.to_string(), recipient.to_string());
        {
            let mut buckets = self.pair_buckets.lock().unwrap_or_else(|e| e.into_inner());
            let count = Self::sweep_and_count(&mut buckets, &pair_key, now, window);
            if count >= max_per_pair as usize {
                return Err(format!(
                    "rate limit exceeded: at most {} redemption builds per hour per \
                     (issuer, recipient) pair",
                    max_per_pair
                ));
            }
            buckets.entry(pair_key).or_default().push(now);
        }
        self.issuer_buckets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(issuer.to_string())
            .or_default()
            .push(now);
        Ok(())
    }
}

type ApiResult<T> = Result<T, (StatusCode, Json<ApiResponse<RedemptionBuildResponse>>)>;

fn api_err<T>(status: StatusCode, msg: impl Into<String>) -> ApiResult<T> {
    Err((status, Json(error_response(msg.into()))))
}

/// Outcome of trying to pick a reserve box for a redemption build.
enum ReserveSelection {
    /// (box_id, collateral, refund_height, reserve_token_id)
    Found((String, u64, u64, String)),
    /// Every otherwise-eligible candidate was skipped because its on-chain R5 does not
    /// match the tracker's reserve tree digest: the mirror has diverged from the chain
    /// (issue #15 review N14) and can be repaired from confirmed spends.
    DigestMismatch {
        box_id: String,
        chain_r5: String,
        mirror_digest: String,
    },
}

/// Select the smallest reserve that covers the redemption, leaves a valid remainder, is
/// actually unspent on-chain, AND whose on-chain R5 (reserve AVL tree) matches the tracker's
/// current reserve tree digest — otherwise the insert proof cannot verify on-chain.
async fn select_reserve_candidate(
    state: &AppState,
    issuer_pubkey: &basis_store::PubKey,
    denomination: &basis_store::DenomKey,
    payload: &RedemptionBuildRequest,
) -> ApiResult<ReserveSelection> {
    // The tracker's current reserve tree root digest; the spent reserve's R5 must equal this.
    let tracker_reserve_digest = {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetReserveStateDigest {
                issuer_pubkey: *issuer_pubkey,
                denomination: *denomination,
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
            Ok(d) => hex::encode(d),
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        }
    };

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
    let mut candidates: Vec<(String, u64, u64, String)> = Vec::new();
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
        // A note redeems only against a reserve of its own denomination
        // (specs/ergo/multi_collateral_reserves.md §6 invariant 3).
        if reserve.denomination().as_ref() != Some(denomination) {
            continue;
        }
        let collateral = reserve.base_info.collateral_amount;
        // For token-backed reserves the collateral is measured in token units,
        // so we only need to cover the redemption amount. For ERG-backed reserves
        // the collateral is the box value in nanoERG and must leave the minimum
        // remainder in the reserve box.
        let collateral_required = if reserve.base_info.reserve_token_id.is_empty() {
            required
        } else {
            payload.amount
        };
        if collateral < collateral_required {
            continue;
        }
        candidates.push((
            decode_box_id(&reserve.box_id),
            collateral,
            reserve.base_info.refund_initiation_height,
            reserve.base_info.reserve_token_id.clone(),
        ));
    }
    candidates.sort_by_key(|(_, c, _, _)| *c);

    // Pick the smallest candidate that is unspent on-chain and whose R5 matches the tracker's
    // current reserve tree digest.
    let chain_node = NodeClient::chain(state);
    let mut digest_mismatch: Option<(String, String)> = None;
    for (box_id, collateral, refund_height, reserve_token_id) in &candidates {
        // For ERG-backed reserves the box must hold enough ERG to cover the redemption
        // plus the minimum remainder. For token-backed reserves the ERG value is preserved,
        // so we only require the minimum box value.
        let min_box_value = if reserve_token_id.is_empty() {
            required
        } else {
            MIN_RESERVE_REMAINDER
        };
        match chain_node.box_details(box_id).await {
            Ok(b) if b.value >= min_box_value => match b.r5_digest_hex() {
                Some(d) if d == tracker_reserve_digest => {
                    return Ok(ReserveSelection::Found((
                        box_id.clone(),
                        *collateral,
                        *refund_height,
                        reserve_token_id.clone(),
                    )));
                }
                Some(d) => {
                    tracing::warn!(
                        "reserve {} R5 {} != tracker tree {}; skipping",
                        box_id,
                        &d[..16.min(d.len())],
                        &tracker_reserve_digest[..16.min(tracker_reserve_digest.len())]
                    );
                    digest_mismatch.get_or_insert((box_id.clone(), d));
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
    if let Some((box_id, chain_r5)) = digest_mismatch {
        return Ok(ReserveSelection::DigestMismatch {
            box_id,
            chain_r5,
            mirror_digest: tracker_reserve_digest,
        });
    }
    api_err(
        StatusCode::BAD_REQUEST,
        format!(
            "no unspent reserve with >= {required} units collateral and a reserve tree matching the tracker (digest {}) for issuer {}",
            &tracker_reserve_digest[..16.min(tracker_reserve_digest.len())],
            payload.issuer_pubkey
        ),
    )
}

/// Repair the (issuer, denomination) reserve mirror from confirmed chain spends
/// (issue #15 review N14): walk the reserve lineage backwards from the diverged box,
/// extract each redemption from the spending transaction's context extension, and let
/// the tracker thread stage-verify-apply them against the on-chain R5.
async fn repair_reserve_mirror(
    state: &AppState,
    issuer_pubkey: &basis_store::PubKey,
    denomination: &basis_store::DenomKey,
    successor_box_id: &str,
    chain_r5: &str,
    mirror_digest: &str,
) -> Result<(), String> {
    let node = NodeClient::chain(state);
    let contract_address = match denomination {
        basis_store::DenomKey::Erg => state.config.basis_reserve_contract_p2s(),
        basis_store::DenomKey::Token(_) => state.config.basis_token_reserve_contract_p2s(),
    };
    if contract_address.is_empty() {
        return Err("reserve contract address is not configured".to_string());
    }
    let collateral_token_id = match denomination {
        basis_store::DenomKey::Token(id) => Some(hex::encode(id)),
        basis_store::DenomKey::Erg => None,
    };
    let (ops, sync_point) = crate::reserve_lineage::collect_lineage_redemption_ops(
        &node,
        contract_address,
        collateral_token_id.as_deref(),
        successor_box_id,
        mirror_digest,
    )
    .await?;
    let expected_digest: [u8; 33] = hex::decode(chain_r5)
        .ok()
        .and_then(|b| b.as_slice().try_into().ok())
        .ok_or_else(|| format!("on-chain R5 digest is not 33 bytes: {chain_r5}"))?;

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .tx
        .send(TrackerCommand::ApplyChainReserveOps {
            issuer_pubkey: *issuer_pubkey,
            denomination: *denomination,
            ops,
            sync_point,
            expected_digest,
            response_tx: tx,
        })
        .await
        .map_err(|_| "tracker thread unavailable".to_string())?;
    rx.await
        .map_err(|_| "tracker thread dropped the repair".to_string())?
        .map_err(|e| format!("{e:?}"))
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

/// Sign the tracker signature (context var #6) for a redemption build.
///
/// The message is bound to the note's STORED denomination: the token reserve
/// contract verifies var #6 over
/// `blake2b256(owner || receiver || tokenId) || totalDebt || timestamp || trackerNftId`
/// (specs/ergo/multi_collateral_reserves.md §4.1). Signing the ERG debtKey instead
/// made every token-denomination tracker signature unverifiable on-chain, so
/// non-emergency token redemptions could never be redeemed through the normal window.
#[allow(clippy::too_many_arguments)]
fn sign_tracker_redemption(
    tracker_secret: &[u8; 32],
    tracker_pubkey: &basis_store::PubKey,
    issuer_pk: &basis_store::PubKey,
    recipient_pk: &basis_store::PubKey,
    denomination: &basis_store::DenomKey,
    total_debt: u64,
    timestamp: u64,
    tracker_nft_id: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let message = basis_store::schnorr::signing_message(
        issuer_pk,
        recipient_pk,
        denomination,
        total_debt,
        timestamp,
        tracker_nft_id,
    );
    basis_store::schnorr::schnorr_sign(&message, tracker_secret, tracker_pubkey)
        .map(|sig| sig.to_vec())
        .map_err(|e| format!("tracker schnorr sign: {e:?}"))
}

async fn build_redemption_inner(
    state: &AppState,
    payload: &RedemptionBuildRequest,
) -> ApiResult<RedemptionBuildResponse> {
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

    // Resolve the note denomination (default: the pair's existing note's
    // denomination when unambiguous, else ERG).
    let denomination = match crate::api::resolve_denomination(
        state,
        &issuer_pubkey,
        &recipient_pubkey,
        &payload.denomination,
    )
    .await
    {
        Ok(d) => d,
        Err(msg) => {
            return api_err(
                StatusCode::BAD_REQUEST,
                format!("invalid denomination: {msg}"),
            )
        }
    };

    let recipient_address = match pubkey_to_address(&payload.recipient_pubkey) {
        Ok(a) => a,
        Err(e) => return api_err(StatusCode::BAD_REQUEST, format!("recipient address: {e}")),
    };

    let chain_node = NodeClient::chain(state);

    // Look up the note to confirm it exists. The stored note's payment
    // timestamp is authoritative for the tracker signature, context var #4, and
    // the reserve tree value — the caller-supplied `payload.timestamp` is
    // ignored (issue #15 S4). Total debt is authoritative from the committed
    // tracker proof below.
    let note = {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetNoteByIssuerAndRecipient {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
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
            Ok(Ok(Some(note))) => note,
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
    };

    // SECURITY (PR #16 H2): fail closed when the issuer has more than one tracked reserve.
    // A note is bound to `hash(owner || receiver)`, not to a reserve box, so with two reserves
    // the same note redeems in full against EACH of them; building against one would let the
    // creditor drain the others. Refuse until the owner consolidates to a single reserve.
    if let Err(msg) =
        crate::api::ensure_single_issuer_reserve(state, &payload.issuer_pubkey, &denomination).await
    {
        return api_err(StatusCode::CONFLICT, msg);
    }

    // Select the smallest reserve that covers the redemption, leaves a valid remainder, is actually
    // unspent on-chain, AND whose on-chain R5 (reserve AVL tree) matches the tracker's current
    // reserve tree digest — otherwise the insert proof cannot verify on-chain. When selection fails
    // only because the mirror has diverged from the chain, repair the mirror from confirmed chain
    // spends once and retry (issue #15 review N14).
    let mut repair_attempted = false;
    let (reserve_box_id, reserve_collateral, reserve_refund_height, reserve_token_id) = loop {
        match select_reserve_candidate(state, &issuer_pubkey, &denomination, payload).await? {
            ReserveSelection::Found(selection) => break selection,
            ReserveSelection::DigestMismatch {
                box_id,
                chain_r5,
                mirror_digest,
            } if !repair_attempted => {
                repair_attempted = true;
                match repair_reserve_mirror(
                    state,
                    &issuer_pubkey,
                    &denomination,
                    &box_id,
                    &chain_r5,
                    &mirror_digest,
                )
                .await
                {
                    Ok(()) => {
                        tracing::info!(
                            "Repaired reserve mirror for issuer {} from confirmed chain spends; retrying selection",
                            payload.issuer_pubkey
                        );
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Reserve mirror repair failed for issuer {}: {}",
                            payload.issuer_pubkey,
                            e
                        );
                        return api_err(
                            StatusCode::BAD_REQUEST,
                            format!(
                                "reserve tree for issuer {} has diverged from the chain and repair failed: {}",
                                payload.issuer_pubkey, e
                            ),
                        );
                    }
                }
            }
            ReserveSelection::DigestMismatch { mirror_digest, .. } => {
                return api_err(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "no unspent reserve with a reserve tree matching the tracker (digest {}) for issuer {}",
                        &mirror_digest[..16.min(mirror_digest.len())],
                        payload.issuer_pubkey
                    ),
                );
            }
        }
    };

    // Acceptance-policy compliance check at redemption time: reject redemptions
    // that would newly violate another debt holder's collateralization policy.
    // Emergency redemptions bypass the tracker (contract path after the time
    // lock) and are exempt from this check.
    if state.config.redemption.enforce_acceptance_policy && !payload.emergency {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetNotesByIssuer {
                issuer_pubkey,
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
        let notes = match rx.await {
            Ok(Ok(notes)) => notes,
            Ok(Err(e)) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("note lookup failed: {e:?}"),
                )
            }
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        };

        // The miner fee is paid by explicit fee inputs, not deducted from the
        // reserve output, so the simulated post-redemption collateral only
        // decreases by the redeemed amount (fee = 0 here).
        if let Err(e) = crate::acceptance::redemption_check::check_redemption_policy_compliance(
            &issuer_pubkey,
            &recipient_pubkey,
            payload.amount,
            reserve_collateral,
            0,
            reserve_refund_height > 0,
            &notes,
            &denomination,
            &state.policy_storage,
            &state.config.acceptance,
        ) {
            use crate::acceptance::redemption_check::RedemptionPolicyError;
            let failure_id = match &e {
                RedemptionPolicyError::WouldViolatePolicy { .. } => "failed_policy_violation",
                RedemptionPolicyError::NotOldestNote { .. } => "failed_not_oldest_note",
            };
            tracing::warn!(
                "Redemption rejected by acceptance policy check ({}): {}",
                failure_id,
                e
            );
            return api_err(StatusCode::BAD_REQUEST, format!("{}: {}", failure_id, e));
        }
    }

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
    // The proof is generated against the COMMITTED on-chain tracker state (the
    // latest confirmed tracker box digest), not the live local tree: any note
    // posted after the last commitment would otherwise change the digest and
    // break every redemption (issue #15 S6). The total debt is the value
    // committed on-chain for this note.
    let (tracker_lookup_proof, total_debt) = {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetCommittedTrackerLookupProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
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
            Ok(Ok(proof)) => {
                let total_debt = if proof.value.len() == 8 {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&proof.value);
                    u64::from_be_bytes(b)
                } else {
                    0
                };
                (proof.proof, total_debt)
            }
            Ok(Err(basis_store::NoteError::NoteNotConfirmed)) => {
                return api_err(
                    StatusCode::BAD_REQUEST,
                    "note is not committed on-chain yet; wait for the next tracker box update",
                )
            }
            Ok(Err(basis_store::NoteError::StaleCommittedDebt { committed, local })) => {
                return api_err(
                    StatusCode::CONFLICT,
                    format!(
                        "the committed tracker state for this note ({}) is ahead of its current total ({}) because a consented debt decrease (exchange) has not been committed yet; building a redemption now would over-redeem. Retry after the next tracker box update (issue #17)",
                        committed, local
                    ),
                )
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
                denomination,
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
        let lookup = match rx.await {
            Ok(Ok(p)) => p,
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
        let is_first = already_redeemed == 0;
        let new_already_redeemed = match already_redeemed.checked_add(payload.amount) {
            Some(v) => v,
            None => {
                return api_err(
                    StatusCode::BAD_REQUEST,
                    "cumulative redeemed amount overflows u64",
                )
            }
        };

        let (itx, irx) = tokio::sync::oneshot::channel();
        if state
            .tx
            .send(TrackerCommand::GetReserveInsertProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                timestamp: note.timestamp,
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
        let (insert_bytes, new_digest) = match irx.await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("reserve insert proof: {e:?}"),
                )
            }
            Err(_) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "tracker unavailable"),
        };
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
        match sign_tracker_redemption(
            &tracker_secret,
            &tracker_pubkey_bytes,
            &issuer_pk,
            &recipient_pk,
            &note.denomination,
            total_debt,
            note.timestamp,
            &note.tracker_nft_id,
        ) {
            Ok(sig) => sig,
            Err(e) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, e),
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

    let reserve_box = match chain_node.box_details(&reserve_box_id).await {
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

    // Token-backed reserves preserve the reserve's ERG value and move reserve tokens.
    let is_token_reserve = !reserve_token_id.is_empty();

    let fee = state.config.transaction_fee();
    // For token reserves the fee inputs must also fund the recipient's min-value ERG box.
    let fee_input_required = if is_token_reserve {
        fee.saturating_add(TOKEN_RECIPIENT_MIN_VALUE_NANOERG)
    } else {
        fee
    };

    // The REDEEMER funds the fee (and the token-recipient min-ERG box) with their own
    // P2PK boxes (issue #15 review N17): the tracker pays nothing, so building can never
    // drain the tracker wallet. Every box must be unspent (`/utxo/byId` only serves the
    // UTXO set), token-free, and carry the recipient's P2PK ergoTree — the client signs
    // them all with the one recipient secret it already holds for the reserve input.
    if payload.fee_box_ids.is_empty() {
        return api_err(
            StatusCode::BAD_REQUEST,
            "fee_box_ids must list at least one recipient-owned box funding the fee",
        );
    }
    let recipient_p2pk_tree = match address_to_ergo_tree(&recipient_address) {
        Ok(t) => t,
        Err(e) => return api_err(StatusCode::BAD_REQUEST, format!("recipient tree: {e}")),
    };
    let mut fee_boxes: Vec<NodeBox> = Vec::with_capacity(payload.fee_box_ids.len());
    let mut seen_fee_ids = std::collections::HashSet::new();
    let mut fee_total: u64 = 0;
    for id in &payload.fee_box_ids {
        if !seen_fee_ids.insert(id.clone()) {
            return api_err(
                StatusCode::BAD_REQUEST,
                format!("duplicate fee box id {id}"),
            );
        }
        if *id == reserve_box_id {
            return api_err(
                StatusCode::BAD_REQUEST,
                "fee_box_ids must not include the reserve box being spent",
            );
        }
        let b = match chain_node.box_details(id).await {
            Ok(b) => b,
            Err(e) => {
                return api_err(
                    StatusCode::BAD_REQUEST,
                    format!("fee box {id} is not an unspent chain box: {e}"),
                )
            }
        };
        if !b.assets.is_empty() {
            return api_err(
                StatusCode::BAD_REQUEST,
                format!("fee box {id} carries tokens; fee inputs must be token-free"),
            );
        }
        if !b.ergo_tree.eq_ignore_ascii_case(&recipient_p2pk_tree) {
            return api_err(
                StatusCode::BAD_REQUEST,
                format!("fee box {id} is not a P2PK box of the recipient"),
            );
        }
        fee_total = fee_total.saturating_add(b.value);
        fee_boxes.push(b);
    }
    if fee_total < fee_input_required {
        return api_err(
            StatusCode::BAD_REQUEST,
            format!("fee boxes total {fee_total} nanoERG, need at least {fee_input_required}"),
        );
    }

    // Change goes back to the redeemer: the caller-supplied change address when given,
    // else the recipient address. Only redeemer funds ever flow through it.
    let change_address = match &payload.change_address {
        Some(a) => match address_to_ergo_tree(a) {
            Ok(_) => a.clone(),
            Err(e) => return api_err(StatusCode::BAD_REQUEST, format!("change_address: {e}")),
        },
        None => recipient_address.clone(),
    };

    // Build the reserve R5 register (updated reserve state as SAvlTree constant).
    let r5_bytes = build_savl_tree_from_digest(&hex::encode(&new_reserve_digest));
    let r5_hex = hex::encode(&r5_bytes);

    // Preserve the refund initiation height from the spent reserve box's R7 register.
    let refund_initiation_height = basis_store::ergo_scanner::decode_ergo_long_register(
        reserve_box.additional_registers.get("R7"),
    );

    // Context extension (Scala order applied below, before parsing).
    // Debt and timestamp are serialized as SIGNED Longs on-chain; values above i64::MAX
    // would wrap negative, so fail loudly instead of casting (stored notes are already
    // capped at i64::MAX — this is defense in depth for legacy databases).
    let total_debt_i64 = match i64::try_from(total_debt) {
        Ok(v) => v,
        Err(_) => {
            return api_err(
                StatusCode::CONFLICT,
                "stored totalDebt exceeds the on-chain signed-Long range".to_string(),
            )
        }
    };
    let note_timestamp_i64 = match i64::try_from(note.timestamp) {
        Ok(v) => v,
        Err(_) => {
            return api_err(
                StatusCode::CONFLICT,
                "stored note timestamp exceeds the on-chain signed-Long range".to_string(),
            )
        }
    };
    let mut context_extension: HashMap<String, String> = HashMap::new();
    context_extension.insert("0".to_string(), serialize_ergo_byte(0));
    context_extension.insert("1".to_string(), format!("07{}", payload.recipient_pubkey));
    context_extension.insert("2".to_string(), serialize_coll_bytes(&issuer_signature));
    context_extension.insert("3".to_string(), serialize_ergo_long(total_debt_i64));
    context_extension.insert("4".to_string(), serialize_ergo_long(note_timestamp_i64));
    context_extension.insert("5".to_string(), serialize_coll_bytes(&insert_proof));
    // Context var #6 must ALWAYS be present. The contract reads it with
    // `getVar[Coll[Byte]](6).get`, and `.get` on an absent var throws -- so omitting the key
    // outright made every emergency redemption fail on-chain, leaving the contract's own
    // `enoughTimeSpent` fallback (which allows omitting the tracker signature after the 3-day
    // window) unreachable. An EMPTY Coll[Byte] is the correct encoding for "no signature": the
    // contract branches on `trackerSigBytes.size > 0`. The Scala reference implementation and the
    // CLI both send var 6 unconditionally for the same reason.
    context_extension.insert(
        "6".to_string(),
        if payload.emergency {
            serialize_coll_bytes(&[])
        } else {
            serialize_coll_bytes(&tracker_signature)
        },
    );
    if let Some(ref lp) = reserve_lookup_proof {
        context_extension.insert("7".to_string(), serialize_coll_bytes(lp));
    }
    context_extension.insert("8".to_string(), serialize_coll_bytes(&tracker_lookup_proof));

    // Output ergoTrees.
    // The reserve output must carry exactly the script of the box being spent
    // (the contract requires selfOut.propositionBytes == SELF.propositionBytes).
    // Take it from the fetched on-chain box rather than a configured/hardcoded
    // constant, so a contract change can never leave this builder stale (B5).
    let reserve_ergo_tree = reserve_box.ergo_tree.clone();
    let reserve_contract_p2s = if is_token_reserve {
        state.config.basis_token_reserve_contract_p2s()
    } else {
        state.config.basis_reserve_contract_p2s()
    };
    match address_to_ergo_tree(reserve_contract_p2s) {
        Ok(configured_tree) if configured_tree != reserve_ergo_tree => {
            tracing::warn!(
                "reserve box {} script differs from the configured contract P2S {}; \
                 building with the box's own script",
                reserve_box_id,
                reserve_contract_p2s
            );
        }
        Ok(_) => {}
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("reserve contract P2S: {e}"),
            )
        }
    }
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

    // Build reserve/recipient outputs and change amount. Token-backed reserves keep the
    // original ERG value in the reserve and move reserve tokens to the recipient.
    let (
        reserve_output_value,
        reserve_output_assets,
        recipient_output_value,
        recipient_output_assets,
        change_amount,
    ) = if is_token_reserve {
        let token_amount_in = match reserve_box
            .assets
            .iter()
            .find(|a| a.token_id == reserve_token_id)
            .map(|a| a.amount)
        {
            Some(a) => a,
            None => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "reserve box missing configured reserve token",
                )
            }
        };
        let token_amount_out = token_amount_in.saturating_sub(payload.amount);
        if token_amount_out == 0 {
            return api_err(
                StatusCode::BAD_REQUEST,
                "reserve token amount would be zero after redemption",
            );
        }
        let reserve_assets = json!([
            { "tokenId": reserve_nft_id, "amount": 1 },
            { "tokenId": reserve_token_id, "amount": token_amount_out }
        ]);
        let recipient_assets = json!([{ "tokenId": reserve_token_id, "amount": payload.amount }]);
        let change = fee_total
            .saturating_sub(fee)
            .saturating_sub(TOKEN_RECIPIENT_MIN_VALUE_NANOERG);
        (
            reserve_box.value,
            reserve_assets,
            TOKEN_RECIPIENT_MIN_VALUE_NANOERG,
            recipient_assets,
            change,
        )
    } else {
        let reserve_output_value = reserve_box.value.saturating_sub(payload.amount);
        if reserve_output_value == 0 {
            return api_err(
                StatusCode::BAD_REQUEST,
                "reserve output value would be zero after redemption",
            );
        }
        let reserve_assets = json!([{ "tokenId": reserve_nft_id, "amount": 1 }]);
        (
            reserve_output_value,
            reserve_assets,
            payload.amount,
            json!([]),
            fee_total.saturating_sub(fee),
        )
    };

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
            "assets": reserve_output_assets,
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
            "assets": recipient_output_assets,
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
    let reserve_box_binary = match chain_node.box_binary(&reserve_box_id).await {
        Ok(b) => b,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("reserve bin: {e}"),
            )
        }
    };
    let tracker_box_binary = match chain_node.box_binary(&tracker_box_id).await {
        Ok(b) => b,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("tracker bin: {e}"),
            )
        }
    };
    let mut fee_binaries = Vec::with_capacity(fee_boxes.len());
    for fb in &fee_boxes {
        match chain_node.box_binary(&fb.box_id).await {
            Ok(b) => fee_binaries.push(b),
            Err(e) => {
                return api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("fee bin {}: {e}", fb.box_id),
                )
            }
        }
    }

    let unsigned: UnsignedTransaction = match serde_json::from_value(unsigned_tx.clone()) {
        Ok(u) => u,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("parse unsigned tx: {e}"),
            )
        }
    };
    let built_tx_id = String::from(unsigned.id());

    // The tracker signs nothing in the transaction: the reserve input is a contract
    // spend and the fee inputs belong to the redeemer, who signs every input locally.
    // The client still needs the serialized boxes and headers to derive the signing
    // context without its own node connection.
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
    // Parse once to fail loudly on malformed node data rather than returning
    // unsignable material to the client.
    parse_box(&reserve_box_binary, "reserve box")?;
    parse_box(&tracker_box_binary, "tracker box")?;
    for (i, fb) in fee_binaries.iter().enumerate() {
        parse_box(fb, &format!("fee box {i}"))?;
    }

    // State context headers (the client derives its PreHeader/ErgoStateContext from these).
    let headers = match chain_node.last_headers().await {
        Ok(h) if h.len() >= 10 => h,
        Ok(h) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("need >=10 headers, got {}", h.len()),
            )
        }
        Err(e) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("headers: {e}")),
    };
    let headers_array: [Header; 10] = headers[..10].to_vec().try_into().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("headers array".to_string())),
        )
    })?;

    let mut input_box_binaries = vec![reserve_box_binary];
    input_box_binaries.extend(fee_binaries);

    // N17: only a build that survived every validation step above consumes rate-limit
    // quota — junk builds (bad keys, unknown note, bad signature) must not be able to
    // lock a victim pair out of the per-pair budget.
    if let Err(msg) = state.redemption_build_rate.check_and_record(
        &hex::encode(issuer_pk),
        &hex::encode(recipient_pk),
        state.config.server.redemption_build_max_per_pair_per_hour,
        state.config.server.redemption_build_max_per_issuer_per_hour,
    ) {
        return api_err(StatusCode::TOO_MANY_REQUESTS, msg);
    }

    let response = RedemptionBuildResponse {
        unsigned_tx,
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
    };

    // SECURITY (PR #16 B6): bind the future /redemption/submit to this exact build. The submit
    // endpoint only accepts a signed transaction whose echoed `unsigned_tx` hashes to a live
    // entry recorded here, and it advances tracker state from the STORED values below — never
    // from caller-supplied amounts. The entry is single-use and expires after
    // PENDING_REDEMPTION_TTL.
    let fingerprint = match unsigned_tx_fingerprint(&response.unsigned_tx) {
        Ok(fp) => fp,
        Err(e) => {
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to fingerprint unsigned transaction: {e}"),
            )
        }
    };
    if let Err(RecordError::IssuerFull) = state.pending_redemptions.record(
        fingerprint,
        PendingRedemption::new(
            built_tx_id,
            response.reserve_box_id.clone(),
            payload.issuer_pubkey.clone(),
            payload.recipient_pubkey.clone(),
            payload.amount,
            new_already_redeemed,
            new_reserve_digest.clone(),
            denomination,
        ),
    ) {
        return api_err(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many outstanding redemption builds for this issuer; submit or let some \
             expire before building more.",
        );
    }

    Ok(response)
}

/// Broadcast a fully-signed redemption transaction to the Ergo node.
///
/// SECURITY (PR #16 B6): the submit is bound to server-side state recorded by
/// `/redemption/build`. The echoed `unsigned_tx` must hash to a live, single-use pending
/// entry, and the signed transaction must have the built transaction's id (so it has the same
/// inputs, context extensions and outputs). After a successful broadcast the tracker's local state is synced
/// from the STORED entry (`redeemed_amount`, `new_already_redeemed`, issuer/recipient) — the
/// caller-supplied amount fields on the wire are ignored (kept only for compatibility).
#[axum::debug_handler]
pub async fn submit_redemption(
    State(state): State<AppState>,
    Json(payload): Json<RedemptionSubmitRequest>,
) -> (StatusCode, Json<ApiResponse<RedemptionSubmitResponse>>) {
    // (a) The echoed unsigned_tx must match a pending entry from /redemption/build.
    let fingerprint = match unsigned_tx_fingerprint(&payload.unsigned_tx) {
        Ok(fp) => fp,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(format!(
                    "failed to fingerprint unsigned_tx: {e}"
                ))),
            )
        }
    };

    // (b) + (d): validate the signed transaction against the stored entry and consume it
    // (single-use — on any failure the client must call /redemption/build again).
    let pending = match state
        .pending_redemptions
        .validate_and_consume(&fingerprint, &payload.signed_tx)
    {
        Ok(p) => p,
        Err((status, msg)) => return (status, Json(error_response(msg))),
    };

    // (c) Everything that touches tracker state comes from the STORED entry.
    let issuer_pubkey: basis_store::PubKey = match hex::decode(&pending.issuer_pubkey)
        .ok()
        .and_then(|b| b.try_into().ok())
    {
        Some(pk) => pk,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    "pending redemption entry has an invalid issuer_pubkey".to_string(),
                )),
            )
        }
    };
    let recipient_pubkey: basis_store::PubKey = match hex::decode(&pending.recipient_pubkey)
        .ok()
        .and_then(|b| b.try_into().ok())
    {
        Some(pk) => pk,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(
                    "pending redemption entry has an invalid recipient_pubkey".to_string(),
                )),
            )
        }
    };

    let node = NodeClient::wallet(&state);
    let tx_id = match node.broadcast(&payload.signed_tx).await {
        Ok(tx_id) => tx_id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response(format!("broadcast failed: {e}"))),
            )
        }
    };

    // Sync local note + reserve tree state with the broadcast transaction. The reserve
    // tree value must be the cumulative amount proven on-chain (from the build), which
    // can differ from the note's cumulative redeemed amount when reserves were created
    // fresh or state was repaired.
    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .tx
        .send(TrackerCommand::CompleteRedemption {
            issuer_pubkey,
            recipient_pubkey,
            denomination: pending.denomination,
            redeemed_amount: pending.redeemed_amount,
            new_already_redeemed: Some(pending.new_already_redeemed),
            response_tx: tx,
        })
        .await
        .is_err()
    {
        tracing::error!(tx_id, "tracker thread unavailable; local state not synced");
    } else {
        match rx.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::error!(tx_id, ?e, "local redemption state sync failed"),
            Err(_) => tracing::error!(tx_id, "tracker dropped state sync response"),
        }
    }

    (
        StatusCode::OK,
        Json(success_response(RedemptionSubmitResponse { tx_id })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ----- pending redemption registry (PR #16 B6) -----

    const RESERVE_BOX: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const FUNDING_BOX: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    /// P2PK ErgoTree of the secp256k1 generator, a valid stand-in for any recipient/attacker.
    const P2PK_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn output(ergo_tree: &str, value: u64, r5_digest: Option<&[u8]>) -> Value {
        let registers = match r5_digest {
            // Registers must be densely packed, so R4 (owner key) comes along with R5.
            Some(d) => json!({
                "R4": format!("07{}", &P2PK_TREE[6..]),
                "R5": format!("64{}032000", hex::encode(d)),
            }),
            None => json!({}),
        };
        json!({
            "value": value,
            "ergoTree": ergo_tree,
            "assets": [],
            "additionalRegisters": registers,
            "creationHeight": 1_000
        })
    }

    /// An unsigned transaction (as `/redemption/build` returns it) and the same transaction
    /// with empty proofs, standing in for the client's signed copy. ergo-lib computes both ids
    /// from the transaction body, exactly as the node does.
    fn tx_pair(inputs: &[&str], outputs: Vec<Value>) -> (Value, Value) {
        tx_pair_with_extension(inputs, outputs, json!({}))
    }

    fn tx_pair_with_extension(
        inputs: &[&str],
        outputs: Vec<Value>,
        extension: Value,
    ) -> (Value, Value) {
        let unsigned_json = json!({
            "inputs": inputs
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let extension = if i == 0 { extension.clone() } else { json!({}) };
                    json!({ "boxId": id, "extension": extension })
                })
                .collect::<Vec<_>>(),
            "dataInputs": [],
            "outputs": outputs,
        });
        let unsigned: UnsignedTransaction = serde_json::from_value(unsigned_json.clone()).unwrap();
        let proofs = vec![
            ergo_lib::ergotree_interpreter::sigma_protocol::prover::ProofBytes::Empty;
            inputs.len()
        ];
        let signed = Transaction::from_unsigned_tx(unsigned, proofs).unwrap();
        (unsigned_json, serde_json::to_value(&signed).unwrap())
    }

    /// The honest built redemption: spends the reserve, reserve successor first.
    fn built_redemption() -> (Value, Value) {
        tx_pair(
            &[RESERVE_BOX, FUNDING_BOX],
            vec![
                output(P2PK_TREE, 9_000_000_000, Some(&[0x11; 33])),
                output(P2PK_TREE, 1_000_000_000, None),
            ],
        )
    }

    fn pending_entry(unsigned_tx: &Value) -> PendingRedemption {
        PendingRedemption {
            unsigned_tx_id: unsigned_tx_id(unsigned_tx).unwrap(),
            reserve_box_id: RESERVE_BOX.to_string(),
            issuer_pubkey: "aa".repeat(33),
            recipient_pubkey: "bb".repeat(33),
            redeemed_amount: 1_000,
            new_already_redeemed: 2_000,
            reserve_r5_digest: vec![0x11; 33],
            denomination: basis_store::DenomKey::Erg,
            issued_at: Instant::now(),
        }
    }

    fn recorded(registry: &PendingRedemptionRegistry) -> (UnsignedTxFingerprint, Value) {
        let (unsigned, signed) = built_redemption();
        let fp = unsigned_tx_fingerprint(&unsigned).unwrap();
        assert!(registry.record(fp, pending_entry(&unsigned)).is_ok());
        (fp, signed)
    }

    #[test]
    fn pending_entry_happy_path_returns_stored_state() {
        let registry = PendingRedemptionRegistry::new();
        let (fp, signed) = recorded(&registry);

        let entry = registry.validate_and_consume(&fp, &signed).unwrap();
        assert_eq!(entry.reserve_box_id, RESERVE_BOX);
        assert_eq!(entry.redeemed_amount, 1_000);
        assert_eq!(entry.new_already_redeemed, 2_000);
    }

    #[test]
    fn pending_entry_unknown_fingerprint_is_conflict() {
        let registry = PendingRedemptionRegistry::new();
        let fp = [7u8; 32];
        let err = registry
            .validate_and_consume(&fp, &json!({ "inputs": [] }))
            .unwrap_err();
        assert_eq!(err.0, StatusCode::CONFLICT);
    }

    #[test]
    fn pending_entry_is_single_use() {
        let registry = PendingRedemptionRegistry::new();
        let (fp, signed) = recorded(&registry);

        assert!(registry.validate_and_consume(&fp, &signed).is_ok());
        let err = registry.validate_and_consume(&fp, &signed).unwrap_err();
        assert_eq!(
            err.0,
            StatusCode::CONFLICT,
            "a replayed fingerprint must be refused"
        );
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn pending_entry_rejects_signed_tx_spending_a_different_box() {
        let registry = PendingRedemptionRegistry::new();
        let (fp, _) = recorded(&registry);

        let (_, signed) = tx_pair(
            &["3333333333333333333333333333333333333333333333333333333333333333"],
            vec![output(P2PK_TREE, 9_000_000_000, Some(&[0x11; 33]))],
        );
        let err = registry.validate_and_consume(&fp, &signed).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn pending_entry_rejects_signed_tx_with_wrong_r5_digest() {
        let registry = PendingRedemptionRegistry::new();
        let (fp, _) = recorded(&registry);

        let (_, signed) = tx_pair(
            &[RESERVE_BOX, FUNDING_BOX],
            vec![
                output(P2PK_TREE, 9_000_000_000, Some(&[0x22; 33])),
                output(P2PK_TREE, 1_000_000_000, None),
            ],
        );
        let err = registry.validate_and_consume(&fp, &signed).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    /// Follow-up review N1: a top-up (action 1, no signature needed) that spends the reserve and
    /// puts a decoy box carrying the build's R5 digest at `outputs[0]` passed the old "spends
    /// the reserve + outputs[0].R5" check, so the tracker applied a redemption that never
    /// happened and its reserve tree diverged from the chain.
    #[test]
    fn pending_entry_rejects_unrelated_tx_with_decoy_r5_output() {
        let registry = PendingRedemptionRegistry::new();
        let (fp, _) = recorded(&registry);

        let (_, top_up) = tx_pair(
            &[RESERVE_BOX, FUNDING_BOX],
            vec![
                // Attacker-owned decoy carrying the expected digest.
                output(P2PK_TREE, 1_000_000, Some(&[0x11; 33])),
                // Real reserve successor: R5 unchanged, +0.1 ERG.
                output(P2PK_TREE, 10_100_000_000, Some(&[0xff; 33])),
            ],
        );
        let err = registry.validate_and_consume(&fp, &top_up).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn pending_entry_rejects_forged_tx_id() {
        let registry = PendingRedemptionRegistry::new();
        let (fp, signed) = recorded(&registry);

        // Same claimed id, different body: ergo-lib recomputes the id and refuses it.
        let (_, mut other) = tx_pair(
            &[RESERVE_BOX, FUNDING_BOX],
            vec![
                output(P2PK_TREE, 1_000_000, Some(&[0x11; 33])),
                output(P2PK_TREE, 10_100_000_000, None),
            ],
        );
        other["id"] = signed["id"].clone();
        let err = registry.validate_and_consume(&fp, &other).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    /// Follow-up review N1: the tx id must cover the reserve input's context extension as well
    /// as its boxes. Otherwise a caller could swap the build's redemption extension (action #0,
    /// amount, issuer signature, AVL proofs) for another one and still pass the id check.
    #[test]
    fn pending_entry_binds_reserve_input_context_extension() {
        let (unsigned, signed) = tx_pair_with_extension(
            &[RESERVE_BOX, FUNDING_BOX],
            vec![
                output(P2PK_TREE, 9_000_000_000, Some(&[0x11; 33])),
                output(P2PK_TREE, 1_000_000_000, None),
            ],
            json!({ "0": "0200" }),
        );
        assert_eq!(
            unsigned_tx_id(&unsigned).unwrap(),
            signed["id"].as_str().unwrap(),
            "the signed copy must have the built transaction id"
        );

        // The matching signed copy is accepted.
        let registry = PendingRedemptionRegistry::new();
        let fp = unsigned_tx_fingerprint(&unsigned).unwrap();
        assert!(registry.record(fp, pending_entry(&unsigned)).is_ok());
        assert!(registry.validate_and_consume(&fp, &signed).is_ok());

        // A different extension changes the transaction id, even though the caller echoes the
        // original unsigned transaction (so the fingerprint still matches).
        let registry = PendingRedemptionRegistry::new();
        let fp = unsigned_tx_fingerprint(&unsigned).unwrap();
        assert!(registry.record(fp, pending_entry(&unsigned)).is_ok());
        let (other_unsigned, other) = tx_pair_with_extension(
            &[RESERVE_BOX, FUNDING_BOX],
            vec![
                output(P2PK_TREE, 9_000_000_000, Some(&[0x11; 33])),
                output(P2PK_TREE, 1_000_000_000, None),
            ],
            json!({ "0": "0201" }),
        );
        assert_ne!(
            unsigned_tx_id(&unsigned).unwrap(),
            unsigned_tx_id(&other_unsigned).unwrap()
        );
        let err = registry.validate_and_consume(&fp, &other).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn pending_entry_expired_is_conflict() {
        let registry = PendingRedemptionRegistry::new();
        let (unsigned, signed) = built_redemption();
        let fp = unsigned_tx_fingerprint(&unsigned).unwrap();
        let mut entry = pending_entry(&unsigned);
        entry.issued_at = Instant::now() - PENDING_REDEMPTION_TTL - Duration::from_secs(1);
        // Bypass `record` so the stale timestamp survives (record sets nothing itself, but
        // inserting directly makes the intent explicit).
        registry.lock().insert(fp, entry);

        let err = registry.validate_and_consume(&fp, &signed).unwrap_err();
        assert_eq!(
            err.0,
            StatusCode::CONFLICT,
            "an expired entry must be refused"
        );
    }

    /// N17: one issuer can hold at most MAX_PENDING_PER_ISSUER live entries, so a
    /// self-redeemer cannot fill the global registry on their own.
    #[test]
    fn pending_registry_caps_entries_per_issuer() {
        let registry = PendingRedemptionRegistry::new();
        let (unsigned, _) = built_redemption();
        let entry = pending_entry(&unsigned);
        for i in 0..MAX_PENDING_PER_ISSUER {
            let mut e = entry.clone();
            e.unsigned_tx_id = format!("tx-{i}");
            registry
                .record([i as u8; 32], e)
                .expect("entries below the per-issuer cap must be accepted");
        }
        let err = registry
            .record([0xff; 32], pending_entry(&unsigned))
            .unwrap_err();
        assert_eq!(err, RecordError::IssuerFull);

        // A different issuer is unaffected by the full issuer's bucket.
        let mut other = pending_entry(&unsigned);
        other.issuer_pubkey = "cc".repeat(33);
        assert!(registry.record([0xfe; 32], other).is_ok());
    }

    /// N17: when the global cap is reached the OLDEST live entry is evicted to make
    /// room; the build is never rejected with a global 503.
    #[test]
    fn pending_registry_evicts_oldest_when_globally_full() {
        let registry = PendingRedemptionRegistry::new();
        let (unsigned, _) = built_redemption();
        // Distinct issuers stay under the per-issuer cap; entries get increasing
        // (issuer, fingerprint) ages via direct insertion so eviction order is explicit.
        let fp_of = |i: usize| {
            let mut fp = [0u8; 32];
            fp[..8].copy_from_slice(&(i as u64).to_be_bytes());
            fp
        };
        for i in 0..MAX_PENDING_REDEMPTIONS {
            let mut e = pending_entry(&unsigned);
            e.issuer_pubkey = format!("{:066x}", i + 1);
            e.unsigned_tx_id = format!("tx-{i}");
            e.issued_at =
                Instant::now() - Duration::from_secs((MAX_PENDING_REDEMPTIONS - i) as u64);
            registry.lock().insert(fp_of(i), e);
        }
        assert_eq!(registry.len(), MAX_PENDING_REDEMPTIONS);

        let mut newcomer = pending_entry(&unsigned);
        newcomer.issuer_pubkey = "dd".repeat(33);
        newcomer.unsigned_tx_id = "newcomer".to_string();
        registry
            .record([0xaa; 32], newcomer)
            .expect("a full registry must evict the oldest entry, not reject");

        assert_eq!(registry.len(), MAX_PENDING_REDEMPTIONS);
        assert!(
            registry.lock().contains_key(&[0xaa; 32]),
            "the new entry must be recorded"
        );
        assert!(
            !registry.lock().contains_key(&fp_of(0)),
            "the oldest entry (issuer 1) must have been evicted"
        );
        assert!(
            registry.lock().contains_key(&fp_of(1)),
            "the second-oldest entry must have survived"
        );
    }

    // ----- build rate limiter (PR #16 M1, reworked for issue #15 review N17) -----

    #[test]
    fn build_rate_limiter_allows_up_to_limit_then_rejects() {
        let limiter = BuildRateLimiter::new();
        assert!(limiter
            .check_and_record("issuer", "recipient", 2, 100)
            .is_ok());
        assert!(limiter
            .check_and_record("issuer", "recipient", 2, 100)
            .is_ok());
        let err = limiter
            .check_and_record("issuer", "recipient", 2, 100)
            .unwrap_err();
        assert!(err.contains("rate limit exceeded"), "got: {err}");
        // A different pair is unaffected.
        assert!(limiter.check_and_record("issuer", "other", 2, 100).is_ok());
    }

    /// N17: rotating fresh recipient keys resets the per-pair bucket, but the per-issuer
    /// budget still binds.
    #[test]
    fn build_rate_limiter_per_issuer_cap_defeats_recipient_rotation() {
        let limiter = BuildRateLimiter::new();
        for i in 0..3 {
            assert!(
                limiter
                    .check_and_record("issuer", &format!("recipient-{i}"), 60, 3)
                    .is_ok(),
                "build {i} should fit the per-issuer budget"
            );
        }
        let err = limiter
            .check_and_record("issuer", "recipient-fresh", 60, 3)
            .unwrap_err();
        assert!(err.contains("per issuer"), "got: {err}");
        // A different issuer is unaffected.
        assert!(limiter
            .check_and_record("other-issuer", "recipient", 1, 1)
            .is_ok());
    }

    #[test]
    fn build_rate_limiter_prunes_attempts_older_than_an_hour() {
        let limiter = BuildRateLimiter::new();
        {
            let mut buckets = limiter.pair_buckets.lock().unwrap();
            buckets.insert(
                ("issuer".to_string(), "recipient".to_string()),
                vec![Instant::now() - Duration::from_secs(3601)],
            );
        }
        assert!(limiter
            .check_and_record("issuer", "recipient", 1, 100)
            .is_ok());
    }

    /// N17: buckets whose sliding window has emptied are removed, so the maps stay
    /// bounded by recent activity rather than every pair ever seen.
    #[test]
    fn build_rate_limiter_evicts_empty_buckets() {
        let limiter = BuildRateLimiter::new();
        {
            let mut pairs = limiter.pair_buckets.lock().unwrap();
            pairs.insert(
                ("stale-issuer".to_string(), "stale-recipient".to_string()),
                vec![Instant::now() - Duration::from_secs(7200)],
            );
        }
        {
            let mut issuers = limiter.issuer_buckets.lock().unwrap();
            issuers.insert(
                "stale-issuer".to_string(),
                vec![Instant::now() - Duration::from_secs(7200)],
            );
        }
        // Any check sweeps both maps.
        assert!(limiter
            .check_and_record("issuer", "recipient", 1, 100)
            .is_ok());
        assert!(
            !limiter
                .pair_buckets
                .lock()
                .unwrap()
                .contains_key(&("stale-issuer".to_string(), "stale-recipient".to_string())),
            "an expired pair bucket must be evicted"
        );
        assert!(
            !limiter
                .issuer_buckets
                .lock()
                .unwrap()
                .contains_key("stale-issuer"),
            "an expired issuer bucket must be evicted"
        );
    }

    // ------------------------------------------------------------------
    // Tracker signature (context var #6) denomination binding
    //
    // The token reserve contract verifies var #6 over
    // blake2b256(owner || receiver || tokenId) || …; a signature over the ERG
    // debtKey makes every token redemption unverifiable on-chain.
    // ------------------------------------------------------------------

    fn sign_and_decode(
        denomination: &basis_store::DenomKey,
    ) -> (
        [u8; 65],
        basis_store::PubKey,
        basis_store::PubKey,
        basis_store::PubKey,
    ) {
        let (tracker_secret, tracker_pubkey) = basis_store::schnorr::generate_keypair();
        let (_, issuer_pk) = basis_store::schnorr::generate_keypair();
        let (_, recipient_pk) = basis_store::schnorr::generate_keypair();
        let sig = sign_tracker_redemption(
            &tracker_secret,
            &tracker_pubkey,
            &issuer_pk,
            &recipient_pk,
            denomination,
            5_000_000,
            1_743_379_200_000,
            &[0x42; 32],
        )
        .expect("tracker signature signs");
        assert_eq!(sig.len(), 65);
        let mut array = [0u8; 65];
        array.copy_from_slice(&sig);
        (array, tracker_pubkey, issuer_pk, recipient_pk)
    }

    #[test]
    fn tracker_redemption_signature_binds_token_denomination() {
        let token_id = [0xab_u8; 32];
        let denom = basis_store::DenomKey::Token(token_id);
        let (sig, tracker_pubkey, issuer_pk, recipient_pk) = sign_and_decode(&denom);

        // Verifies against the TOKEN debtKey message — what the token contract evaluates.
        let token_message = basis_store::schnorr::signing_message(
            &issuer_pk,
            &recipient_pk,
            &denom,
            5_000_000,
            1_743_379_200_000,
            &[0x42; 32],
        );
        basis_store::schnorr::schnorr_verify(&sig, &token_message, &tracker_pubkey)
            .expect("token tracker signature must verify over the token debtKey message");

        // Regression pin: the ERG debtKey message must NOT verify. Signing the ERG
        // key was the bug — such a signature is rejected by the reserve contract.
        let erg_message = basis_store::schnorr::signing_message(
            &issuer_pk,
            &recipient_pk,
            &basis_store::DenomKey::Erg,
            5_000_000,
            1_743_379_200_000,
            &[0x42; 32],
        );
        assert!(
            basis_store::schnorr::schnorr_verify(&sig, &erg_message, &tracker_pubkey).is_err(),
            "token-note tracker signature must not verify over the ERG debtKey message"
        );
    }

    #[test]
    fn tracker_redemption_signature_erg_note_unchanged() {
        let (sig, tracker_pubkey, issuer_pk, recipient_pk) =
            sign_and_decode(&basis_store::DenomKey::Erg);

        let erg_message = basis_store::schnorr::signing_message(
            &issuer_pk,
            &recipient_pk,
            &basis_store::DenomKey::Erg,
            5_000_000,
            1_743_379_200_000,
            &[0x42; 32],
        );
        basis_store::schnorr::schnorr_verify(&sig, &erg_message, &tracker_pubkey)
            .expect("ERG-note tracker signature still verifies over the ERG debtKey message");

        // Sanity: a different denomination's message must not verify for the ERG note.
        let token_message = basis_store::schnorr::signing_message(
            &issuer_pk,
            &recipient_pk,
            &basis_store::DenomKey::Token([0xab_u8; 32]),
            5_000_000,
            1_743_379_200_000,
            &[0x42; 32],
        );
        assert!(
            basis_store::schnorr::schnorr_verify(&sig, &token_message, &tracker_pubkey).is_err()
        );
    }
}
