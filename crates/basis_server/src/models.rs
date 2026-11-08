use basis_store::IouNote;
use serde::{Deserialize, Serialize};

/// Confirmation summary attached to note responses.
/// Mirrors the store-side `NoteConfirmation` but includes the
/// redeemable flag/amount computed for the client.
#[derive(Debug, Serialize)]
pub struct NoteConfirmationSummary {
    pub status: String,
    pub confirmed_total_debt: Option<u64>,
    pub pending_total_debt: Option<u64>,
    pub confirmed_box_id: Option<String>,
    pub confirmed_height: Option<u64>,
    pub pending_tx_id: Option<String>,
    pub redeemable: bool,
    pub redeemable_amount: u64,
}

impl NoteConfirmationSummary {
    /// Build a summary from a store confirmation and the already-redeemed amount.
    pub fn from_confirmation(
        confirmation: &basis_store::NoteConfirmation,
        already_redeemed: u64,
    ) -> Self {
        Self {
            status: status_to_string(confirmation.status),
            confirmed_total_debt: confirmation.confirmed_total_debt,
            pending_total_debt: confirmation.pending_total_debt,
            confirmed_box_id: confirmation.confirmed_box_id.clone(),
            confirmed_height: confirmation.confirmed_height,
            pending_tx_id: confirmation.pending_tx_id.clone(),
            redeemable: confirmation.is_redeemable(already_redeemed),
            redeemable_amount: confirmation.redeemable_amount(already_redeemed),
        }
    }
}

fn status_to_string(status: basis_store::NoteConfirmationStatus) -> String {
    match status {
        basis_store::NoteConfirmationStatus::LocalOnly => "local_only".to_string(),
        basis_store::NoteConfirmationStatus::Pending => "pending".to_string(),
        basis_store::NoteConfirmationStatus::Confirmed => "confirmed".to_string(),
    }
}

// Request structure for creating a new IOU note
// Using hex-encoded strings for public keys and signature
// `Debug` is implemented by hand (redacted): `create_note` debug-logs the request.
#[derive(Deserialize)]
pub struct CreateNoteRequest {
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub signature: String,
    pub issuer_pubkey: String,
    /// Tracker NFT ID the note is bound to (part of the signed message, issue #15 C3).
    /// Must match this tracker's configured `ergo.tracker_nft_id`.
    pub tracker_nft_id: String,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
    /// Bound notes only (a timestamp ending in 997 ms): the issuer's prior
    /// acknowledgment, its signature (hex, 65 bytes) over the edge's standard
    /// note message at its CURRENT total and this note's timestamp. The note
    /// applies only while that total is still the stored one. Must be absent for
    /// every other note.
    #[serde(default)]
    pub prior_signature: Option<String>,
    /// Optional and diagnostic: the current total the prior acknowledgment
    /// covers. When present it must equal the stored total (409 `stale_prior`).
    /// Only valid together with `prior_signature`.
    #[serde(default)]
    pub prior_total: Option<u64>,
}

/// Redacted (see [`crate::redact`]): signatures appear only as fingerprints.
impl std::fmt::Debug for CreateNoteRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::redact::{Denom, HexField, SigHex};
        f.debug_struct("CreateNoteRequest")
            .field("recipient_pubkey", &HexField(&self.recipient_pubkey, 33))
            .field("amount", &self.amount)
            .field("timestamp", &self.timestamp)
            .field("signature", &SigHex(&self.signature))
            .field("issuer_pubkey", &HexField(&self.issuer_pubkey, 33))
            .field("tracker_nft_id", &HexField(&self.tracker_nft_id, 32))
            .field("denomination", &Denom(&self.denomination))
            .field("prior_signature", &self.prior_signature.as_deref().map(SigHex))
            .field("prior_total", &self.prior_total)
            .finish()
    }
}

/// Cross-denomination debt exchange request (off-chain novation / triage).
///
/// Moves `from_amount` of cumulative debt out of the (issuer, recipient,
/// from_denomination) edge and `to_amount` into the (issuer, recipient,
/// to_denomination) edge (specs/ergo/multi_collateral_reserves.md §10). All four
/// signatures are over the respective edge's standard 80-byte note message at the
/// NEW cumulative total and `timestamp`; the recipient's signatures are the
/// creditor consent required by the S3 rule.
#[derive(Debug, Deserialize)]
pub struct ExchangeNotesRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub from_denomination: String,
    pub from_amount: u64,
    pub to_denomination: String,
    pub to_amount: u64,
    /// New timestamp for both edges; must be newer than both edges' current ones.
    pub timestamp: u64,
    /// Issuer signature over the from-edge's new note message (hex, 65 bytes)
    pub issuer_from_signature: String,
    /// Recipient signature over the from-edge's new note message (creditor consent)
    pub recipient_from_signature: String,
    /// Issuer signature over the to-edge's new note message (hex, 65 bytes)
    pub issuer_to_signature: String,
    /// Recipient signature over the to-edge's new note message (creditor acceptance)
    pub recipient_to_signature: String,
}

#[derive(Debug, Serialize)]
pub struct ExchangeNotesResponse {
    pub exchanged: bool,
    pub from_denomination: String,
    pub to_denomination: String,
    pub from_new_total: u64,
    pub to_new_total: u64,
    pub timestamp: u64,
    /// Status of the to-edge increase: always `"pending"` on success — only
    /// the from-edge decrease is applied immediately; the to-edge increase is
    /// staged until the decrease is confirmed in a tracker box and the
    /// from-denomination reserve is verified unchanged (issue #17). The
    /// to-edge note appears once applied; a cancelled exchange is visible via
    /// `GET /exchanges/pending`.
    pub to_edge_status: String,
}

/// Debt transfer request (triad novation, specs/spec.md "Debt Transfer"),
/// version 3.
///
/// Moves `amount` of the issuer's cumulative debt from the edge
/// (issuer -> from_recipient) to the edge (issuer -> to_recipient) in one
/// denomination. Seven signatures (hex, 65 bytes each), all over the standard
/// 80-byte note message at `timestamp` (see `basis_store::TransferSignatures`):
/// the issuer re-signs both edges at their new totals; the old creditor
/// acknowledges its edge's current total and signs both new totals (the consent
/// the S3 rule requires for lowering its edge, bound to this destination); the
/// new creditor acknowledges its edge's current total (0 for a new edge) and
/// signs its new total (its acceptance). The prior acknowledgments make the
/// amount exact: the transfer applies only while both edges are still at the
/// acknowledged totals. Pure-credit trackers only: a tracker with on-chain
/// state refuses. Unknown fields are refused, so a field this tracker does not
/// know is never silently dropped. `Debug` is redacted (signatures appear only
/// as fingerprints, see [`crate::redact`]).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferNotesRequest {
    pub issuer_pubkey: String,
    pub from_recipient_pubkey: String,
    pub to_recipient_pubkey: String,
    /// Required, and a denomination this tracker serves ("erg" or
    /// "token:<64-hex>"); there is no default. A missing value reads as empty
    /// and is refused like one (400 `invalid denomination`).
    #[serde(default)]
    pub denomination: String,
    pub amount: u64,
    /// New timestamp for both edges; must end in 999 ms, be newer than both
    /// edges' current ones and be at most an hour old.
    pub timestamp: u64,
    /// Issuer signature over the from-edge's new note message (hex, 65 bytes)
    pub issuer_from_signature: String,
    /// Old creditor's prior acknowledgment: its signature over the from-edge's
    /// note message at the edge's CURRENT total (hex, 65 bytes)
    pub from_recipient_prior_signature: String,
    /// Old creditor's signature over the from-edge's new note message (consent)
    pub from_recipient_signature: String,
    /// Old creditor's signature over the to-edge's new note message: binds its
    /// consent to this destination and amount
    pub from_recipient_to_signature: String,
    /// Issuer signature over the to-edge's new note message (hex, 65 bytes)
    pub issuer_to_signature: String,
    /// New creditor's prior acknowledgment: its signature over the to-edge's
    /// note message at the edge's CURRENT total, 0 for a new edge (hex, 65 bytes)
    pub to_recipient_prior_signature: String,
    /// New creditor's signature over the to-edge's new note message (acceptance)
    pub to_recipient_signature: String,
    /// Optional and diagnostic: the from-edge total the old creditor
    /// acknowledged. When present it must equal the stored total (409 `stale_prior`).
    #[serde(default)]
    pub from_prior_total: Option<u64>,
    /// Optional and diagnostic: the to-edge total the new creditor acknowledged
    /// (0 for a new edge). When present it must equal the stored total (409 `stale_prior`).
    #[serde(default)]
    pub to_prior_total: Option<u64>,
}

/// Redacted (see [`crate::redact`]): signatures appear only as fingerprints.
impl std::fmt::Debug for TransferNotesRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::redact::{Denom, HexField, SigHex};
        f.debug_struct("TransferNotesRequest")
            .field("issuer_pubkey", &HexField(&self.issuer_pubkey, 33))
            .field("from_recipient_pubkey", &HexField(&self.from_recipient_pubkey, 33))
            .field("to_recipient_pubkey", &HexField(&self.to_recipient_pubkey, 33))
            .field("denomination", &Denom(&self.denomination))
            .field("amount", &self.amount)
            .field("timestamp", &self.timestamp)
            .field("issuer_from_signature", &SigHex(&self.issuer_from_signature))
            .field(
                "from_recipient_prior_signature",
                &SigHex(&self.from_recipient_prior_signature),
            )
            .field("from_recipient_signature", &SigHex(&self.from_recipient_signature))
            .field(
                "from_recipient_to_signature",
                &SigHex(&self.from_recipient_to_signature),
            )
            .field("issuer_to_signature", &SigHex(&self.issuer_to_signature))
            .field(
                "to_recipient_prior_signature",
                &SigHex(&self.to_recipient_prior_signature),
            )
            .field("to_recipient_signature", &SigHex(&self.to_recipient_signature))
            .field("from_prior_total", &self.from_prior_total)
            .field("to_prior_total", &self.to_prior_total)
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct TransferNotesResponse {
    pub transferred: bool,
    pub denomination: String,
    pub from_new_total: u64,
    pub to_new_total: u64,
    pub timestamp: u64,
    /// The from-edge's total before the transfer (the old creditor's acknowledged prior).
    pub from_prior_total: u64,
    /// The to-edge's total before the transfer, 0 for a new edge (the new creditor's acknowledged prior).
    pub to_prior_total: u64,
}

/// One edge of a multilateral set-off request (set-off version 3). Unknown
/// fields are refused. `Debug` is redacted (signatures appear only as
/// fingerprints, see [`crate::redact`]).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetOffEdgeRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// How much this edge's cumulative total falls.
    pub decrease: u64,
    /// Optional: this edge's own timestamp ts_e (per-edge timestamps), which
    /// must end in 998 ms, be at most an hour old and be newer than the edge's
    /// current timestamp. Absent (or null), the request's `timestamp` applies.
    #[serde(default)]
    pub timestamp: Option<u64>,
    /// Issuer's signature over the edge's note message at its new total and
    /// the edge's timestamp (hex, 65 bytes)
    pub issuer_signature: String,
    /// Creditor's signature over the same message (consent to lowering)
    pub recipient_signature: String,
}

/// Redacted (see [`crate::redact`]): signatures appear only as fingerprints.
impl std::fmt::Debug for SetOffEdgeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::redact::{HexField, SigHex};
        f.debug_struct("SetOffEdgeRequest")
            .field("issuer_pubkey", &HexField(&self.issuer_pubkey, 33))
            .field("recipient_pubkey", &HexField(&self.recipient_pubkey, 33))
            .field("decrease", &self.decrease)
            .field("timestamp", &self.timestamp)
            .field("issuer_signature", &SigHex(&self.issuer_signature))
            .field("recipient_signature", &SigHex(&self.recipient_signature))
            .finish()
    }
}

/// Multilateral set-off (obligation clearing) request, version 3: lowers every
/// listed edge by its decrease, each at its own timestamp (the edge's
/// `timestamp`, else this request's `timestamp`), all ending in 998 ms. For
/// every key, decreases as creditor must equal decreases as issuer
/// (net-neutral). Unknown fields are refused, so a field this tracker does not
/// know is never silently dropped. `Debug` is redacted (the edges'
/// signatures appear only as fingerprints).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetOffRequest {
    #[serde(default)]
    pub denomination: String,
    /// The timestamp of every edge that does not carry its own.
    pub timestamp: u64,
    pub edges: Vec<SetOffEdgeRequest>,
}

/// Redacted: each edge as [`SetOffEdgeRequest`]'s redacted `Debug`.
impl std::fmt::Debug for SetOffRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetOffRequest")
            .field("denomination", &crate::redact::Denom(&self.denomination))
            .field("timestamp", &self.timestamp)
            .field("edges", &self.edges)
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct SetOffEdgeResult {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub new_total: u64,
    /// The edge's new timestamp: its own `timestamp`, else the request's.
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct SetOffResponse {
    pub cleared: bool,
    pub denomination: String,
    pub timestamp: u64,
    pub total_cleared: u64,
    pub edges: Vec<SetOffEdgeResult>,
}

/// `GET /capabilities` body: `{"success": true, "data": {...}}`. A client
/// enables a feature only when `data` is exactly what it expects (design
/// §3.6): routes answer OPTIONS whatever they enforce, so probing them proves
/// nothing. Trackers without this route (3df7ed93, caa2e86, a00233d) answer
/// 404.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitiesResponse {
    pub success: bool,
    pub data: TrackerCapabilities,
}

/// The build identity and the rules this tracker enforces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerCapabilities {
    /// Always `"basis"`.
    pub tracker: String,
    /// The rule set: `"valun-circulating-credit/3"` ([`CAPABILITIES_BUILD`]).
    pub build: String,
    /// The git commit the binary was built from (`BASIS_GIT_COMMIT` at build
    /// time), or `"unknown"`.
    pub commit: String,
    /// `[server] pure_credit`.
    pub pure_credit: bool,
    /// The timestamp residues (`ts % 1000`) reserved for operations; ordinary
    /// notes use 000-994: `[995, 996, 997, 998, 999]`.
    pub reserved_ms: Vec<u64>,
    /// Bound notes (POST /notes with `prior_signature`).
    pub bound_notes: BoundNotesCapability,
    /// Every edge a novation changes becomes bound-only.
    pub bound_only_edges: bool,
    pub novation: NovationCapabilities,
    /// Edges whose confirmation record has a committed or pending on-chain
    /// total (0 on a pure-credit tracker).
    pub onchain_edges: u64,
    /// Why debt transfers and set-offs are refused (503 `novations_disabled`),
    /// or `null`.
    pub novations_disabled_reason: Option<String>,
}

/// Bound notes: residue 997, version 2 (tracker v3.1): a bound note older than
/// `max_age_ms` is refused (`novation_expired`), like a novation. Version 1
/// (tracker v3) had no maximum age.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundNotesCapability {
    pub ms: u64,
    pub version: u32,
    /// The maximum age of a bound note's timestamp (one hour, the novation
    /// maximum age).
    pub max_age_ms: u64,
}

/// The novation routes and their shared maximum age.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovationCapabilities {
    /// The maximum age of a novation's timestamp (one hour).
    pub max_age_ms: u64,
    pub transfer: TransferCapability,
    pub setoff: SetOffCapability,
}

/// POST /notes/transfer: residue 999, version 3, its seven signature slots in
/// request order of `basis_store::TransferSignatures`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferCapability {
    pub ms: u64,
    pub version: u32,
    pub signatures: Vec<String>,
}

/// POST /notes/setoff: residue 998, version 3, at most 64 edges, per-edge
/// timestamps, two signatures per edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetOffCapability {
    pub ms: u64,
    pub version: u32,
    pub max_edges: u64,
    pub per_edge_timestamps: bool,
    pub signatures_per_edge: Vec<String>,
}

/// The `build` of [`TrackerCapabilities`]: the Valun circulating-credit rule
/// set, tracker v3.
pub const CAPABILITIES_BUILD: &str = "valun-circulating-credit/3";

impl TrackerCapabilities {
    /// This build's capabilities, with the run-time values: the `pure_credit`
    /// setting, the number of on-chain edges and the novations-disabled reason.
    /// Every rule value comes from the store's constants, so a changed rule
    /// changes the response.
    pub fn current(
        pure_credit: bool,
        onchain_edges: u64,
        novations_disabled_reason: Option<String>,
    ) -> Self {
        let names = |slots: &[&str]| slots.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        Self {
            tracker: "basis".to_string(),
            build: CAPABILITIES_BUILD.to_string(),
            commit: option_env!("BASIS_GIT_COMMIT")
                .filter(|c| !c.is_empty())
                .unwrap_or("unknown")
                .to_string(),
            pure_credit,
            reserved_ms: vec![
                basis_store::EXCHANGE_TS_MS,
                basis_store::COMPRESS_TS_MS,
                basis_store::BOUND_NOTE_TS_MS,
                basis_store::SETOFF_TS_MS,
                basis_store::TRANSFER_TS_MS,
            ],
            bound_notes: BoundNotesCapability {
                ms: basis_store::BOUND_NOTE_TS_MS,
                version: 2,
                max_age_ms: basis_store::NOVATION_MAX_AGE_MS,
            },
            bound_only_edges: true,
            novation: NovationCapabilities {
                max_age_ms: basis_store::NOVATION_MAX_AGE_MS,
                transfer: TransferCapability {
                    ms: basis_store::TRANSFER_TS_MS,
                    version: 3,
                    signatures: names(&[
                        "issuer_from",
                        "issuer_to",
                        "from_recipient_prior",
                        "from_recipient",
                        "from_recipient_to",
                        "to_recipient_prior",
                        "to_recipient",
                    ]),
                },
                setoff: SetOffCapability {
                    ms: basis_store::SETOFF_TS_MS,
                    version: 3,
                    max_edges: basis_store::MAX_SETOFF_EDGES as u64,
                    per_edge_timestamps: true,
                    signatures_per_edge: names(&["issuer", "recipient"]),
                },
            },
            onchain_edges,
            novations_disabled_reason,
        }
    }
}

/// One pending (staged or cancelled) cross-denomination exchange, as returned
/// by `GET /exchanges/pending` (issue #17).
#[derive(Debug, Serialize)]
pub struct PendingExchangeView {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub from_denomination: String,
    /// The from-edge's collected total before the exchange decrease.
    pub from_old_total: u64,
    /// The from-edge's collected total after the decrease (applied immediately).
    pub from_new_total: u64,
    pub to_denomination: String,
    /// The to-edge increase amount.
    pub to_amount: u64,
    /// The to-edge's total once the staged increase is applied.
    pub to_new_total: u64,
    /// `"staged"` while the to-edge increase awaits the from-edge decrease
    /// being committed and the reserve being verified; `"cancelled"` when the
    /// exchange was abandoned (`cancel_reason` explains why).
    pub status: String,
    pub cancel_reason: Option<String>,
    /// Unix timestamp (millis) when the exchange was staged.
    pub created_at: u64,
}

// Response structure for API responses
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    /// Machine-readable error code (POST /notes and the novation routes), such
    /// as `reserved_timestamp` or `stale_prior`. Omitted from the JSON when
    /// absent, so every response without a code is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

// Event types for tracker events
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum EventType {
    NoteUpdated,
    ReserveCreated,
    ReserveToppedUp,
    ReserveRedeemed,
    ReserveSpent,
    Commitment,
    CollateralAlert { ratio: f64 },
}

// Unified event structure for paginated events
#[derive(Debug, Clone, Serialize)]
pub struct TrackerEvent {
    pub id: u64,
    pub event_type: EventType,
    pub timestamp: u64,
    pub issuer_pubkey: Option<String>,
    pub recipient_pubkey: Option<String>,
    pub amount: Option<u64>,
    pub reserve_box_id: Option<String>,
    pub collateral_amount: Option<u64>,
    pub redeemed_amount: Option<u64>,
    pub height: Option<u64>,
}

// Serializable version of IouNote for API responses
#[derive(Debug, Serialize)]
pub struct SerializableIouNote {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
    /// Tracker NFT ID this note is bound to (part of the signed message, issue #15 C3).
    pub tracker_nft_id: String,
    /// Denomination of the debt: "erg" or "token:<64-hex>".
    pub denomination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<NoteConfirmationSummary>,
}

// Serializable version of IouNote for API responses with age
#[derive(Debug, Serialize)]
pub struct SerializableIouNoteWithAge {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
    /// Tracker NFT ID this note is bound to (part of the signed message, issue #15 C3).
    pub tracker_nft_id: String,
    pub age_seconds: u64,
    /// Denomination of the debt: "erg" or "token:<64-hex>".
    pub denomination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<NoteConfirmationSummary>,
}

impl From<IouNote> for SerializableIouNote {
    fn from(note: IouNote) -> Self {
        Self {
            issuer_pubkey: "".to_string(), // Will be set by the API handler
            recipient_pubkey: hex::encode(note.recipient_pubkey),
            amount_collected: note.amount_collected,
            amount_redeemed: note.amount_redeemed,
            timestamp: note.timestamp,
            signature: hex::encode(note.signature),
            tracker_nft_id: hex::encode(note.tracker_nft_id),
            denomination: note.denomination.denomination_id(),
            confirmation: None,
        }
    }
}

// Key status response
#[derive(Debug, Serialize)]
pub struct KeyStatusResponse {
    pub issuer_pubkey: String,
    /// Unit-free aggregate: total number of notes across all denominations.
    pub note_count: usize,
    pub last_updated: u64,
    pub has_pending_refund: bool,
    /// Per-denomination breakdown; debt and collateral are never summed across
    /// denominations (specs/ergo/multi_collateral_reserves.md §11).
    pub denominations: Vec<KeyStatusDenomination>,
}

/// Per-denomination debt/collateral status for an issuer.
#[derive(Debug, Serialize)]
pub struct KeyStatusDenomination {
    /// "erg" or "token:<64-hex>"
    pub denomination: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_debt: u64,
    pub collateral: u64,
    pub collateralization_ratio: f64,
    pub note_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve_box_id: Option<String>,
    pub has_pending_refund: bool,
}

// Redemption request
// `Debug` is implemented by hand (redacted): `initiate_redemption` debug-logs the request.
#[derive(Deserialize)]
pub struct RedeemRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    /// Reserve box ID (optional - will be looked up if not provided)
    #[serde(default)]
    pub reserve_box_id: String,
    /// Recipient address for redemption output (optional - derived from recipient_pubkey if not provided)
    #[serde(default)]
    pub recipient_address: String,
    /// Issuer's Schnorr signature (65 bytes, hex encoded = 130 chars)
    pub issuer_signature: String,
    /// Whether this is an emergency redemption
    #[serde(default)]
    pub emergency: bool,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
}

/// Redacted (see [`crate::redact`]): the issuer's signature appears only as its
/// fingerprint, and free-text fields only up to 72 characters (else by length).
impl std::fmt::Debug for RedeemRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::redact::{Denom, HexField, SigHex, Text};
        f.debug_struct("RedeemRequest")
            .field("issuer_pubkey", &HexField(&self.issuer_pubkey, 33))
            .field("recipient_pubkey", &HexField(&self.recipient_pubkey, 33))
            .field("amount", &self.amount)
            .field("timestamp", &self.timestamp)
            .field("reserve_box_id", &Text(&self.reserve_box_id))
            .field("recipient_address", &Text(&self.recipient_address))
            .field("issuer_signature", &SigHex(&self.issuer_signature))
            .field("emergency", &self.emergency)
            .field("denomination", &Denom(&self.denomination))
            .finish()
    }
}

// Redemption completion request
#[derive(Debug, Deserialize)]
pub struct CompleteRedemptionRequest {
    pub redemption_id: String,
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
    pub redeemed_amount: u64,
    /// Optional explicit cumulative reserve-tree value (from the on-chain build).
    /// When omitted, the note's cumulative redeemed amount is used.
    #[serde(default)]
    pub new_already_redeemed: Option<u64>,
}

// Redemption response
#[derive(Debug, Serialize)]
pub struct RedeemResponse {
    pub redemption_id: String,
    pub amount: u64,
    pub timestamp: u64,
    pub proof_available: bool,
    pub transaction_pending: bool,
    /// Prepared transaction data that can be submitted to Ergo node
    /// Contains all necessary fields for wallet payment API
    pub transaction_data: Option<TransactionData>,
    /// Raw Ergo transaction JSON (hex encoded) that can be signed and submitted
    pub transaction_bytes: Option<String>,
}

// Transaction data that can be submitted to Ergo node
#[derive(Debug, Serialize)]
pub struct TransactionData {
    /// Target address for the transaction
    pub address: String,
    /// Value in nanoERG
    pub value: u64,
    /// Register values
    pub registers: std::collections::HashMap<String, String>,
    /// Assets to include in transaction
    pub assets: Vec<TokenData>,
    /// Transaction fee
    pub fee: u64,
}

// Token/Asset data for transaction
#[derive(Debug, Serialize)]
pub struct TokenData {
    pub token_id: String,
    pub amount: u64,
}

// Proof response
#[derive(Debug, Serialize)]
pub struct ProofResponse {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub proof_data: String,
    pub tracker_state_digest: String,
    pub block_height: u64,
    pub timestamp: u64,
}

// Request for tracker signature
// Following specs/server/redemption_state_spec.md - POST /tracker/signature
// Request for tracker signature
// Following specs/server/redemption_state_spec.md - POST /tracker/signature
//
// The server signs only server-authoritative values (issue #15 S4): a stored note
// for (issuer, recipient) must exist, and the message covers the stored
// `amount_collected`/`timestamp`. `total_debt`/`timestamp` must match the stored
// note; a mismatch is rejected with 409 Conflict.
#[derive(Debug, Deserialize)]
pub struct TrackerSignatureRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// Must equal the stored note's total debt (rejected otherwise).
    pub total_debt: u64,
    /// Must equal the stored note's payment timestamp (rejected otherwise).
    pub timestamp: u64,
    #[serde(default)]
    pub emergency: bool,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
}

// Response for tracker signature
// Following specs/server/redemption_state_spec.md
#[derive(Debug, Serialize)]
pub struct TrackerSignatureResponse {
    pub success: bool,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub message_signed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_emergency: Option<bool>,
}

// Request for redemption preparation
#[derive(Debug, Deserialize)]
pub struct RedemptionPreparationRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// Intended redemption amount. Used only for the acceptance-policy check; the
    /// signed message always covers the stored note's cumulative debt.
    pub amount: u64,
    /// Must equal the stored note's payment timestamp (rejected with 409 otherwise).
    pub timestamp: u64,
    /// Whether this is an emergency redemption (exempt from the acceptance-policy check)
    #[serde(default)]
    pub emergency: bool,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
}

// Response for redemption preparation
#[derive(Debug, Serialize)]
pub struct RedemptionPreparationResponse {
    pub redemption_id: String,
    pub avl_proof: String,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub tracker_state_digest: String,
    pub block_height: u64,
}

// Redemption proof response - following specs/server/redemption_state_spec.md
// GET /proof/redemption endpoint response
#[derive(Debug, Serialize)]
pub struct RedemptionProofResponse {
    pub success: bool,
    pub data: RedemptionProofData,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionProofData {
    pub tracker_lookup_proof: String,
    pub reserve_lookup_proof: Option<String>,
    pub reserve_insert_proof: String,
    pub tracker_state_digest: String,
    pub reserve_state_digest: String,
    pub total_debt: u64,
    pub already_redeemed: u64,
    pub proof_valid: bool,
    pub is_first_redemption: bool,
}

// Tracker lookup proof response - for context var #8
// GET /tracker/proof endpoint response
#[derive(Debug, Serialize)]
pub struct TrackerProofResponse {
    pub success: bool,
    pub data: TrackerProofData,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TrackerProofData {
    /// Hex-encoded AVL tree key: hash(ownerKey || receiverKey)
    pub key: String,
    /// Hex-encoded value: totalDebt as 8-byte big-endian
    pub value: String,
    /// Hex-encoded AVL proof bytes
    pub proof: String,
    /// Total debt as integer
    pub total_debt: u64,
    /// Current tracker state digest (R5 register value)
    pub tracker_state_digest: String,
}

// Reserve lookup proof response - for context var #7
// GET /reserve/proof endpoint response
#[derive(Debug, Serialize)]
pub struct ReserveProofData {
    /// Hex-encoded AVL tree key: hash(ownerKey || receiverKey)
    pub key: String,
    /// Hex-encoded value: already_redeemed as 8-byte big-endian
    pub value: String,
    /// Hex-encoded AVL proof bytes (None for first redemption) - for context var #7 (lookup)
    pub proof: Option<String>,
    /// Already redeemed amount as integer
    pub already_redeemed: u64,
    /// Whether this is the first redemption (no lookup proof needed)
    pub is_first_redemption: bool,
    /// Hex-encoded AVL insert proof for context var #5 (insert operation)
    /// This proof is used to INSERT the new already_redeemed amount into the reserve tree
    pub insert_proof: String,
    /// Hex-encoded updated reserve state digest after the insert operation (R5 register value)
    pub new_reserve_state_digest: String,
}

// Redemption preparation response - updated with new fields
#[derive(Debug, Serialize)]
pub struct RedemptionPreparationResponseV2 {
    pub success: bool,
    pub data: RedemptionPreparationData,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionPreparationData {
    pub redemption_id: String,
    pub tracker_lookup_proof: String,
    pub reserve_lookup_proof: Option<String>,
    pub reserve_insert_proof: String,
    pub tracker_signature: String,
    pub reserve_signature: String,
    pub tracker_pubkey: String,
    pub tracker_state_digest: String,
    pub reserve_state_digest: String,
    pub total_debt: u64,
    pub already_redeemed: u64,
    pub is_first_redemption: bool,
    pub transaction_bytes: String,
}

// Request for creating a reserve
#[derive(Debug, Deserialize)]
pub struct CreateReserveRequest {
    pub nft_id: String,
    pub owner_pubkey: String,
    /// ERG value for the reserve box (nanoERG). Always required to cover min-box-value and fees.
    pub erg_amount: u64,
    /// Amount of the reserve token to lock in the reserve (raw token units).
    /// Required when the tracker is configured for token-backed reserves.
    #[serde(default)]
    pub token_amount: u64,
    /// Reserve token ID (hex-encoded, 32 bytes). Required when the tracker is configured
    /// for token-backed reserves; must match the configured `reserve_token_id`.
    #[serde(default)]
    pub token_id: String,
}

// Response for reserve creation - formatted for the Ergo node's /wallet/payment/send API.
//
// The OWNER submits this payload to their own node, which signs it with the owner's wallet.
// The tracker's wallet is never involved: `change_address` is the owner's P2PK address derived
// from `owner_pubkey`. (The old `POST /reserves/submit` route and its `submission_permit` were
// removed because that route paid from the tracker's wallet, PR #16 B4.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveCreationResponse {
    pub requests: Vec<ReservePaymentRequest>,
    pub fee: u64,
    pub change_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservePaymentRequest {
    pub address: String,
    pub value: u64,
    pub assets: Vec<Asset>,
    pub registers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub token_id: String,
    pub amount: u64,
}
#[derive(Debug, Serialize)]
pub struct TrackerBoxIdResponse {
    pub tracker_box_id: String,
    pub timestamp: u64,
    pub height: u64,
    /// Tracker NFT ID (hex) that notes must bind to in their signed message (issue #15 C3).
    /// Empty when not configured.
    #[serde(default)]
    pub tracker_nft_id: String,
}

// Reserve-token configuration exposed to clients so they can display and create
// token-backed reserves with the correct token id and decimals.
#[derive(Debug, Clone, Serialize)]
pub struct ReserveTokenConfigResponse {
    pub reserve_token_id: Option<String>,
    pub reserve_token_decimals: u8,
    pub basis_token_reserve_contract_p2s: String,
}

/// One token the tracker serves as reserve collateral.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServedTokenInfo {
    pub token_id: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_collateralization_ratio: Option<f64>,
}

/// One denomination the tracker serves (ERG plus every served token).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServedDenomination {
    /// "erg" or "token:<64-hex>"
    pub id: String,
    pub symbol: String,
    pub decimals: u8,
}

/// Reserve collateral configuration: contract addresses plus the served-token table.
#[derive(Debug, Clone, Serialize)]
pub struct ReserveCollateralConfigResponse {
    pub erg_contract_p2s: String,
    pub token_contract_p2s: String,
    pub tokens: Vec<ServedTokenInfo>,
    pub denominations: Vec<ServedDenomination>,
}

// Tracker state response - shows local, confirmed and pending digests.
#[derive(Debug, Clone, Serialize)]
pub struct TrackerStateResponse {
    pub local_digest: String,
    pub confirmed_digest: Option<String>,
    pub confirmed_box_id: Option<String>,
    pub confirmed_height: Option<u64>,
    pub pending_digest: Option<String>,
    pub pending_tx_id: Option<String>,
    pub pending_submitted_height: Option<u64>,
    pub tracker_box_id: Option<String>,
}

// Pending tracker update transaction response.
#[derive(Debug, Clone, Serialize)]
pub struct PendingTxResponse {
    pub pending_tx_id: Option<String>,
    pub pending_digest: Option<String>,
    pub submitted_height: Option<u64>,
}

// Request for a single note's confirmation state.
#[derive(Debug, Deserialize)]
pub struct NoteStateRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
}

// Response for a single note's confirmation state.
#[derive(Debug, Clone, Serialize)]
pub struct NoteStateResponse {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub local: u64,
    pub confirmed: Option<u64>,
    pub pending: Option<u64>,
    pub already_redeemed: u64,
    pub redeemable: bool,
    pub redeemable_amount: u64,
    pub status: String,
}

// Request for checking note acceptance
#[derive(Debug, Deserialize)]
pub struct CheckAcceptanceRequest {
    /// Hex-encoded issuer public key (33 bytes)
    pub issuer_pubkey: String,
    /// Total cumulative debt amount
    pub total_debt: u64,
    /// Optional hex-encoded recipient public key (33 bytes) for per-recipient policy lookup
    #[serde(default)]
    pub recipient_pubkey: Option<String>,
    /// Denomination of the debt ("erg" or "token:<64-hex>"); empty means default
    /// resolution: the pair's existing note's denomination when unambiguous, else "erg".
    #[serde(default)]
    pub denomination: String,
}

// Response for checking note acceptance
#[derive(Debug, Serialize)]
pub struct CheckAcceptanceResponse {
    /// Whether the note is acceptable
    pub acceptable: bool,
    /// Optional reason for rejection
    pub reason: Option<String>,
}

// Request for uploading acceptance policy
#[derive(Debug, Deserialize)]
pub struct UploadPolicyRequest {
    /// Hex-encoded recipient public key (33 bytes)
    pub recipient_pubkey: String,
    /// Policy JSON string (serialized AcceptanceConfig)
    pub policy_json: String,
    /// Schnorr signature over policy_json (65 bytes, hex encoded)
    pub signature: String,
}

// Response for uploading acceptance policy
#[derive(Debug, Serialize)]
pub struct UploadPolicyResponse {
    /// Unix timestamp when policy was uploaded
    pub uploaded_at: u64,
    /// Hash of the policy for verification
    pub policy_hash: String,
}

// Response for getting a recipient's acceptance policy
#[derive(Debug, Serialize)]
pub struct GetPolicyResponse {
    /// Recipient public key (hex encoded)
    pub recipient_pubkey: String,
    /// Policy JSON string
    pub policy_json: String,
    /// Hex-encoded Schnorr signature (65 bytes = 130 hex chars)
    pub signature: String,
    /// Unix timestamp when policy was uploaded
    pub uploaded_at: u64,
}

// Success response helper
pub fn success_response<T>(data: T) -> ApiResponse<T> {
    ApiResponse {
        success: true,
        data: Some(data),
        error: None,
        code: None,
    }
}

// Error response helper
pub fn error_response<T>(message: String) -> ApiResponse<T> {
    ApiResponse {
        success: false,
        data: None,
        error: Some(message),
        code: None,
    }
}

/// Error response with a machine-readable `code` (see [`ApiResponse::code`]).
pub fn error_response_with_code<T>(message: String, code: &str) -> ApiResponse<T> {
    ApiResponse {
        success: false,
        data: None,
        error: Some(message),
        code: Some(code.to_string()),
    }
}
