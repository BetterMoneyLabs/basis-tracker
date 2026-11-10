//! Core data structures for Basis tracker

#[cfg(test)]
pub mod avl_queries_tests;
pub mod avl_tree;

pub mod contract_compiler;
#[cfg(test)]
pub mod cross_validation_tests;
pub mod cross_verification;
pub mod ergo_scanner;
pub mod http;
pub mod persistence;
pub mod redemption;
#[cfg(test)]
pub mod redemption_blockchain_tests;
#[cfg(test)]
pub mod redemption_simple_tests;
pub mod reserve_tracker;
#[cfg(test)]
mod review_regression_tests;
pub mod scala_test_vectors;
pub mod schnorr;
pub mod schnorr_test_vectors;
pub mod schnorr_tests;
#[cfg(test)]
pub mod simple_integration_tests;
pub mod tests;
pub mod tracker_scanner;
pub mod transaction_builder;

// Test modules
#[cfg(test)]
pub mod basis_spec_tests;
#[cfg(test)]
pub mod basis_token_spec_tests;
#[cfg(test)]
pub mod cross_verification_tests;
#[cfg(test)]
pub mod property_tests;
#[cfg(test)]
pub mod real_scanner_integration_tests;
#[cfg(test)]
pub mod reserve_tracking_test;
#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod tracker_scanner_test;

use basis_core::impls::SchnorrVerifier;
use basis_core::traits::SignatureVerifier;
use std::path::Path;

/// Public key type (Secp256k1)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1) - following chaincash-rs format: 33 bytes a + 32 bytes z
pub type Signature = [u8; 65];

/// IOU Note representing debt from A to B
///
/// `Debug` is redacted (signatures are never logged): the issuer's signature
/// appears only as its fingerprint, see [`signature_fingerprint`].
#[derive(Clone, PartialEq, Eq)]
pub struct IouNote {
    /// Recipient's public key
    pub recipient_pubkey: PubKey,
    /// Total amount ever collected (cumulative debt)
    pub amount_collected: u64,
    /// Total amount ever redeemed
    pub amount_redeemed: u64,
    /// Timestamp of latest payment/update
    pub timestamp: u64,
    /// Signature from issuer (A)
    pub signature: Signature,
    /// Tracker NFT ID this note is bound to (32 bytes). Part of the signed message,
    /// so the note is only redeemable under this tracker (issue #15 C3).
    pub tracker_nft_id: [u8; 32],
    /// Denomination of the debt (ERG or a specific token). Part of the debt-tree
    /// key derivation, so one (issuer, recipient) pair can hold an independent
    /// debt per denomination (specs/ergo/multi_collateral_reserves.md).
    pub denomination: DenomKey,
}

/// Tracker state commitment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerState {
    /// AVL+ tree root digest of all notes (32 bytes label + 1 byte height)
    pub avl_root_digest: [u8; 33],
    /// Block height of last on-chain commitment
    pub last_commit_height: u64,
    /// Timestamp of last state update
    pub last_update_timestamp: u64,
}

/// Confirmation status of a note relative to the on-chain tracker box commitment.
///
/// A note is only redeemable when its `totalDebt` is committed in the confirmed
/// on-chain tracker box R5 (i.e. `Confirmed`). Notes that are only in the local
/// tracker tree (`LocalOnly`) or in a submitted-but-unconfirmed update transaction
/// (`Pending`) cannot be redeemed yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteConfirmationStatus {
    /// Present only in the local tracker tree, not yet submitted on-chain.
    LocalOnly,
    /// Included in a tracker box update transaction that has been submitted but
    /// not yet confirmed on-chain.
    Pending,
    /// Committed in the latest confirmed on-chain tracker box R5.
    Confirmed,
}

/// Cached confirmation record for a single note, keyed by
/// `blake2b256(issuer_pubkey || recipient_pubkey)`.
///
/// The record tracks the note's value at each commitment level so that clients
/// can determine exactly how much is redeemable right now (the confirmed value)
/// versus how much is only known locally or is still in flight.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoteConfirmation {
    /// Current confirmation status of the note.
    pub status: NoteConfirmationStatus,
    /// The `totalDebt` value committed in the latest confirmed on-chain tracker
    /// box R5. `None` if the note has never been confirmed on-chain. This is the
    /// maximum amount the contract will accept for redemption.
    pub confirmed_total_debt: Option<u64>,
    /// The `totalDebt` value included in the currently in-flight tracker box
    /// update transaction. `None` if the note is not part of a pending update.
    pub pending_total_debt: Option<u64>,
    /// Box ID of the confirmed tracker box that committed `confirmed_total_debt`.
    pub confirmed_box_id: Option<String>,
    /// Height at which the confirmed tracker box was observed.
    pub confirmed_height: Option<u64>,
    /// Transaction ID of the in-flight tracker box update that covers this note.
    pub pending_tx_id: Option<String>,
}

impl NoteConfirmation {
    /// Create a fresh record for a note that exists only in the local tree.
    pub fn local_only() -> Self {
        Self {
            status: NoteConfirmationStatus::LocalOnly,
            confirmed_total_debt: None,
            pending_total_debt: None,
            confirmed_box_id: None,
            confirmed_height: None,
            pending_tx_id: None,
        }
    }

    /// Returns true when the note has a confirmed value that exceeds the
    /// `already_redeemed` amount, i.e. there is something left to redeem.
    pub fn is_redeemable(&self, already_redeemed: u64) -> bool {
        self.confirmed_total_debt
            .map(|debt| debt > already_redeemed)
            .unwrap_or(false)
    }

    /// Returns the amount that can be redeemed right now:
    /// `max(0, confirmed_total_debt - already_redeemed)`.
    pub fn redeemable_amount(&self, already_redeemed: u64) -> u64 {
        self.confirmed_total_debt
            .map(|debt| debt.saturating_sub(already_redeemed))
            .unwrap_or(0)
    }
}

impl Default for NoteConfirmation {
    fn default() -> Self {
        Self::local_only()
    }
}

/// Note key (32 bytes) used to index confirmation records.
pub type NoteKeyBytes = [u8; 32];

/// Reserve information for a public key
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReserveInfo {
    /// On-chain collateral amount (ERG nanoERG for ERG reserves, raw token units for token reserves)
    pub collateral_amount: u64,
    /// Last known block height
    pub last_updated_height: u64,
    /// Reserve contract address
    pub contract_address: String,
    /// Tracker NFT ID from R6 register (hex-encoded serialized SColl(SByte) format following byte_array_register_serialization.md spec)
    pub tracker_nft_id: String,
    /// Refund initiation height from R7 register (0 if no refund pending)
    #[serde(default)]
    pub refund_initiation_height: u64,
    /// Reserve token ID (hex-encoded) for token-backed reserves, or empty for ERG reserves.
    #[serde(default)]
    pub reserve_token_id: String,
}

/// Tracker box information for state commitment boxes
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrackerBoxInfo {
    /// Box ID (hex encoded)
    pub box_id: String,
    /// Tracker public key (hex encoded, from R4)
    pub tracker_pubkey: String,
    /// State commitment hash (hex encoded, from R5)
    pub state_commitment: String,
    /// Last verified height (from R6)
    pub last_verified_height: u64,
    /// Box value in nanoERG
    pub value: u64,
    /// Creation height
    pub creation_height: u64,
    /// Tracker NFT ID (hex encoded)
    pub tracker_nft_id: String,
}

/// Proof for a specific note against tracker state
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteProof {
    /// The IOU note being proven
    pub note: IouNote,
    /// AVL tree proof bytes
    pub avl_proof: Vec<u8>,
    /// Operations performed to generate the proof
    pub operations: Vec<u8>,
}

/// Tracker lookup proof for context var #8 in redemption transactions
/// Proves that totalDebt exists in the tracker's AVL tree at key hash(ownerKey||receiverKey)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerLookupProof {
    /// The AVL tree key: blake2b256(ownerKey || receiverKey) (32 bytes)
    pub key: Vec<u8>,
    /// The value: totalDebt as 8-byte big-endian
    pub value: Vec<u8>,
    /// AVL proof bytes for the lookup
    pub proof: Vec<u8>,
}

/// Reserve lookup proof for context var #7 in redemption transactions
/// Proves that already_redeemed exists in the reserve's AVL tree at key hash(ownerKey||receiverKey)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveLookupProof {
    /// The AVL tree key: blake2b256(ownerKey || receiverKey) (32 bytes)
    pub key: Vec<u8>,
    /// The value: timestamp || already_redeemed (16 bytes BE, zero-filled for first redemption)
    pub value: Vec<u8>,
    /// AVL proof bytes for the lookup; a proof of absence for a first redemption.
    /// Always present: context var #7 is mandatory on-chain.
    pub proof: Option<Vec<u8>>,
}

/// Key for note lookup: the denomination-derived debt-tree key
/// (blake2b256(issuer ‖ receiver) for ERG, blake2b256(issuer ‖ receiver ‖ tokenId)
/// for token denominations)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteKey {
    /// Denomination-derived debt-tree key hash (32 bytes)
    pub key_hash: [u8; 32],
}

impl NoteKey {
    /// Create a note key from issuer and recipient public keys and the denomination
    pub fn from_assets(
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Self {
        let key_hash = denomination.debt_key(issuer_pubkey, recipient_pubkey);
        Self { key_hash }
    }

    /// Create an ERG-denominated note key (blake2b256(issuer ‖ receiver)).
    pub fn from_keys(issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Self {
        Self::from_assets(issuer_pubkey, recipient_pubkey, &DenomKey::Erg)
    }

    /// Convert note key to bytes for AVL tree
    pub fn to_bytes(&self) -> Vec<u8> {
        self.key_hash.to_vec()
    }

    /// Create a note key from bytes (32-byte hash)
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self { key_hash: *bytes }
    }
}

/// Status information for a public key
#[derive(Debug, Clone, PartialEq)]
pub struct KeyStatus {
    /// Total issued debt
    pub total_debt: u64,
    /// On-chain collateral
    pub collateral: u64,
    /// Collateralization ratio (collateral / debt)
    pub collateralization_ratio: f64,
    /// Number of outstanding notes
    pub note_count: usize,
    /// Last update timestamp
    pub last_updated: u64,
}

/// Error types for note operations
#[derive(Debug)]
pub enum NoteError {
    InvalidSignature,
    AmountOverflow,
    FutureTimestamp,
    PastTimestamp,
    /// A note tried to lower the committed cumulative debt for an existing (issuer, recipient)
    /// pair. Only the creditor can consent to that; the debtor's signature is not enough.
    DebtDecreaseNotPermitted {
        previous: u64,
        requested: u64,
    },
    /// The amount or timestamp exceeds the range the on-chain contract can
    /// represent. ErgoScript reads both with the signed `byteArrayToLong`, so
    /// any value >= 2^63 comes out negative on-chain and the note can never
    /// redeem against the reserve (S3 bypass).
    AmountExceedsContractRange {
        field: &'static str,
        value: u64,
    },
    /// No note exists for the requested (issuer, recipient) pair.
    NotFound,
    /// The note is not committed in the latest confirmed on-chain tracker box,
    /// so no committed-state proof can be generated for it yet.
    NoteNotConfirmed,
    /// A cross-denomination exchange request was malformed (same denominations,
    /// zero amounts, or it would push an edge below its already-redeemed amount).
    InvalidExchange(String),
    /// A debt transfer (triad novation) was malformed or is not possible in
    /// this tracker's state: same creditors, zero amount, not the transfer
    /// residue, or more than the issuer owes the old creditor (an old
    /// creditor's edge with on-chain state is [`NoteError::OnchainScope`]).
    InvalidTransfer(String),
    /// The timestamp has a reserved residue (see [`TRANSFER_TS_MS`]): 999
    /// (transfer), 998 (set-off), 996 or 995 (reserved), or 997 (bound note)
    /// without the issuer's prior acknowledgment. Ordinary notes may not use
    /// them, so a raised edge's issuer signature from a novation can never be
    /// submitted on its own.
    ReservedTimestamp,
    /// The edge was changed by a novation (debt transfer, set-off) and is
    /// bound-only: a new note on it must be a bound note (residue 997 with the
    /// issuer's prior acknowledgment of the edge's current total and timestamp).
    BoundNoteRequired,
    /// A bound note's prior acknowledgment is not the issuer's signature over
    /// the acknowledgment `basis/prior-ack/1` of the edge's current total and
    /// timestamp at the note's timestamp (see [`schnorr::prior_ack_message`]).
    InvalidPriorSignature,
    /// An acknowledged prior total or prior timestamp sent with the request
    /// (diagnostic only) is not the edge's current total or timestamp.
    StalePrior,
    /// A bound note must raise the edge's total above the acknowledged prior
    /// total. (Kept from bound notes version 2, where the acknowledgment was a
    /// note message at the prior total and could otherwise have been submitted
    /// as a note; a version-3 acknowledgment is never a note message.)
    NotAnIncrease,
    /// A novation's timestamp (a debt transfer's, a set-off edge's) or a bound
    /// note's is older than the maximum age, [`NOVATION_MAX_AGE_MS`].
    NovationExpired,
    /// The request is malformed (for example a prior acknowledgment on a note
    /// that is not a bound note).
    InvalidRequest(String),
    /// The operation needs an edge without committed or pending on-chain state.
    OnchainScope(String),
    /// A cross-denomination exchange (issue #17) is staged on one of the
    /// operation's (issuer, creditor) pairs. The exchange computed its totals
    /// against the state the operation would change, so the operation has to
    /// wait until the exchange resolves.
    StagedExchange(String),
    /// A multilateral set-off was malformed or is not possible in this tracker's
    /// state: fewer than two or more than [`MAX_SETOFF_EDGES`] edges, duplicates,
    /// an edge's timestamp without the set-off residue, a decrease larger than an
    /// edge, or a participant whose decreases do not balance (an edge with
    /// on-chain state is [`NoteError::OnchainScope`], a staged exchange
    /// [`NoteError::StagedExchange`]).
    InvalidSetOff(String),
    /// The exchange was rejected by the recipient's acceptance policy in the
    /// target denomination. Evaluated inside the tracker thread against the
    /// pre-mutation state (finding #8), so a rejection is atomic with the
    /// would-be mutation.
    AcceptancePolicy(String),
    /// The note is bound to a different tracker NFT ID than this tracker's.
    /// Accepting it would pin debt that can never redeem against this tracker's
    /// reserves (issue #15 C3).
    TrackerIdMismatch,
    /// The note's committed on-chain total is greater than its current local
    /// total, i.e. a consented debt decrease (exchange) has not been committed
    /// yet. Building a redemption against the committed value would redeem more
    /// than the issuer currently owes on the edge (issue #17).
    StaleCommittedDebt {
        committed: u64,
        local: u64,
    },
    RedemptionTooEarly,
    InsufficientCollateral,
    StorageError(String),
    UnsupportedOperation,
}

impl From<secp256k1::Error> for NoteError {
    fn from(_: secp256k1::Error) -> Self {
        NoteError::InvalidSignature
    }
}

/// Pre-mutation tracker state handed to an exchange's acceptance-policy check
/// (finding #8). The check must run atomically with the mutation, so the
/// tracker thread supplies the exact state the exchange is about to change
/// instead of the API handler racing concurrent commands with a stale snapshot.
pub struct ExchangeAcceptanceView {
    /// The to-edge note before the exchange (`None` when the edge is new).
    pub to_note: Option<IouNote>,
    /// All of the issuer's notes before the exchange.
    pub issuer_notes: Vec<IouNote>,
    /// Confirmation records (committed/pending cumulative totals) before the
    /// exchange, keyed by note-key hash.
    pub confirmations: std::collections::HashMap<[u8; 32], NoteConfirmation>,
}

/// Status of a persisted cross-denomination exchange (issue #17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingExchangeStatus {
    /// The to-edge increase is staged; it is applied once the from-edge
    /// decrease is committed and the from-reserve is verified unchanged.
    Staged,
    /// The exchange was abandoned during staging; `reason` explains why.
    Cancelled { reason: String },
}

/// Persisted record of a two-phase cross-denomination exchange (issue #17).
///
/// Phase 1 (request time) applies only the from-edge decrease and persists this
/// record holding the signed to-edge note. Phase 2 (driven by tracker box
/// confirmations) applies the to-edge increase once the from-decrease is
/// confirmed on-chain and the from-denomination reserve is verified unchanged,
/// so a creditor cannot redeem the old from-edge note AND collect the to-edge
/// increase. Applied records are deleted by
/// [`TrackerStateManager::process_staged_exchanges`]. See
/// `specs/ergo/exchange_atomicity.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingExchangeRecord {
    pub issuer_pubkey: PubKey,
    pub recipient_pubkey: PubKey,
    pub from_denomination: DenomKey,
    /// The from-edge's collected total before the exchange decrease.
    pub from_old_total: u64,
    /// The from-edge's collected total after the decrease (applied in phase 1).
    pub from_new_total: u64,
    pub to_denomination: DenomKey,
    /// The issuer-signed to-edge note to apply in phase 2
    /// (`amount_collected == to_new_total`).
    pub to_note: IouNote,
    /// The to-edge's collected total before the exchange (`None` when the edge
    /// was new). Phase 2 refuses to apply if the edge changed meanwhile.
    pub to_base_collected: Option<u64>,
    /// Box id of the issuer's canonical from-denomination reserve at exchange
    /// time. Phase 2 requires it to still be unspent with an R5 digest equal to
    /// the tracker's reserve-tree digest.
    pub reserve_box_id: String,
    /// Unix timestamp (millis) when the exchange was staged.
    pub created_at: u64,
    pub status: PendingExchangeStatus,
}

/// Minimal info the tracker box updater needs to observe a staged exchange
/// (issue #17 phase 2).
#[derive(Debug, Clone)]
pub struct StagedExchangeInfo {
    /// From-edge note key hash (the staged record's key).
    pub key: [u8; 32],
    /// The from-denomination reserve box whose on-chain state must be unchanged.
    pub reserve_box_id: String,
}

/// On-chain observation of one staged exchange's reserve box, gathered by the
/// tracker box updater (issue #17 phase 2).
#[derive(Debug, Clone)]
pub struct ExchangeObservation {
    /// From-edge note key hash of the staged exchange.
    pub key: [u8; 32],
    /// The reserve box's on-chain R5 digest, or `None` when the box is no
    /// longer unspent (spent by a redemption, top-up, or refund completion).
    pub reserve_r5_digest: Option<[u8; 33]>,
}

/// Outcome of a phase-2 pending-exchange pass (issue #17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessedExchanges {
    /// Staged exchanges whose to-edge increase was applied.
    pub applied: usize,
    /// Staged exchanges cancelled (reason recorded on the persisted record).
    pub cancelled: usize,
}

/// Phase-2 decision for one staged exchange (issue #17).
#[derive(Debug, Clone, PartialEq, Eq)]
enum StagedExchangeDecision {
    /// Apply the staged to-edge increase.
    Apply,
    /// Cancel the exchange; the string is the persisted reason.
    Cancel(String),
    /// Keep the exchange staged (from-edge not confirmed yet, or no
    /// observation available this cycle).
    Wait,
}

/// Cumulative totals of the two edges of a debt transfer, after and before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferTotals {
    pub from_new_total: u64,
    pub to_new_total: u64,
    /// The old creditor's edge before the transfer (X): the total its prior
    /// acknowledgment covers.
    pub from_prior_total: u64,
    /// The new creditor's edge before the transfer (Y, 0 when the edge was new):
    /// the total its prior acknowledgment covers.
    pub to_prior_total: u64,
}

/// The seven signatures of a debt transfer (version 4, tracker v3.2), see
/// [`TrackerStateManager::transfer_debt`]. A is the issuer, B the old creditor,
/// C the new creditor, a the amount, T the transfer's timestamp, X and p the
/// stored A->B total and timestamp, Y and q the stored A->C total and timestamp
/// (0 and 0 when that edge is absent). Five are over the standard 80-byte note
/// message `M(I,R,t,ts) = signing_message(I, R, denomination, t, ts, nft)` at
/// T; the two `_prior` ones are over the 105-byte prior acknowledgment
/// `PA(I,R,t,p,T) = prior_ack_message(I, R, denomination, t, p, T, nft)`
/// (tag `basis/prior-ack/1`):
///
/// | field | signer | message |
/// |---|---|---|
/// | `issuer_from` | A | M(A,B,X-a,T) |
/// | `issuer_to` | A | M(A,C,Y+a,T) |
/// | `from_recipient_prior` | B | PA(A,B,X,p,T) |
/// | `from_recipient` | B | M(A,B,X-a,T) |
/// | `from_recipient_to` | B | M(A,C,Y+a,T) |
/// | `to_recipient_prior` | C | PA(A,C,Y,q,T) |
/// | `to_recipient` | C | M(A,C,Y+a,T) |
///
/// The two `_prior` signatures are the creditors' prior acknowledgments: the
/// transfer applies only while both edges are still in the acknowledged
/// states (X, p) and (Y, q). An edge's timestamp strictly increases with every
/// write, so each state occurs once: an acknowledgment can apply at most once,
/// whatever totals the edge takes later (version 3 acknowledged M(A,B,X,T) and
/// M(A,C,Y,T), the totals alone, so an attempt could apply again after both
/// edges returned to X and Y: audit VS-1).
///
/// `Debug` is redacted (signatures are never logged): each signature appears
/// only as its fingerprint, see [`signature_fingerprint`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TransferSignatures {
    pub issuer_from: Signature,
    pub issuer_to: Signature,
    pub from_recipient_prior: Signature,
    pub from_recipient: Signature,
    pub from_recipient_to: Signature,
    pub to_recipient_prior: Signature,
    pub to_recipient: Signature,
}

/// A debt transfer that passed every check of
/// [`TrackerStateManager::transfer_debt`], with the two notes it writes.
struct TransferPlan {
    from_note: IouNote,
    to_note: IouNote,
    totals: TransferTotals,
}

/// Timestamp residues (`timestamp % 1000`, the millisecond part), allocated once
/// for the whole roadmap. Every operation signs standard note messages, so a
/// signature is told apart only by the residue of its timestamp: each kind of
/// operation uses its own residue, and POST /notes refuses the residues of the
/// other operations. A raised edge's issuer signature from a transfer is a valid
/// note message on its own; its residue keeps it from being submitted through
/// POST /notes, which would raise that edge without the matching decrease, and
/// keeps one operation's signatures out of every other operation.
///
/// | residue | operation |
/// |---|---|
/// | 999 | debt transfer (POST /notes/transfer) |
/// | 998 | multilateral set-off (POST /notes/setoff) |
/// | 997 | bound note (POST /notes with the issuer's prior acknowledgment) |
/// | 996 | triad compress (reserved, not implemented) |
/// | 995 | exchange (reserved, not implemented) |
/// | 000-994 | ordinary note (POST /notes) |
pub const TRANSFER_TS_MS: u64 = 999;
/// Residue of a multilateral set-off's timestamp (see [`TRANSFER_TS_MS`]).
pub const SETOFF_TS_MS: u64 = 998;
/// Residue of a bound note's timestamp (see [`TRANSFER_TS_MS`] and [`PriorAck`]).
pub const BOUND_NOTE_TS_MS: u64 = 997;
/// Residue reserved for a triad compress (not implemented; refused by POST /notes).
pub const COMPRESS_TS_MS: u64 = 996;
/// Residue reserved for an exchange (not implemented; refused by POST /notes).
pub const EXCHANGE_TS_MS: u64 = 995;
/// The lowest reserved residue: residues `RESERVED_FROM_MS..=999` are never
/// ordinary notes, and ordinary notes use 000-994.
pub const RESERVED_FROM_MS: u64 = 995;

/// Does `timestamp` have a reserved residue (995-999 ms)? Ordinary notes may not.
pub fn is_reserved_timestamp(timestamp: u64) -> bool {
    timestamp % 1000 >= RESERVED_FROM_MS
}

/// Does `timestamp` have the debt-transfer residue (999 ms)?
pub fn is_transfer_timestamp(timestamp: u64) -> bool {
    timestamp % 1000 == TRANSFER_TS_MS
}

/// Does `timestamp` have the set-off residue (998 ms)?
pub fn is_setoff_timestamp(timestamp: u64) -> bool {
    timestamp % 1000 == SETOFF_TS_MS
}

/// Does `timestamp` have the bound-note residue (997 ms)?
pub fn is_bound_note_timestamp(timestamp: u64) -> bool {
    timestamp % 1000 == BOUND_NOTE_TS_MS
}

/// The maximum age of a novation's or a bound note's timestamp, in
/// milliseconds (one hour). A debt transfer whose timestamp, a set-off with any
/// edge whose timestamp, or a bound note (POST /notes at residue 997, see
/// [`PriorAck`]) whose timestamp is older than `now - NOVATION_MAX_AGE_MS` is
/// refused with [`NoteError::NovationExpired`]. This bounds how long unapplied
/// signatures stay usable: a payment attempt (a transfer or a bound note) that
/// has not applied by its timestamp plus this age never will, which is what
/// lets a payer fall back to another kind of payment safely. Compiled in, not
/// configurable.
pub const NOVATION_MAX_AGE_MS: u64 = 3_600_000;

/// An issuer prior acknowledgment, sent with a bound note (POST /notes at
/// residue 997; bound notes version 3, tracker v3.2): the issuer's signature
/// over the prior acknowledgment of the edge's CURRENT state (P, p) at the
/// bound note's own timestamp ts,
/// `PA(I,R,P,p,ts) = prior_ack_message(issuer, recipient, denomination, P, p, ts, nft)`,
/// where P is the stored total and p the stored note's timestamp (0 and 0 for an
/// edge that does not exist yet). The tracker applies the bound note only while
/// the edge is still in that state and the note raises it, so a note signed
/// against a stale state can never apply: the issuer pays exactly `new - P`,
/// both values it signed. Each state (P, p) of an edge occurs once (the
/// timestamp strictly increases with every write), so two bound notes on the
/// same prior exclude each other even if a transfer or set-off lowers the edge
/// back to P in between (version 2 acknowledged the note message M(I,R,P,ts),
/// the total alone). Like a novation, a bound note expires: one whose timestamp
/// is older than [`NOVATION_MAX_AGE_MS`] is refused, so an unapplied bound note
/// is dead an hour after its timestamp.
///
/// `Debug` is redacted (signatures are never logged): the signature appears
/// only as its fingerprint, see [`signature_fingerprint`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PriorAck {
    /// The issuer's signature over the prior acknowledgment (65 bytes).
    pub signature: Signature,
    /// Optional and diagnostic: the prior total the issuer acknowledged. When
    /// present it must equal the stored total, else [`NoteError::StalePrior`].
    pub prior_total: Option<u64>,
    /// Optional and diagnostic: the prior timestamp the issuer acknowledged (0
    /// for an absent edge). When present it must equal the stored note's
    /// timestamp, else [`NoteError::StalePrior`].
    pub prior_timestamp: Option<u64>,
}

/// Upper bound on the edges of one set-off (keeps verification and the storage
/// batch small).
pub const MAX_SETOFF_EDGES: usize = 64;

/// One edge lowered by a multilateral set-off (version 3), with both parties'
/// signatures over the edge's standard note message at its new total and its
/// own timestamp: `M(I,R,P-d,ts_e) = signing_message(I, R, denomination, P - d,
/// ts_e, nft)`, where P is the edge's stored total and d its `decrease`.
///
/// `Debug` is redacted (signatures are never logged): both signatures appear
/// only as fingerprints, see [`signature_fingerprint`].
#[derive(Clone)]
pub struct SetOffEdge {
    pub issuer_pubkey: PubKey,
    pub recipient_pubkey: PubKey,
    pub decrease: u64,
    /// This edge's timestamp ts_e (per-edge timestamps): it must end in 998 ms
    /// ([`SETOFF_TS_MS`]), be at most now and at most [`NOVATION_MAX_AGE_MS`]
    /// old, and be newer than the edge's stored timestamp. The lowered note is
    /// stored at it. Edges of one set-off may have different timestamps, so a
    /// moved edge can be re-signed on its own.
    pub timestamp: u64,
    pub issuer_signature: Signature,
    pub recipient_signature: Signature,
}

/// One edge as a set-off left it: its new total and its new timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetOffEdgeTotal {
    pub issuer_pubkey: PubKey,
    pub recipient_pubkey: PubKey,
    /// The edge's total after the set-off (its stored total minus the decrease).
    pub new_total: u64,
    /// The edge's timestamp after the set-off (the edge's ts_e).
    pub timestamp: u64,
}

/// Result of a set-off: every lowered edge's new total and timestamp, in the
/// order of the request's edges, and the gross debt cleared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetOffResult {
    pub new_totals: Vec<SetOffEdgeTotal>,
    pub total_cleared: u64,
}

/// The 8-hex-character fingerprint of a signature: the first 4 bytes of the
/// SHA-256 of its bytes, as lowercase hex. Debug output shows a signature only
/// as `sha256:<fingerprint>` (design §3.8: signatures are never logged). A
/// fingerprint identifies a signature its holder already knows without
/// revealing any of it. basis_server's `redact` module uses the same
/// fingerprint, so a request's log line and its command's can be matched.
pub fn signature_fingerprint(signature: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(signature)[..4])
}

/// Field renderings for the redacted `Debug` impls of the store types that
/// carry signatures ([`IouNote`], [`PriorAck`], [`TransferSignatures`],
/// [`SetOffEdge`]): their derived `Debug` printed every signature as a list of
/// 65 numbers, and any `{:?}` of them would put it in a log.
mod redacted_debug {
    use std::fmt;

    /// A signature, as `sha256:<fingerprint>`.
    pub(crate) struct Sig<'a>(pub(crate) &'a [u8]);

    impl fmt::Debug for Sig<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "sha256:{}", super::signature_fingerprint(self.0))
        }
    }

    /// Bytes (a public key, an NFT id) as lowercase hex.
    pub(crate) struct Hex<'a>(pub(crate) &'a [u8]);

    impl fmt::Debug for Hex<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&hex::encode(self.0))
        }
    }
}

/// Redacted: the signature only as its fingerprint, the denomination as its id.
impl std::fmt::Debug for IouNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use redacted_debug::{Hex, Sig};
        f.debug_struct("IouNote")
            .field("recipient_pubkey", &Hex(&self.recipient_pubkey))
            .field("amount_collected", &self.amount_collected)
            .field("amount_redeemed", &self.amount_redeemed)
            .field("timestamp", &self.timestamp)
            .field("signature", &Sig(&self.signature))
            .field("tracker_nft_id", &Hex(&self.tracker_nft_id))
            .field("denomination", &self.denomination.denomination_id())
            .finish()
    }
}

/// Redacted: the signature only as its fingerprint.
impl std::fmt::Debug for PriorAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PriorAck")
            .field("signature", &redacted_debug::Sig(&self.signature))
            .field("prior_total", &self.prior_total)
            .field("prior_timestamp", &self.prior_timestamp)
            .finish()
    }
}

/// Redacted: each of the seven signatures only as its fingerprint.
impl std::fmt::Debug for TransferSignatures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use redacted_debug::Sig;
        f.debug_struct("TransferSignatures")
            .field("issuer_from", &Sig(&self.issuer_from))
            .field("issuer_to", &Sig(&self.issuer_to))
            .field("from_recipient_prior", &Sig(&self.from_recipient_prior))
            .field("from_recipient", &Sig(&self.from_recipient))
            .field("from_recipient_to", &Sig(&self.from_recipient_to))
            .field("to_recipient_prior", &Sig(&self.to_recipient_prior))
            .field("to_recipient", &Sig(&self.to_recipient))
            .finish()
    }
}

/// Redacted: both signatures only as fingerprints.
impl std::fmt::Debug for SetOffEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use redacted_debug::{Hex, Sig};
        f.debug_struct("SetOffEdge")
            .field("issuer_pubkey", &Hex(&self.issuer_pubkey))
            .field("recipient_pubkey", &Hex(&self.recipient_pubkey))
            .field("decrease", &self.decrease)
            .field("timestamp", &self.timestamp)
            .field("issuer_signature", &Sig(&self.issuer_signature))
            .field("recipient_signature", &Sig(&self.recipient_signature))
            .finish()
    }
}

/// Boxed acceptance-policy check for an exchange (see
/// [`TrackerStateManager::exchange_debt_checked`]). `Err(reason)` aborts the
/// exchange without mutating.
pub type ExchangeAcceptanceCheckFn =
    Box<dyn FnOnce(ExchangeAcceptanceView) -> Result<(), String> + Send>;

/// What opening the store found (see [`TrackerStateManager::load_report`]): the
/// loading problems a pure-credit tracker refuses to start on, and the
/// confirmation-load failure that makes every tracker refuse novations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreLoadReport {
    /// Note records the index rebuild could not decode and skipped.
    pub undecodable_note_records: usize,
    /// Loading steps that failed while the store opened, each as
    /// "<step>: <error>" (the note index rebuild, the tree rebuild).
    pub load_errors: Vec<String>,
    /// Why the confirmation records failed to load (from the latest
    /// [`TrackerStateManager::rebuild_confirmations`]), if they did. While it is
    /// set the store cannot tell which edges carry on-chain state, so it refuses
    /// novations (debt transfer, set-off) with [`NoteError::OnchainScope`].
    pub confirmations_error: Option<String>,
}

/// Why a store must not serve as a pure-credit tracker (design §3.7), from
/// [`TrackerStateManager::check_pure_credit`]. Its `Display` is the startup
/// error message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PureCreditRefusal {
    /// Stored confirmation records with a committed or pending on-chain total.
    pub onchain_confirmations: usize,
    /// Note records that could not be decoded.
    pub undecodable_note_records: usize,
    /// Notes or confirmations that failed to load, each as "<step>: <error>".
    pub load_errors: Vec<String>,
}

impl std::fmt::Display for PureCreditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut problems = Vec::new();
        if self.onchain_confirmations > 0 {
            problems.push(format!(
                "{} stored confirmation record(s) carry a committed or pending on-chain total",
                self.onchain_confirmations
            ));
        }
        if self.undecodable_note_records > 0 {
            problems.push(format!(
                "{} note record(s) could not be decoded",
                self.undecodable_note_records
            ));
        }
        for e in &self.load_errors {
            problems.push(format!("loading failed ({})", e));
        }
        write!(
            f,
            "refusing to start in pure_credit mode: {}. A pure-credit tracker serves only \
             credit edges that were never committed on-chain, and it must be able to read every \
             stored record; start without server.pure_credit, or with a data directory that \
             qualifies.",
            problems.join("; ")
        )
    }
}

impl std::error::Error for PureCreditRefusal {}

/// The public reason novations are refused while the confirmation records
/// failed to load (GET /capabilities `novations_disabled_reason`, and the 503
/// of the novation routes). Fixed text: the underlying error is only logged.
pub const NOVATIONS_DISABLED_CONFIRMATIONS: &str = "the tracker's confirmation records failed to load, so it cannot tell which edges carry on-chain state; debt transfers and set-offs are refused until it restarts with readable records";

/// Tracker state manager with persistent AVL tree
pub struct TrackerStateManager {
    avl_state: basis_trees::BasisAvlTree,
    current_state: TrackerState,
    storage: persistence::NoteStorage,
    /// Per-(issuer, denomination) reserve AVL trees tracking
    /// hash(ownerKey || receiverKey) -> (timestamp, already_redeemed) (16 bytes BE).
    /// Each (issuer, denomination) pair has its own tree so that a fresh reserve
    /// starts from the empty tree digest and first redemptions succeed even if other
    /// issuers or denominations have redemption history.
    ///
    /// NOTE: Each on-chain reserve box has its own R5 digest, so this design assumes ONE
    /// active reserve per (issuer, denomination). That assumption is load-bearing, not
    /// cosmetic: a note's signed message is only
    /// `debtKey || totalDebt || timestamp || trackerNftId` and is not bound to a reserve box,
    /// so with two reserves of the same denomination under the same owner and tracker the
    /// SAME note redeems in full against EACH of them -- overpaying the creditor and
    /// draining the owner's other reserves to do it.
    ///
    /// This cannot be fixed here (the redemption state lives in the contract's per-box R5),
    /// so the tracker fails closed instead: see `ReserveTracker::resolve_issuer_reserve`,
    /// which returns an error for a multi-reserve (issuer, denomination) pair.
    /// See specs/pr14_triage.md (C3) and specs/ergo/multi_collateral_reserves.md.
    reserve_avl_states: std::collections::HashMap<(PubKey, DenomKey), basis_trees::BasisAvlTree>,
    /// Per-note confirmation records, keyed by note key (32 bytes).
    confirmations: std::collections::HashMap<NoteKeyBytes, NoteConfirmation>,
    /// AVL root digest of the latest confirmed on-chain tracker box (R5).
    /// Redemption proofs must be generated against this committed state, not the
    /// live tree: any note posted after the last on-chain commitment changes the
    /// live digest, which would make live-tree proofs fail against the on-chain
    /// tracker box (issue #15 S6). `None` until an update confirms or the updater
    /// observes a matching on-chain digest after restart.
    confirmed_digest: Option<[u8; 33]>,
    /// Set when a canonical tree rebuild failed after storage was committed
    /// (issue #20): the live tree is then stale relative to storage, and
    /// `update_state` retries the rebuild before refreshing the committed
    /// digest. The old (valid) tree is kept in place meanwhile, so a
    /// half-rebuilt tree is never committed.
    tree_dirty: bool,
    /// What opening the store found (undecodable records, failed loading
    /// steps, a confirmation-load failure); see [`Self::load_report`].
    load_report: StoreLoadReport,
    /// Test-only fault-injection hook: while set, `rebuild_tree_canonically`
    /// returns an error without touching the live tree (issue #20 tests).
    #[cfg(test)]
    fail_tree_rebuild: bool,
}

impl TrackerStateManager {
    /// Create a new tracker state manager with the configured storage location.
    ///
    /// Loading problems (undecodable note records, failed loading steps, a
    /// confirmation-load failure) are logged and recorded in
    /// [`Self::load_report`]; they do not stop the store from opening. A
    /// pure-credit server refuses to start on them ([`Self::check_pure_credit`]),
    /// and every server refuses novations while the confirmation records failed
    /// to load ([`Self::novations_disabled_reason`]).
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        tracing::debug!("Creating TrackerStateManager...");
        let mut load_report = StoreLoadReport::default();

        // Use the configured storage location
        tracing::debug!("Opening note storage...");
        let storage_path = data_dir.as_ref().join("notes");
        let storage = match persistence::NoteStorage::open(&storage_path) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully at: {:?}", storage_path);
                // Rebuild indices to ensure all existing notes are indexed
                // (especially important after upgrading to indexed storage)
                match storage.rebuild_indices() {
                    Ok(scan) => {
                        tracing::info!("Note indices rebuilt: {} notes indexed", scan.decoded);
                        load_report.undecodable_note_records = scan.skipped;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to rebuild note indices: {:?}", e);
                        load_report
                            .load_errors
                            .push(format!("note index rebuild: {:?}", e));
                    }
                }
                storage
            }
            Err(e) => {
                tracing::error!("Failed to initialize note storage: {:?}", e);
                // Fallback to in-memory storage if file storage fails
                // In production, this should handle errors properly
                panic!("Failed to initialize note storage: {:?}", e);
            }
        };

        // Create in-memory AVL tree
        let avl_state = match basis_trees::BasisAvlTree::new() {
            Ok(tree) => {
                tracing::debug!("In-memory AVL tree created successfully");
                tree
            }
            Err(e) => {
                tracing::error!("Failed to initialize AVL tree: {:?}", e);
                panic!("Failed to initialize AVL tree: {:?}", e);
            }
        };

        // Per-issuer reserve AVL trees start empty; they are populated as redemptions complete.
        let reserve_avl_states = std::collections::HashMap::new();

        // Rebuild AVL tree from all stored notes to ensure consistency after restart
        let mut manager = Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
            reserve_avl_states,
            confirmations: std::collections::HashMap::new(),
            confirmed_digest: None,
            tree_dirty: false,
            load_report,
            #[cfg(test)]
            fail_tree_rebuild: false,
        };

        if let Err(e) = manager.rebuild_avl_tree() {
            tracing::warn!("Failed to rebuild AVL tree from storage: {:?}", e);
            manager
                .load_report
                .load_errors
                .push(format!("tree rebuild: {:?}", e));
        }

        // Rebuild confirmation records from storage and mark every stored note as
        // LocalOnly until the updater confirms otherwise. A failure is recorded
        // in the load report (see `rebuild_confirmations`).
        if let Err(e) = manager.rebuild_confirmations() {
            tracing::error!(
                "Confirmation records failed to load: {:?}. Debt transfers and set-offs are refused until the tracker restarts with readable records.",
                e
            );
        }

        // Rebuild per-issuer reserve AVL trees from the persisted journal so that
        // redemptions can continue after a server restart without manual repair.
        if let Err(e) = manager.rebuild_reserve_avl_trees() {
            tracing::warn!("Failed to rebuild reserve AVL trees from storage: {:?}", e);
        }

        tracing::debug!("TrackerStateManager created successfully");
        manager
    }

    /// Rebuild the AVL tree from all notes stored in the database.
    /// This is critical after server restart to ensure the AVL tree matches
    /// the on-chain commitment. AVL trees are insertion-order sensitive,
    /// so notes must be inserted in chronological order (by timestamp).
    pub fn rebuild_avl_tree(&mut self) -> Result<(), NoteError> {
        tracing::info!("Rebuilding AVL tree from stored notes...");

        Self::populate_tree_from_storage(&self.storage, &mut self.avl_state)?;

        self.update_state();
        let root_digest = self.current_state.avl_root_digest;
        tracing::info!(
            "AVL tree rebuilt successfully with root digest: {}",
            hex::encode(root_digest)
        );

        Ok(())
    }

    /// Apply all stored notes to `tree` in canonical (timestamp-sorted) order.
    /// Shared by `rebuild_avl_tree` (in-place, at startup) and
    /// `rebuild_tree_canonically` (into a fresh tree, swapped in on success).
    fn populate_tree_from_storage(
        storage: &persistence::NoteStorage,
        tree: &mut basis_trees::BasisAvlTree,
    ) -> Result<(), NoteError> {
        let mut notes_with_issuer = storage
            .get_all_notes_with_issuer()
            .map_err(|e| NoteError::StorageError(format!("Failed to get all notes: {:?}", e)))?;

        // AVL tree structure depends on insertion order, so notes must be
        // inserted in the canonical chronological order every rebuild uses.
        notes_with_issuer.sort_by_key(|(_, note)| note.timestamp);

        for (issuer_pubkey, note) in &notes_with_issuer {
            let key =
                NoteKey::from_assets(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
            let key_bytes = key.to_bytes();
            let value_bytes = note.amount_collected.to_be_bytes().to_vec();

            tree.update(key_bytes, value_bytes).map_err(|e| {
                NoteError::StorageError(format!("AVL tree update failed during rebuild: {:?}", e))
            })?;
        }
        Ok(())
    }

    /// Create a new tracker state manager with temporary storage (used in tests only)
    pub fn new_with_temp_storage() -> Self {
        tracing::debug!("Creating TrackerStateManager (test version with temporary storage)...");

        // Use a temporary directory for storage to avoid test conflicts
        tracing::debug!("Opening note storage...");
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let storage_path = std::env::temp_dir().join(format!(
            "basis_test_{}_{}_{}",
            unique_id,
            std::process::id(),
            rand::random::<u64>()
        ));

        // Try to clean up any existing storage at this path first
        let _ = std::fs::remove_dir_all(&storage_path);

        let storage = match persistence::NoteStorage::open(&storage_path) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully at: {:?}", storage_path);
                storage
            }
            Err(e) => {
                tracing::error!(
                    "Failed to initialize note storage: {:?}. Retrying with new path...",
                    e
                );

                // Create a new unique path if the first one failed
                let unique_id_retry = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
                    + rand::random::<u128>();
                let storage_path_retry = std::env::temp_dir().join(format!(
                    "basis_test_retry_{}_{}_{}",
                    unique_id_retry,
                    std::process::id(),
                    rand::random::<u64>()
                ));

                // Try to clean up the retry path as well
                let _ = std::fs::remove_dir_all(&storage_path_retry);

                match persistence::NoteStorage::open(&storage_path_retry) {
                    Ok(storage) => {
                        tracing::debug!(
                            "Note storage opened successfully at retry path: {:?}",
                            storage_path_retry
                        );
                        storage
                    }
                    Err(e2) => {
                        tracing::error!("Failed to initialize note storage on retry: {:?}", e2);
                        // Fallback to in-memory storage if file storage fails
                        // In production, this should handle errors properly
                        panic!("Failed to initialize note storage: {:?}", e);
                    }
                }
            }
        };

        // Create in-memory AVL tree
        let avl_state = match basis_trees::BasisAvlTree::new() {
            Ok(tree) => {
                tracing::debug!("In-memory AVL tree created successfully");
                tree
            }
            Err(e) => {
                tracing::error!("Failed to initialize AVL tree: {:?}", e);
                panic!("Failed to initialize AVL tree: {:?}", e);
            }
        };

        // Per-issuer reserve AVL trees start empty in test instances.
        let reserve_avl_states = std::collections::HashMap::new();

        tracing::debug!("TrackerStateManager created successfully");
        let mut manager = Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
            reserve_avl_states,
            confirmations: std::collections::HashMap::new(),
            confirmed_digest: None,
            tree_dirty: false,
            load_report: StoreLoadReport::default(),
            #[cfg(test)]
            fail_tree_rebuild: false,
        };

        // Rebuild AVL tree and confirmations so test instances mirror production.
        if let Err(e) = manager.rebuild_avl_tree() {
            tracing::warn!("Failed to rebuild AVL tree in test instance: {:?}", e);
            manager
                .load_report
                .load_errors
                .push(format!("tree rebuild: {:?}", e));
        }
        if let Err(e) = manager.rebuild_confirmations() {
            tracing::warn!("Failed to rebuild confirmations in test instance: {:?}", e);
        }

        manager
    }

    /// Reject values the on-chain contract cannot represent.
    ///
    /// The reserve contract reads `totalDebt` and `timestamp` with ErgoScript's
    /// signed `byteArrayToLong`, so any value >= 2^63 comes out negative
    /// on-chain: the creditor's claim would be permanently bricked (the
    /// contract's collateral arithmetic no longer matches the tracker's).
    fn validate_contract_range(note: &IouNote) -> Result<(), NoteError> {
        if note.amount_collected > i64::MAX as u64 {
            return Err(NoteError::AmountExceedsContractRange {
                field: "amount_collected",
                value: note.amount_collected,
            });
        }
        if note.timestamp > i64::MAX as u64 {
            return Err(NoteError::AmountExceedsContractRange {
                field: "timestamp",
                value: note.timestamp,
            });
        }
        Ok(())
    }

    /// The last check before a write gives the (issuer, recipient,
    /// denomination) edge of `note` a new state: `note.timestamp` must be later
    /// than the edge's stored timestamp, else `PastTimestamp` and nothing is
    /// written. An edge's timestamp then strictly increases with every write,
    /// so each state (total, timestamp) of an edge occurs once, which the prior
    /// acknowledgments (tracker v3.2) rely on: one applies at most once. Every
    /// path that writes a new state calls it right before its storage commit:
    /// `add_note`, `update_note`, `exchange_debt` (the from-edge),
    /// `apply_staged_exchange` (the to-edge), `apply_transfer` (both edges) and
    /// `set_off` (every edge). Each checks the timestamp earlier on its own
    /// (`decide_staged_exchange` cancels an exchange whose to-edge moved past
    /// it); this guard holds even where such a check is missed.
    /// `record_redemption` is not a new state (it keeps the stored total,
    /// timestamp and signature and raises only the redeemed amount), so it does
    /// not call it.
    fn require_later_timestamp(
        &self,
        issuer_pubkey: &PubKey,
        note: &IouNote,
    ) -> Result<(), NoteError> {
        match self.get_note_optional(issuer_pubkey, &note.recipient_pubkey, &note.denomination)? {
            Some(stored) if note.timestamp <= stored.timestamp => {
                tracing::error!(
                    "Write refused: the new timestamp {} is not later than the edge's stored timestamp {}",
                    note.timestamp,
                    stored.timestamp
                );
                Err(NoteError::PastTimestamp)
            }
            _ => Ok(()),
        }
    }

    /// Add a new note to the tracker state
    /// Updates the AVL tree with hash(issuer||receiver) -> totalDebt mapping
    ///
    /// `prior` is the issuer's prior acknowledgment of a bound note (timestamp
    /// residue 997, see [`PriorAck`]); it must be `None` for every other note.
    /// Every check that existed before bound notes runs first, in its original
    /// order, so existing error precedence is kept; then the residue and
    /// bound-note rules (see [`Self::check_residue_and_binding`]).
    pub fn add_note(
        &mut self,
        issuer_pubkey: &PubKey,
        note: &IouNote,
        prior: Option<PriorAck>,
    ) -> Result<(), NoteError> {
        Self::validate_contract_range(note)?;

        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;

        if note.timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }

        // Carry the already-redeemed amount forward from the previous note for this pair.
        //
        // A new note for an existing (issuer, recipient) pair is an increase of the SAME cumulative
        // debt, not a fresh claim. Persisting the incoming note verbatim would reset
        // `amount_redeemed` to 0 and destroy the outstanding-debt / FIFO accounting, so the
        // tracker's view of "how much of this edge is still outstanding" would grow without bound.
        //
        // Only the LOCAL record is adjusted: the AVL value committed on-chain is `amount_collected`,
        // which the contract pins, so the signed message and the on-chain commitment are unchanged.
        // The per-issuer reserve AVL tree (which is what the contract actually reads) is untouched.
        let mut note_to_store = note.clone();
        // The edge's stored state (P and p for a bound note): 0 and 0 when the edge is absent.
        let mut stored_total = 0u64;
        let mut stored_timestamp = 0u64;

        // Check if there is an existing note with the same issuer-recipient pair
        // and ensure the new timestamp is greater than the existing one (ever increasing).
        // Only `NotFound` means "first note for this pair": a storage or decode
        // error must propagate, not silently skip the timestamp and
        // debt-monotonicity checks below.
        match self.lookup_note(issuer_pubkey, &note.recipient_pubkey, &note.denomination) {
            Ok(existing_note) => {
                if note.timestamp <= existing_note.timestamp {
                    return Err(NoteError::PastTimestamp);
                }
                note_to_store.amount_redeemed = existing_note.amount_redeemed;
                // SECURITY: cumulative debt may never decrease without the creditor's consent.
                //
                // The contract pins every redemption to the tracker tree, so lowering the committed
                // totalDebt for an (issuer, recipient) pair makes every existing note for that pair
                // permanently unredeemable: `trackerDebtCorrect` fails in the normal path AND in the
                // emergency path, because both compare the note's totalDebt against the committed value.
                // A debtor could therefore post totalDebt = 0, wait out the reserve's refund waiting
                // period, and withdraw the collateral while the creditor's claim is silently dead.
                //
                // The debtor's signature proves that the debtor consents, never that the creditor does.
                // Transferring debt to a *different* creditor is unaffected, because that is a different
                // key in the tree; reducing an existing creditor's claim is not something the debtor can
                // do unilaterally.
                if note.amount_collected < existing_note.amount_collected {
                    return Err(NoteError::DebtDecreaseNotPermitted {
                        previous: existing_note.amount_collected,
                        requested: note.amount_collected,
                    });
                }
                stored_total = existing_note.amount_collected;
                stored_timestamp = existing_note.timestamp;
            }
            Err(NoteError::NotFound) => {}
            Err(e) => return Err(e),
        }

        // Verify the note signature before storing it
        note.verify_signature(issuer_pubkey).map_err(|e| {
            tracing::error!("Invalid note signature when adding note: {:?}", e);
            NoteError::InvalidSignature
        })?;

        // Prepare AVL tree key: the denomination-derived debt key
        let key = NoteKey::from_assets(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
        let key_bytes = key.to_bytes();

        // Timestamp residues, bound notes (with their maximum age) and
        // bound-only edges. Checked after every other validation, so every
        // existing error keeps its priority.
        self.check_residue_and_binding(
            issuer_pubkey,
            note,
            &key.key_hash,
            stored_total,
            stored_timestamp,
            prior,
            current_time,
        )?;

        // Value is just the totalDebt (amount_collected) as 8-byte big-endian
        // This matches the contract spec: hash(A||B) -> totalDebt
        let value_bytes = note.amount_collected.to_be_bytes().to_vec();

        // Persist FIRST (a single atomic batch inside store_note): a storage
        // failure then leaves nothing changed anywhere. The in-memory tree is
        // updated after, so a tree failure leaves storage merely AHEAD of the
        // tree — the safe direction: the committed digest lacks a note the
        // tracker cannot prove yet, which is indistinguishable from the normal
        // "local, not yet committed" state, and the dirty flag makes
        // update_state rebuild the tree from storage before the next commit.
        // (The reverse order committed debt the tracker had no record of —
        // issue #20 follow-up.)
        self.require_later_timestamp(issuer_pubkey, &note_to_store)?;
        self.storage.store_note(issuer_pubkey, &note_to_store)?;
        if let Err(e) = self.avl_state.update(key_bytes.clone(), value_bytes) {
            self.tree_dirty = true;
            tracing::error!(
                "Note stored but the live tree update failed; the tree is stale and will be rebuilt from storage before the next digest update: {:?}",
                e
            );
            return Err(NoteError::StorageError(e.to_string()));
        }
        self.update_state();

        // Recompute the confirmation status for this note based on the
        // new local value versus the confirmed/pending values. The local
        // value has just changed, so the note is only Confirmed/Pending
        // if the new value matches what is already on-chain / in-flight.
        let mut key32 = [0u8; 32];
        key32.copy_from_slice(&key_bytes);
        self.recompute_confirmation_status(&key32, note.amount_collected);

        Ok(())
    }

    /// The timestamp-residue and bound-note rules of POST /notes, in this order
    /// (each refusal is checked only if the ones before it passed):
    /// 1. residue 995, 996, 998 or 999 (another operation's) -> `ReservedTimestamp`;
    /// 2. residue 997 (bound note) without `prior` -> `ReservedTimestamp`;
    /// 3. residue 997 with `prior`, where P = `stored_total` and p =
    ///    `stored_timestamp` (0 and 0 for an absent edge): a timestamp older than
    ///    `now` - [`NOVATION_MAX_AGE_MS`] -> `NovationExpired` (a bound note is a
    ///    payment attempt and expires like a novation, so an unapplied one is dead
    ///    an hour after its timestamp); a `prior_total` other than P or a
    ///    `prior_timestamp` other than p -> `StalePrior`; the issuer's signature
    ///    over the prior acknowledgment (bound notes version 3)
    ///    `prior_ack_message(issuer, recipient, denomination, P, p, timestamp, nft)`
    ///    must verify, else `InvalidPriorSignature`; the new total must be
    ///    strictly greater than P, else `NotAnIncrease`;
    /// 4. any other residue on a bound-only edge -> `BoundNoteRequired`;
    /// 5. a `prior` on a note that is not a bound note -> `InvalidRequest`.
    #[allow(clippy::too_many_arguments)]
    fn check_residue_and_binding(
        &self,
        issuer_pubkey: &PubKey,
        note: &IouNote,
        key: &NoteKeyBytes,
        stored_total: u64,
        stored_timestamp: u64,
        prior: Option<PriorAck>,
        now: u64,
    ) -> Result<(), NoteError> {
        let bound_note = is_bound_note_timestamp(note.timestamp);
        if is_reserved_timestamp(note.timestamp) && !bound_note {
            return Err(NoteError::ReservedTimestamp);
        }
        if bound_note {
            let prior = prior.ok_or(NoteError::ReservedTimestamp)?;
            if note.timestamp < now.saturating_sub(NOVATION_MAX_AGE_MS) {
                return Err(NoteError::NovationExpired);
            }
            if prior.prior_total.is_some_and(|t| t != stored_total)
                || prior.prior_timestamp.is_some_and(|p| p != stored_timestamp)
            {
                return Err(NoteError::StalePrior);
            }
            // The acknowledgment binds the edge's state (P, p) and this note's
            // timestamp: an edge state occurs once, so it applies at most once.
            let prior_message = schnorr::prior_ack_message(
                issuer_pubkey,
                &note.recipient_pubkey,
                &note.denomination,
                stored_total,
                stored_timestamp,
                note.timestamp,
                &note.tracker_nft_id,
            );
            if SchnorrVerifier
                .verify_signature(&prior.signature, &prior_message, issuer_pubkey)
                .is_err()
            {
                return Err(NoteError::InvalidPriorSignature);
            }
            if note.amount_collected <= stored_total {
                return Err(NoteError::NotAnIncrease);
            }
            return Ok(());
        }
        if self.storage.is_bound_only(key)? {
            return Err(NoteError::BoundNoteRequired);
        }
        if prior.is_some() {
            return Err(NoteError::InvalidRequest(
                "prior_signature is only valid on a bound note (timestamp ending in 997 ms)"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Is the (issuer, recipient, denomination) edge bound-only, i.e. changed by
    /// a novation (debt transfer, set-off)? New notes on a bound-only edge must
    /// be bound notes. A storage error is returned, never read as "not bound".
    pub fn is_bound_only(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<bool, NoteError> {
        let key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, denomination);
        self.storage.is_bound_only(&key.key_hash)
    }

    /// Convert an issuer/recipient/denomination triple into the fixed-size confirmation key.
    fn confirmation_key(
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> NoteKeyBytes {
        let key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, denomination);
        let bytes = key.to_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    }

    /// Recompute a single note's confirmation status from its local value versus
    /// the cached confirmed/pending values. Persists the result best-effort,
    /// unless confirmation write-backs are suspended
    /// ([`Self::confirmation_write_back_allowed`]).
    fn recompute_confirmation_status(&mut self, key: &NoteKeyBytes, local_value: u64) {
        let write_back = self.confirmation_write_back_allowed();
        let entry = self
            .confirmations
            .entry(*key)
            .or_insert_with(NoteConfirmation::local_only);

        let confirmed = entry.confirmed_total_debt;
        let pending = entry.pending_total_debt;

        entry.status = if Some(local_value) == confirmed {
            NoteConfirmationStatus::Confirmed
        } else if Some(local_value) == pending {
            NoteConfirmationStatus::Pending
        } else {
            NoteConfirmationStatus::LocalOnly
        };

        if write_back {
            let _ = self.storage.store_confirmation(key, entry);
        } else {
            Self::log_write_back_suspended("add_note", 1);
        }
    }

    /// Whether confirmation records may be written back to storage: not while
    /// the stored records failed to load ([`StoreLoadReport::confirmations_error`]).
    /// The in-memory records are then built without them (every note
    /// local-only), so writing one back would replace stored evidence of
    /// on-chain state (a committed or pending total, or a record that does not
    /// parse) with a readable local-only record: the evidence would be erased,
    /// and after a restart the load would succeed and the edge would look
    /// pure-credit, so novations would be allowed on it again. While this is
    /// false, every write-back (a note's recomputed status, the tracker box
    /// updater's pending, confirmed, reverted and reconciled totals, an
    /// exchange's confirmation records) updates the in-memory record only, and
    /// storage keeps what it had until a rebuild succeeds. Novations never get
    /// as far as their write: their scope check refuses them first. A
    /// pure-credit tracker never runs in this state (it refuses to start).
    fn confirmation_write_back_allowed(&self) -> bool {
        self.load_report.confirmations_error.is_none()
    }

    /// The warning for confirmation records `source` did not write back
    /// (see [`Self::confirmation_write_back_allowed`]).
    fn log_write_back_suspended(source: &str, count: usize) {
        if count > 0 {
            tracing::warn!(
                "{}: {} confirmation record(s) updated in memory only, not written back: the stored confirmation records failed to load and are kept as they are until a rebuild succeeds",
                source,
                count
            );
        }
    }

    /// Rebuild the in-memory confirmation map from storage. Called on startup.
    ///
    /// On-chain state may have advanced while the server was offline, so we keep
    /// the persisted `confirmed_total_debt` metadata but recompute the status from
    /// the current local value and clear any pending in-flight state (pending
    /// transactions do not survive a restart).
    ///
    /// Returns the number of confirmation records rebuilt. Fails closed: when
    /// the notes or the stored confirmation records fail to load, it returns the
    /// error and records it in the load report
    /// ([`StoreLoadReport::confirmations_error`]); the store then refuses
    /// novations until a rebuild succeeds. On an error the map holds what could
    /// be built: nothing when the notes failed to load, every note local-only
    /// when the stored records failed to load.
    pub fn rebuild_confirmations(&mut self) -> Result<usize, NoteError> {
        let result = self.load_confirmations();
        self.load_report.confirmations_error = result
            .as_ref()
            .err()
            .map(|e| format!("confirmation rebuild: {:?}", e));
        result
    }

    /// The work of [`Self::rebuild_confirmations`].
    fn load_confirmations(&mut self) -> Result<usize, NoteError> {
        self.confirmations.clear();

        let notes = match self.storage.get_all_notes_with_issuer() {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Failed to load notes for confirmation rebuild: {:?}", e);
                return Err(e);
            }
        };

        let (stored, load_error) = match self.storage.get_all_confirmations() {
            Ok(stored) => (stored, None),
            Err(e) => {
                tracing::warn!("Failed to load stored confirmation records: {:?}", e);
                (Vec::new(), Some(e))
            }
        };
        let stored_map: std::collections::HashMap<NoteKeyBytes, NoteConfirmation> =
            stored.into_iter().collect();

        for (issuer_pubkey, note) in &notes {
            let key =
                Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
            let mut record = stored_map.get(&key).cloned().unwrap_or_default();

            // Pending state does not survive a restart.
            record.pending_total_debt = None;
            record.pending_tx_id = None;

            let local_value = note.amount_collected;
            record.status = if Some(local_value) == record.confirmed_total_debt {
                NoteConfirmationStatus::Confirmed
            } else {
                NoteConfirmationStatus::LocalOnly
            };

            self.confirmations.insert(key, record);
        }

        if let Some(e) = load_error {
            return Err(e);
        }
        tracing::info!(
            "Rebuilt confirmation records for {} notes",
            self.confirmations.len()
        );
        Ok(self.confirmations.len())
    }

    /// What opening the store found: undecodable note records, failed loading
    /// steps and whether the confirmation records failed to load.
    pub fn load_report(&self) -> &StoreLoadReport {
        &self.load_report
    }

    /// Why debt transfers and set-offs must be refused on this store, if they
    /// must: the confirmation records failed to load, so the store cannot tell
    /// which edges carry on-chain state. Fixed public text
    /// ([`NOVATIONS_DISABLED_CONFIRMATIONS`]); the underlying error is in
    /// [`StoreLoadReport::confirmations_error`] and the log.
    pub fn novations_disabled_reason(&self) -> Option<String> {
        self.load_report
            .confirmations_error
            .as_ref()
            .map(|_| NOVATIONS_DISABLED_CONFIRMATIONS.to_string())
    }

    /// The number of edges whose confirmation record has a committed or pending
    /// on-chain total (the edges a novation may not lower). Always 0 on a
    /// pure-credit tracker.
    pub fn onchain_edge_count(&self) -> usize {
        self.confirmations
            .values()
            .filter(|c| c.confirmed_total_debt.is_some() || c.pending_total_debt.is_some())
            .count()
    }

    /// The pure-credit startup check (design §3.7). A pure-credit tracker
    /// serves only credit edges that were never committed on-chain, so it
    /// refuses to start (`Err`) when:
    /// - any STORED confirmation record has a committed or pending on-chain
    ///   total (`confirmed_total_debt` or `pending_total_debt`; pending totals
    ///   count although a restart clears them in memory, because the
    ///   transaction may still confirm);
    /// - any note record fails to decode (normally such records are skipped);
    /// - the notes or the confirmation records failed to load, when the store
    ///   opened or now.
    /// The note and confirmation partitions are read again here, so the check
    /// does not rely on the in-memory state alone.
    pub fn check_pure_credit(&self) -> Result<(), PureCreditRefusal> {
        let mut refusal = PureCreditRefusal {
            undecodable_note_records: self.load_report.undecodable_note_records,
            load_errors: self.load_report.load_errors.clone(),
            ..Default::default()
        };
        if let Some(e) = &self.load_report.confirmations_error {
            refusal.load_errors.push(e.clone());
        }
        match self.storage.get_all_notes_with_issuer_counted() {
            Ok((_, skipped)) => {
                refusal.undecodable_note_records = refusal.undecodable_note_records.max(skipped)
            }
            Err(e) => refusal.load_errors.push(format!("note records: {:?}", e)),
        }
        match self.storage.get_all_confirmations() {
            Ok(stored) => {
                refusal.onchain_confirmations = stored
                    .iter()
                    .filter(|(_, c)| {
                        c.confirmed_total_debt.is_some() || c.pending_total_debt.is_some()
                    })
                    .count()
            }
            Err(e) => refusal
                .load_errors
                .push(format!("confirmation records: {:?}", e)),
        }
        if refusal == PureCreditRefusal::default() {
            Ok(())
        } else {
            Err(refusal)
        }
    }

    /// Novations need to know which edges carry on-chain state: while the
    /// confirmation records failed to load
    /// ([`StoreLoadReport::confirmations_error`]) no edge can be shown to have
    /// none, so the scope check refuses every edge ([`NoteError::OnchainScope`]).
    fn require_known_onchain_state(&self) -> Result<(), NoteError> {
        match self.load_report.confirmations_error {
            None => Ok(()),
            Some(_) => Err(NoteError::OnchainScope(
                NOVATIONS_DISABLED_CONFIRMATIONS.to_string(),
            )),
        }
    }

    /// Get a clone of the confirmation record for a note, if one exists.
    pub fn get_confirmation(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Option<NoteConfirmation> {
        let key = Self::confirmation_key(issuer_pubkey, recipient_pubkey, denomination);
        self.confirmations.get(&key).cloned()
    }

    /// Get a snapshot of all confirmation records keyed by note key.
    pub fn all_confirmations(&self) -> std::collections::HashMap<NoteKeyBytes, NoteConfirmation> {
        self.confirmations.clone()
    }

    /// Mark every note whose local value differs from its confirmed value as
    /// `Pending`, recording the value that the in-flight update transaction will
    /// commit. Returns the number of notes transitioned to `Pending`.
    pub fn mark_notes_pending(
        &mut self,
        digest: [u8; 33],
        tx_id: &str,
        submitted_height: u64,
    ) -> Result<usize, NoteError> {
        let _ = (digest, submitted_height);
        let notes = self.storage.get_all_notes_with_issuer()?;
        let write_back = self.confirmation_write_back_allowed();
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key =
                Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
            let local_value = note.amount_collected;
            let entry = self
                .confirmations
                .entry(key)
                .or_insert_with(NoteConfirmation::local_only);

            if Some(local_value) != entry.confirmed_total_debt {
                entry.pending_total_debt = Some(local_value);
                entry.pending_tx_id = Some(tx_id.to_string());
                entry.status = NoteConfirmationStatus::Pending;
                if write_back {
                    let _ = self.storage.store_confirmation(&key, entry);
                }
                count += 1;
            }
        }
        if !write_back {
            Self::log_write_back_suspended("mark_notes_pending", count);
        }

        tracing::info!(
            "Marked {} notes as pending for update tx {} (digest {})",
            count,
            tx_id,
            hex::encode(digest)
        );
        Ok(count)
    }

    /// Promote every `Pending` note to `Confirmed`, copying the pending value to
    /// the confirmed value and recording the confirming box metadata. `digest` is
    /// the AVL root committed by the confirming tracker box; it becomes the
    /// committed state that redemption proofs are generated against. Returns the
    /// number of notes transitioned to `Confirmed`.
    pub fn confirm_pending_notes(
        &mut self,
        digest: [u8; 33],
        box_id: &str,
        height: u64,
    ) -> Result<usize, NoteError> {
        self.confirmed_digest = Some(digest);
        let keys: Vec<NoteKeyBytes> = self.confirmations.keys().copied().collect();
        let write_back = self.confirmation_write_back_allowed();
        let mut count = 0usize;

        for key in keys {
            let should_confirm = self
                .confirmations
                .get(&key)
                .map(|c| c.status == NoteConfirmationStatus::Pending)
                .unwrap_or(false);

            if should_confirm {
                if let Some(entry) = self.confirmations.get_mut(&key) {
                    entry.confirmed_total_debt = entry.pending_total_debt;
                    entry.pending_total_debt = None;
                    entry.pending_tx_id = None;
                    entry.confirmed_box_id = Some(box_id.to_string());
                    entry.confirmed_height = Some(height);
                    entry.status = NoteConfirmationStatus::Confirmed;
                    if write_back {
                        let _ = self.storage.store_confirmation(&key, entry);
                    }
                    count += 1;
                }
            }
        }
        if !write_back {
            Self::log_write_back_suspended("confirm_pending_notes", count);
        }

        tracing::info!(
            "Confirmed {} notes in tracker box {} at height {}",
            count,
            box_id,
            height
        );
        Ok(count)
    }

    /// Revert every `Pending` note back to its prior state (used when an update
    /// transaction is dropped or rejected). Clears pending metadata and recomputes
    /// the status from the local value versus the confirmed value. Returns the
    /// number of notes reverted.
    pub fn revert_pending_notes(&mut self) -> Result<usize, NoteError> {
        let notes = self.storage.get_all_notes_with_issuer()?;
        let write_back = self.confirmation_write_back_allowed();
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key =
                Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
            let is_pending = self
                .confirmations
                .get(&key)
                .map(|c| c.status == NoteConfirmationStatus::Pending)
                .unwrap_or(false);

            if is_pending {
                if let Some(entry) = self.confirmations.get_mut(&key) {
                    entry.pending_total_debt = None;
                    entry.pending_tx_id = None;
                    let local_value = note.amount_collected;
                    entry.status = if Some(local_value) == entry.confirmed_total_debt {
                        NoteConfirmationStatus::Confirmed
                    } else {
                        NoteConfirmationStatus::LocalOnly
                    };
                    if write_back {
                        let _ = self.storage.store_confirmation(&key, entry);
                    }
                    count += 1;
                }
            }
        }
        if !write_back {
            Self::log_write_back_suspended("revert_pending_notes", count);
        }

        tracing::info!("Reverted {} pending notes to local state", count);
        Ok(count)
    }

    /// Reconcile confirmation records with an observed on-chain digest. The
    /// observed digest is always recorded as the committed state that redemption
    /// proofs are generated against. When the confirmed digest equals the current
    /// local digest, every note's local value is the confirmed value, so mark them
    /// all as `Confirmed` with the given box metadata. Returns the number of notes
    /// promoted to `Confirmed`.
    pub fn reconcile_with_confirmed_digest(
        &mut self,
        confirmed_digest: &[u8; 33],
        box_id: &str,
        height: u64,
    ) -> Result<usize, NoteError> {
        self.confirmed_digest = Some(*confirmed_digest);

        if confirmed_digest != &self.current_state.avl_root_digest {
            return Ok(0);
        }

        let notes = self.storage.get_all_notes_with_issuer()?;
        let write_back = self.confirmation_write_back_allowed();
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key =
                Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
            let local_value = note.amount_collected;
            let entry = self
                .confirmations
                .entry(key)
                .or_insert_with(NoteConfirmation::local_only);

            if entry.status != NoteConfirmationStatus::Confirmed
                || entry.confirmed_total_debt != Some(local_value)
            {
                entry.confirmed_total_debt = Some(local_value);
                entry.pending_total_debt = None;
                entry.pending_tx_id = None;
                entry.confirmed_box_id = Some(box_id.to_string());
                entry.confirmed_height = Some(height);
                entry.status = NoteConfirmationStatus::Confirmed;
                if write_back {
                    let _ = self.storage.store_confirmation(&key, entry);
                }
                count += 1;
            }
        }
        if !write_back {
            Self::log_write_back_suspended("reconcile_with_confirmed_digest", count);
        }

        if count > 0 {
            tracing::info!(
                "Reconciled {} notes to confirmed digest {} (box {})",
                count,
                hex::encode(confirmed_digest),
                box_id
            );
        }
        Ok(count)
    }

    /// Update an existing note in the tracker state
    /// Updates the AVL tree with hash(issuer||receiver) -> totalDebt mapping
    pub fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        Self::validate_contract_range(note)?;

        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;

        if note.timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }

        // Carry the already-redeemed amount forward from the previous note for this pair.
        //
        // A new note for an existing (issuer, recipient) pair is an increase of the SAME cumulative
        // debt, not a fresh claim. Persisting the incoming note verbatim would reset
        // `amount_redeemed` to 0 and destroy the outstanding-debt / FIFO accounting, so the
        // tracker's view of "how much of this edge is still outstanding" would grow without bound.
        //
        // Only the LOCAL record is adjusted: the AVL value committed on-chain is `amount_collected`,
        // which the contract pins, so the signed message and the on-chain commitment are unchanged.
        // The per-issuer reserve AVL tree (which is what the contract actually reads) is untouched.
        let mut note_to_store = note.clone();

        // Check if there is an existing note with the same issuer-recipient pair
        // and ensure the new timestamp is greater than the existing one (ever increasing).
        // Only `NotFound` means "first note for this pair": a storage or decode
        // error must propagate, not silently skip the timestamp and
        // debt-monotonicity checks below.
        match self.lookup_note(issuer_pubkey, &note.recipient_pubkey, &note.denomination) {
            Ok(existing_note) => {
                if note.timestamp <= existing_note.timestamp {
                    return Err(NoteError::PastTimestamp);
                }
                note_to_store.amount_redeemed = existing_note.amount_redeemed;
                // SECURITY: cumulative debt may never decrease without the creditor's consent.
                //
                // The contract pins every redemption to the tracker tree, so lowering the committed
                // totalDebt for an (issuer, recipient) pair makes every existing note for that pair
                // permanently unredeemable: `trackerDebtCorrect` fails in the normal path AND in the
                // emergency path, because both compare the note's totalDebt against the committed value.
                // A debtor could therefore post totalDebt = 0, wait out the reserve's refund waiting
                // period, and withdraw the collateral while the creditor's claim is silently dead.
                //
                // The debtor's signature proves that the debtor consents, never that the creditor does.
                // Transferring debt to a *different* creditor is unaffected, because that is a different
                // key in the tree; reducing an existing creditor's claim is not something the debtor can
                // do unilaterally.
                if note.amount_collected < existing_note.amount_collected {
                    return Err(NoteError::DebtDecreaseNotPermitted {
                        previous: existing_note.amount_collected,
                        requested: note.amount_collected,
                    });
                }
            }
            Err(NoteError::NotFound) => {}
            Err(e) => return Err(e),
        }

        // Prepare AVL tree key: the denomination-derived debt key
        let key = NoteKey::from_assets(issuer_pubkey, &note.recipient_pubkey, &note.denomination);
        let key_bytes = key.to_bytes();

        // The residue and bound-only rules of `add_note`, after every existing
        // check. This path takes no prior acknowledgment, so it can write
        // neither a bound note (residue 997) nor any note on a bound-only edge.
        if is_reserved_timestamp(note.timestamp) {
            return Err(NoteError::ReservedTimestamp);
        }
        if self.storage.is_bound_only(&key.key_hash)? {
            return Err(NoteError::BoundNoteRequired);
        }

        // Value is just the totalDebt (amount_collected) as 8-byte big-endian
        // This matches the contract spec: hash(A||B) -> totalDebt
        let value_bytes = note.amount_collected.to_be_bytes().to_vec();

        // Persist FIRST, then update the in-memory tree — same ordering and
        // rationale as `add_note` (issue #20 follow-up): a storage failure
        // leaves nothing changed; a tree failure leaves storage safely ahead
        // and sets the dirty flag so the tree is rebuilt before the next
        // digest update.
        self.require_later_timestamp(issuer_pubkey, &note_to_store)?;
        self.storage.store_note(issuer_pubkey, &note_to_store)?;
        if let Err(e) = self.avl_state.update(key_bytes, value_bytes) {
            self.tree_dirty = true;
            tracing::error!(
                "Note update stored but the live tree update failed; the tree is stale and will be rebuilt from storage before the next digest update: {:?}",
                e
            );
            return Err(NoteError::StorageError(e.to_string()));
        }
        self.update_state();
        Ok(())
    }

    /// Atomically move debt between two denominations of the same (issuer, recipient)
    /// pair: decrease the `from` edge by `from_amount` and increase the `to` edge by
    /// `to_amount`. This is the off-chain novation used for cross-denomination
    /// exchange (specs/ergo/multi_collateral_reserves.md §10).
    ///
    /// TWO-PHASE (issue #17, `specs/ergo/exchange_atomicity.md`): only the
    /// from-edge decrease is applied here. The to-edge increase is STAGED in the
    /// `pending_exchanges` partition together with the issuer-signed to-edge note
    /// and the from-denomination reserve box id, and is applied by
    /// [`TrackerStateManager::process_staged_exchanges`] only after the decrease
    /// is confirmed in a tracker box and the reserve is verified unchanged.
    /// Applying both edges at once would let a creditor redeem the old from-edge
    /// total with a pre-built transaction AND collect the to-edge increase.
    ///
    /// SECURITY: decreasing an edge is the S3 operation — it is only permitted here
    /// because the creditor consented, proven by the recipient's signature over the
    /// from-edge's new message. All four signatures (issuer and recipient over each
    /// edge's 80-byte message) are verified BEFORE any state changes.
    ///
    /// ATOMICITY (issue #20): the staged record, the decreased from-edge note and
    /// its confirmation record are committed in one fjall batch — they land
    /// together or not at all, so a storage failure mid-exchange cannot leave the
    /// from-edge reduced without the exchange staged.
    #[allow(clippy::too_many_arguments)]
    pub fn exchange_debt(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        from_denomination: &DenomKey,
        from_amount: u64,
        to_denomination: &DenomKey,
        to_amount: u64,
        timestamp: u64,
        issuer_from_signature: &Signature,
        recipient_from_signature: &Signature,
        issuer_to_signature: &Signature,
        recipient_to_signature: &Signature,
        from_reserve_box_id: &str,
    ) -> Result<(), NoteError> {
        if from_denomination == to_denomination {
            return Err(NoteError::InvalidExchange(
                "from and to denominations must differ".to_string(),
            ));
        }
        if from_amount == 0 || to_amount == 0 {
            return Err(NoteError::InvalidExchange(
                "exchange amounts must be positive".to_string(),
            ));
        }

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;
        if timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }

        // Only one staged exchange may be in flight per (issuer, recipient)
        // pair: a second staged exchange would compute its totals against the
        // pre-stage state of an edge another staged exchange is about to move.
        for (_, existing) in self.storage.get_all_pending_exchanges()? {
            if matches!(existing.status, PendingExchangeStatus::Staged)
                && existing.issuer_pubkey == *issuer_pubkey
                && existing.recipient_pubkey == *recipient_pubkey
            {
                return Err(NoteError::InvalidExchange(format!(
                    "another exchange for this issuer/recipient pair is already staged (from {} to {}); it resolves after the next tracker box commit",
                    existing.from_denomination.denomination_id(),
                    existing.to_denomination.denomination_id()
                )));
            }
        }

        // Resolve both edges.
        let from_note = self.lookup_note(issuer_pubkey, recipient_pubkey, from_denomination)?;
        let to_note_opt =
            self.get_note_optional(issuer_pubkey, recipient_pubkey, to_denomination)?;

        let from_new_total = from_note
            .amount_collected
            .checked_sub(from_amount)
            .ok_or(NoteError::AmountOverflow)?;
        // The edge cannot drop below what was already redeemed from it.
        if from_new_total < from_note.amount_redeemed {
            return Err(NoteError::InvalidExchange(format!(
                "exchange would lower the from-edge total ({}) below its already-redeemed amount ({})",
                from_new_total, from_note.amount_redeemed
            )));
        }
        let to_base_collected = to_note_opt.as_ref().map(|n| n.amount_collected);
        let to_new_total = to_base_collected
            .unwrap_or(0)
            .checked_add(to_amount)
            .ok_or(NoteError::AmountOverflow)?;

        // The exchange must move both edges forward in time.
        if timestamp <= from_note.timestamp
            || to_note_opt
                .as_ref()
                .map(|n| timestamp <= n.timestamp)
                .unwrap_or(false)
        {
            return Err(NoteError::PastTimestamp);
        }

        // The tracker NFT binding must agree across both edges.
        let tracker_nft_id = from_note.tracker_nft_id;
        if let Some(to_note) = &to_note_opt {
            if to_note.tracker_nft_id != tracker_nft_id {
                return Err(NoteError::TrackerIdMismatch);
            }
        }

        // Contract range (B2): ErgoScript reads both amounts with signed
        // byteArrayToLong, so they must fit in i64.
        for (field, value) in [
            ("from_new_total", from_new_total),
            ("to_new_total", to_new_total),
        ] {
            if value > i64::MAX as u64 {
                return Err(NoteError::AmountExceedsContractRange { field, value });
            }
        }

        // Verify all four signatures BEFORE mutating anything.
        let msg_from = schnorr::signing_message(
            issuer_pubkey,
            recipient_pubkey,
            from_denomination,
            from_new_total,
            timestamp,
            &tracker_nft_id,
        );
        let msg_to = schnorr::signing_message(
            issuer_pubkey,
            recipient_pubkey,
            to_denomination,
            to_new_total,
            timestamp,
            &tracker_nft_id,
        );
        let verifier = SchnorrVerifier;
        for (sig, msg, key) in [
            (issuer_from_signature, &msg_from, issuer_pubkey),
            (recipient_from_signature, &msg_from, recipient_pubkey),
            (issuer_to_signature, &msg_to, issuer_pubkey),
            (recipient_to_signature, &msg_to, recipient_pubkey),
        ] {
            if verifier.verify_signature(sig, msg, key).is_err() {
                return Err(NoteError::InvalidSignature);
            }
        }

        // The signed to-edge note. Its `amount_redeemed` is carried forward from
        // the edge's CURRENT state at apply time (phase 2), in case a redemption
        // was recorded while the exchange was staged.
        let to_note_new = IouNote {
            recipient_pubkey: *recipient_pubkey,
            amount_collected: to_new_total,
            amount_redeemed: to_note_opt.as_ref().map(|n| n.amount_redeemed).unwrap_or(0),
            timestamp,
            signature: *issuer_to_signature,
            tracker_nft_id,
            denomination: *to_denomination,
        };

        // Phase 1 commits in ONE atomic storage batch (issue #20): the staged
        // record, the decreased from-edge note and its recomputed confirmation
        // record land together or not at all. If the commit fails, nothing —
        // storage, tree, confirmations — has changed, so no rollback is needed.
        let from_key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, from_denomination);
        let staged = PendingExchangeRecord {
            issuer_pubkey: *issuer_pubkey,
            recipient_pubkey: *recipient_pubkey,
            from_denomination: *from_denomination,
            from_old_total: from_note.amount_collected,
            from_new_total,
            to_denomination: *to_denomination,
            to_note: to_note_new,
            to_base_collected,
            reserve_box_id: from_reserve_box_id.to_string(),
            created_at: current_time,
            status: PendingExchangeStatus::Staged,
        };
        let from_note_new = IouNote {
            recipient_pubkey: *recipient_pubkey,
            amount_collected: from_new_total,
            amount_redeemed: from_note.amount_redeemed,
            timestamp,
            signature: *issuer_from_signature,
            tracker_nft_id,
            denomination: *from_denomination,
        };
        let new_confirmation = self.recomputed_confirmation(&from_key.key_hash, from_new_total);
        // Not written back while the stored confirmation records failed to load
        // (see `confirmation_write_back_allowed`): the stored record stays.
        let write_back = self.confirmation_write_back_allowed();
        self.require_later_timestamp(issuer_pubkey, &from_note_new)?;
        self.storage.commit_exchange_staged(
            issuer_pubkey,
            &staged,
            &from_note_new,
            write_back.then_some(&new_confirmation),
        )?;
        if !write_back {
            Self::log_write_back_suspended("exchange_debt", 1);
        }
        self.confirmations
            .insert(from_key.key_hash, new_confirmation);

        tracing::info!(
            "Exchange staged: from-edge {} decreased {} -> {}; to-edge {} increase to {} applies after the decrease is confirmed (issue #17)",
            from_denomination.denomination_id(),
            from_note.amount_collected,
            from_new_total,
            to_denomination.denomination_id(),
            to_new_total
        );

        // The exchange moves the from-edge's note timestamp forward, which can
        // reorder it relative to other notes in the timestamp-sorted rebuild
        // (`rebuild_avl_tree` after a restart, and the committed-tree rebuild
        // behind lookup proofs). AVL+ digests depend on first-insertion order,
        // so without this the live tree's shape would no longer be reproducible
        // and every subsequent committed lookup proof would fail its digest
        // check until a restart. Recreate the live tree in the canonical
        // (timestamp-sorted) order so the shape stays reproducible; the digest
        // change is committed by the next tracker box update as usual.
        //
        // Storage is already committed and consistent at this point, so a
        // rebuild failure is not fatal: the previous (valid) tree stays in
        // place and the rebuild is retried before the next digest update.
        if let Err(e) = self.rebuild_tree_canonically() {
            tracing::error!(
                "Exchange committed but the live tree rebuild failed; retrying before the next digest update (issue #20): {:?}",
                e
            );
        }
        self.update_state();
        Ok(())
    }

    /// Recreate the live AVL tree from storage in the canonical
    /// timestamp-sorted insertion order, replacing the current tree shape.
    ///
    /// The new tree is built separately and swapped in only on success
    /// (issue #20): on failure the previous, still-valid tree stays in place
    /// and `tree_dirty` is set so `update_state` retries the rebuild before
    /// publishing a digest. A half-rebuilt tree is never committed.
    fn rebuild_tree_canonically(&mut self) -> Result<(), NoteError> {
        #[cfg(test)]
        if self.fail_tree_rebuild {
            self.tree_dirty = true;
            return Err(NoteError::StorageError(
                "injected tree rebuild failure".to_string(),
            ));
        }
        let mut fresh = basis_trees::BasisAvlTree::new().map_err(|e| {
            NoteError::StorageError(format!("Failed to create fresh AVL tree: {:?}", e))
        })?;
        match Self::populate_tree_from_storage(&self.storage, &mut fresh) {
            Ok(()) => {
                self.avl_state = fresh;
                self.tree_dirty = false;
                Ok(())
            }
            Err(e) => {
                self.tree_dirty = true;
                Err(e)
            }
        }
    }

    /// Compute the confirmation record for `key` as if its local value became
    /// `local_value`, without mutating anything. The exchange paths persist
    /// the result inside their atomic storage commit (issue #20) and update
    /// the in-memory map afterwards.
    fn recomputed_confirmation(&self, key: &NoteKeyBytes, local_value: u64) -> NoteConfirmation {
        let mut entry = self
            .confirmations
            .get(key)
            .cloned()
            .unwrap_or_else(NoteConfirmation::local_only);
        entry.status = if Some(local_value) == entry.confirmed_total_debt {
            NoteConfirmationStatus::Confirmed
        } else if Some(local_value) == entry.pending_total_debt {
            NoteConfirmationStatus::Pending
        } else {
            NoteConfirmationStatus::LocalOnly
        };
        entry
    }

    /// Evaluate an exchange's acceptance-policy check against the pre-mutation
    /// state and, on success, perform the exchange — atomically.
    ///
    /// Finding #8: the API layer must not evaluate the acceptance policy itself.
    /// Reading the notes there and mutating here leaves a window in which a
    /// concurrent exchange (or note creation) lands, so two requests can both
    /// pass against the same snapshot and together push the to-edge above the
    /// policy limit. Tracker commands are processed sequentially on one thread,
    /// so evaluating the check here — against the state this very call reads —
    /// and mutating immediately after closes that window.
    ///
    /// The closure carries the predicate and the reserve-collateral snapshot
    /// captured at request time (collateral can still move concurrently via the
    /// scanner, as with every other endpoint); the note state it receives is
    /// authoritative. `Err(reason)` aborts the exchange with
    /// [`NoteError::AcceptancePolicy`] and nothing is mutated.
    #[allow(clippy::too_many_arguments)]
    pub fn exchange_debt_checked(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        from_denomination: &DenomKey,
        from_amount: u64,
        to_denomination: &DenomKey,
        to_amount: u64,
        timestamp: u64,
        issuer_from_signature: &Signature,
        recipient_from_signature: &Signature,
        issuer_to_signature: &Signature,
        recipient_to_signature: &Signature,
        from_reserve_box_id: &str,
        acceptance_check: Option<ExchangeAcceptanceCheckFn>,
    ) -> Result<(), NoteError> {
        if let Some(check) = acceptance_check {
            let view = ExchangeAcceptanceView {
                to_note: self.get_note_optional(
                    issuer_pubkey,
                    recipient_pubkey,
                    to_denomination,
                )?,
                issuer_notes: self.get_issuer_notes(issuer_pubkey)?,
                confirmations: self.all_confirmations(),
            };
            if let Err(reason) = check(view) {
                return Err(NoteError::AcceptancePolicy(reason));
            }
        }
        self.exchange_debt(
            issuer_pubkey,
            recipient_pubkey,
            from_denomination,
            from_amount,
            to_denomination,
            to_amount,
            timestamp,
            issuer_from_signature,
            recipient_from_signature,
            issuer_to_signature,
            recipient_to_signature,
            from_reserve_box_id,
        )
    }

    /// Debt transfer (triad novation, specs/spec.md "Debt Transfer"), version 4
    /// (tracker v3.2): move `amount` of the issuer's cumulative debt from the
    /// edge (issuer -> from_recipient) to the edge (issuer -> to_recipient) in
    /// one denomination. The old creditor is paid by the transfer; the new
    /// creditor is owed by the issuer instead.
    ///
    /// Notation: A issuer, B old creditor (`from_recipient`), C new creditor
    /// (`to_recipient`), a `amount`, T `timestamp`, X and p the stored A->B
    /// total and timestamp, Y and q the stored A->C total and timestamp (0 and 0
    /// when that edge is absent), M(I,R,t,ts) the standard 80-byte note message
    /// and PA(I,R,t,p,T) the 105-byte prior acknowledgment
    /// ([`schnorr::prior_ack_message`], tag `basis/prior-ack/1`).
    ///
    /// SIGNATURES: seven (see [`TransferSignatures`]), all verified BEFORE any
    /// state changes:
    /// - A over M(A,B,X-a,T): the lowered total stays a note the issuer signed,
    ///   so the edge remains redeemable;
    /// - A over M(A,C,Y+a,T): the debtor's consent to owe the new creditor;
    /// - B over PA(A,B,X,p,T), its creditor prior acknowledgment, and over
    ///   M(A,B,X-a,T), the consent the S3 rule requires for lowering an edge.
    ///   The acknowledgment pins the stored state the decrease starts from, so
    ///   B loses exactly a, both values it signed. Without it, if A->B rose and
    ///   A->C fell between signing and submission, whoever holds the signatures
    ///   could move more of B's claim than B agreed to (finding F-A). It binds
    ///   the stored timestamp p as well as X, so it applies at most once: the
    ///   first attempt that applies moves A->B to a later timestamp, and no
    ///   later total of the edge brings (X, p) back (audit VS-1: version 3
    ///   acknowledged M(A,B,X,T), so an attempt could apply again after A->B
    ///   and A->C returned to X and Y);
    /// - B over M(A,C,Y+a,T): binds B's consent to this destination. Without it,
    ///   the issuer and a key it controls could take B's from-edge signatures
    ///   and redirect the debt to themselves;
    /// - C over PA(A,C,Y,q,T), its prior acknowledgment, and over M(A,C,Y+a,T),
    ///   its acceptance: C gains exactly a, so a payer cannot underpay C by
    ///   signing a smaller decrease while a note C does not know of has raised
    ///   A->C.
    /// No note, set-off edge or statement is a PA message (it is never 80 bytes
    /// and starts with its tag), and it binds the edge's state and T, so an
    /// acknowledgment cannot be lifted from another kind of operation or reused
    /// at another T. It does not name the signer's role: B's acknowledgment of
    /// A->B as the new creditor of a transfer INTO A->B at the same T is the
    /// same message, which gives nothing away (paying out of A->B also needs
    /// B's M(A,B,X-a,T) and M(A,C,Y+a,T), and both transfers move A->B to T, so
    /// at most one of them applies). The issuer's total owed never changes: the
    /// decrease on one edge always equals the increase on the other.
    ///
    /// CHECKS, all before anything is written, in this order:
    /// 1. B != C and A is neither of them, and a > 0 (`InvalidTransfer`);
    /// 2. T ends in 999 ms, the transfer residue [`TRANSFER_TS_MS`]
    ///    (`InvalidTransfer`);
    /// 3. T <= now (`FutureTimestamp`);
    /// 4. T >= now - [`NOVATION_MAX_AGE_MS`] (`NovationExpired`);
    /// 5. no exchange is staged on either pair (`StagedExchange`);
    /// 6. the from-edge exists (`NotFound`);
    /// 7. the from-edge has no committed or pending on-chain total
    ///    (`OnchainScope`; also refused while the confirmation records failed
    ///    to load, see [`StoreLoadReport::confirmations_error`]);
    /// 8. `from_prior_total`, `to_prior_total`, `from_prior_timestamp` and
    ///    `to_prior_timestamp`, optional and diagnostic, equal X, Y, p and q
    ///    when they are sent (`StalePrior`);
    /// 9. X >= a and X - a >= the from-edge's redeemed amount
    ///    (`InvalidTransfer`); Y + a does not overflow (`AmountOverflow`);
    /// 10. T is newer than both edges' timestamps (`PastTimestamp`);
    /// 11. both edges are bound to the same tracker NFT (`TrackerIdMismatch`);
    /// 12. both new totals fit the contract's signed 64-bit range
    ///     (`AmountExceedsContractRange`);
    /// 13. all seven signatures verify (`InvalidSignature`; the failing slot is
    ///     logged at warn, never a signature).
    /// [`Self::transfer_debt_checked`] then evaluates the new creditor's
    /// acceptance policy, still before anything is written.
    ///
    /// TIMESTAMP: the transfer residue keeps every transfer signature out of
    /// POST /notes, which refuses it, so the issuer's signature on the raised
    /// edge cannot be submitted on its own as an ordinary note. Each edge's
    /// timestamp strictly increases, so two attempts built on the same state
    /// of A->B exclude each other for good: whichever applies first moves A->B
    /// to its own T, past the other attempt's prior acknowledgment (X, p) and,
    /// if the other is older, past its timestamp. The maximum age bounds how
    /// long unapplied signatures stay usable.
    ///
    /// SCOPE: the from-edge must have no committed or pending on-chain total
    /// (always the case on a pure-credit tracker). Then no tracker box holds an
    /// older, higher total that could still be redeemed, so both edges are
    /// applied at once, and the next tracker box commits the new totals. An
    /// edge with on-chain state is refused: there, as for exchanges (issue
    /// #17), the decrease would have to be committed before the increase
    /// applies, and that two-phase path is not implemented yet.
    ///
    /// ATOMICITY: both notes (A->B stores the `issuer_from` signature, A->C the
    /// `issuer_to` signature), both confirmation records, the index entries of
    /// a new to-edge and the bound-only marks of both edges are committed in one
    /// storage batch. Afterwards new notes on either edge must be bound notes.
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_debt(
        &mut self,
        issuer_pubkey: &PubKey,
        from_recipient_pubkey: &PubKey,
        to_recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        amount: u64,
        timestamp: u64,
        signatures: &TransferSignatures,
        from_prior_total: Option<u64>,
        to_prior_total: Option<u64>,
        from_prior_timestamp: Option<u64>,
        to_prior_timestamp: Option<u64>,
    ) -> Result<TransferTotals, NoteError> {
        let plan = self.check_transfer(
            issuer_pubkey,
            from_recipient_pubkey,
            to_recipient_pubkey,
            denomination,
            amount,
            timestamp,
            signatures,
            from_prior_total,
            to_prior_total,
            from_prior_timestamp,
            to_prior_timestamp,
        )?;
        self.apply_transfer(issuer_pubkey, plan)
    }

    /// [`Self::transfer_debt`] with the new creditor's acceptance-policy check
    /// evaluated against the pre-mutation state, atomically with the mutation
    /// (finding #8, as for exchanges). It runs after every check of
    /// [`Self::transfer_debt`] has passed, signatures included, and before
    /// anything is written. The view's `to_note` is the
    /// (issuer -> to_recipient) edge before the transfer. `Err(reason)` aborts
    /// with [`NoteError::AcceptancePolicy`] and nothing is mutated.
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_debt_checked(
        &mut self,
        issuer_pubkey: &PubKey,
        from_recipient_pubkey: &PubKey,
        to_recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        amount: u64,
        timestamp: u64,
        signatures: &TransferSignatures,
        from_prior_total: Option<u64>,
        to_prior_total: Option<u64>,
        from_prior_timestamp: Option<u64>,
        to_prior_timestamp: Option<u64>,
        acceptance_check: Option<ExchangeAcceptanceCheckFn>,
    ) -> Result<TransferTotals, NoteError> {
        let plan = self.check_transfer(
            issuer_pubkey,
            from_recipient_pubkey,
            to_recipient_pubkey,
            denomination,
            amount,
            timestamp,
            signatures,
            from_prior_total,
            to_prior_total,
            from_prior_timestamp,
            to_prior_timestamp,
        )?;
        if let Some(check) = acceptance_check {
            let view = ExchangeAcceptanceView {
                to_note: self.get_note_optional(issuer_pubkey, to_recipient_pubkey, denomination)?,
                issuer_notes: self.get_issuer_notes(issuer_pubkey)?,
                confirmations: self.all_confirmations(),
            };
            if let Err(reason) = check(view) {
                return Err(NoteError::AcceptancePolicy(reason));
            }
        }
        self.apply_transfer(issuer_pubkey, plan)
    }

    /// Checks 1-13 of [`Self::transfer_debt`], in that order, without changing
    /// anything. Returns the two notes the transfer writes.
    #[allow(clippy::too_many_arguments)]
    fn check_transfer(
        &self,
        issuer_pubkey: &PubKey,
        from_recipient_pubkey: &PubKey,
        to_recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        amount: u64,
        timestamp: u64,
        signatures: &TransferSignatures,
        from_prior_total: Option<u64>,
        to_prior_total: Option<u64>,
        from_prior_timestamp: Option<u64>,
        to_prior_timestamp: Option<u64>,
    ) -> Result<TransferPlan, NoteError> {
        // 1. Three distinct parties, a positive amount.
        if from_recipient_pubkey == to_recipient_pubkey {
            return Err(NoteError::InvalidTransfer(
                "the old and the new creditor must differ".to_string(),
            ));
        }
        if from_recipient_pubkey == issuer_pubkey || to_recipient_pubkey == issuer_pubkey {
            return Err(NoteError::InvalidTransfer(
                "the issuer cannot be its own creditor".to_string(),
            ));
        }
        if amount == 0 {
            return Err(NoteError::InvalidTransfer(
                "the transfer amount must be positive".to_string(),
            ));
        }
        // 2. The transfer residue.
        if !is_transfer_timestamp(timestamp) {
            return Err(NoteError::InvalidTransfer(
                "a transfer's timestamp must end in 999 ms (the novation timestamp domain)".to_string(),
            ));
        }

        // 3, 4. Not in the future, and not older than the maximum novation age.
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;
        if timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }
        if timestamp < current_time.saturating_sub(NOVATION_MAX_AGE_MS) {
            return Err(NoteError::NovationExpired);
        }

        // 5. A staged exchange on either pair would compute its totals against
        // the state this transfer is about to change.
        for (_, existing) in self.storage.get_all_pending_exchanges()? {
            if matches!(existing.status, PendingExchangeStatus::Staged)
                && existing.issuer_pubkey == *issuer_pubkey
                && (existing.recipient_pubkey == *from_recipient_pubkey
                    || existing.recipient_pubkey == *to_recipient_pubkey)
            {
                return Err(NoteError::StagedExchange(
                    "an exchange on one of these issuer/creditor pairs is staged".to_string(),
                ));
            }
        }

        // 6. Both edges (the to-edge may be new).
        let from_note = self.lookup_note(issuer_pubkey, from_recipient_pubkey, denomination)?;
        let to_note_opt =
            self.get_note_optional(issuer_pubkey, to_recipient_pubkey, denomination)?;

        // 7. SCOPE (see `transfer_debt`): the from-edge has never been committed on-chain.
        // Unknown while the confirmation records failed to load: then refused.
        self.require_known_onchain_state()?;
        let from_key_hash =
            NoteKey::from_assets(issuer_pubkey, from_recipient_pubkey, denomination).key_hash;
        if let Some(c) = self.confirmations.get(&from_key_hash) {
            if c.confirmed_total_debt.is_some() || c.pending_total_debt.is_some() {
                return Err(NoteError::OnchainScope(
                    "the old creditor's edge has a committed or pending on-chain total; lowering it needs the two-phase path (the decrease committed before the increase applies), which is not implemented yet".to_string(),
                ));
            }
        }

        // 8. The stored states (total, timestamp) the creditors' prior
        // acknowledgments cover: (0, 0) for an absent to-edge.
        let from_prior = from_note.amount_collected;
        let from_prior_ts = from_note.timestamp;
        let to_prior = to_note_opt.as_ref().map(|n| n.amount_collected).unwrap_or(0);
        let to_prior_ts = to_note_opt.as_ref().map(|n| n.timestamp).unwrap_or(0);
        if from_prior_total.is_some_and(|p| p != from_prior)
            || to_prior_total.is_some_and(|p| p != to_prior)
            || from_prior_timestamp.is_some_and(|p| p != from_prior_ts)
            || to_prior_timestamp.is_some_and(|p| p != to_prior_ts)
        {
            return Err(NoteError::StalePrior);
        }

        // 9. Amounts.
        let from_new_total = from_prior.checked_sub(amount).ok_or_else(|| {
            NoteError::InvalidTransfer(format!(
                "the issuer owes the old creditor {} in this denomination, less than the {} to transfer",
                from_prior, amount
            ))
        })?;
        if from_new_total < from_note.amount_redeemed {
            return Err(NoteError::InvalidTransfer(format!(
                "the transfer would lower the from-edge total ({}) below its already-redeemed amount ({})",
                from_new_total, from_note.amount_redeemed
            )));
        }
        let to_new_total = to_prior.checked_add(amount).ok_or(NoteError::AmountOverflow)?;

        // 10. Both edges move forward in time.
        if timestamp <= from_note.timestamp
            || to_note_opt
                .as_ref()
                .map(|n| timestamp <= n.timestamp)
                .unwrap_or(false)
        {
            return Err(NoteError::PastTimestamp);
        }

        // 11. Both edges are bound to the same tracker.
        let tracker_nft_id = from_note.tracker_nft_id;
        if let Some(to_note) = &to_note_opt {
            if to_note.tracker_nft_id != tracker_nft_id {
                return Err(NoteError::TrackerIdMismatch);
            }
        }

        // 12. Contract range (B2): ErgoScript reads amounts with signed byteArrayToLong.
        for (field, value) in [
            ("from_new_total", from_new_total),
            ("to_new_total", to_new_total),
        ] {
            if value > i64::MAX as u64 {
                return Err(NoteError::AmountExceedsContractRange { field, value });
            }
        }

        // 13. All seven signatures, in the order of `TransferSignatures`: five
        // over the note messages of the new totals at T, the two creditor prior
        // acknowledgments over PA(A,B,X,p,T) and PA(A,C,Y,q,T).
        let message = |recipient: &PubKey, total: u64| {
            schnorr::signing_message(
                issuer_pubkey,
                recipient,
                denomination,
                total,
                timestamp,
                &tracker_nft_id,
            )
        };
        let prior_ack = |recipient: &PubKey, prior_total: u64, prior_timestamp: u64| {
            schnorr::prior_ack_message(
                issuer_pubkey,
                recipient,
                denomination,
                prior_total,
                prior_timestamp,
                timestamp,
                &tracker_nft_id,
            )
        };
        let msg_from_prior = prior_ack(from_recipient_pubkey, from_prior, from_prior_ts);
        let msg_from = message(from_recipient_pubkey, from_new_total);
        let msg_to_prior = prior_ack(to_recipient_pubkey, to_prior, to_prior_ts);
        let msg_to = message(to_recipient_pubkey, to_new_total);
        let verifier = SchnorrVerifier;
        for (slot, sig, msg, key) in [
            ("issuer_from", &signatures.issuer_from, &msg_from, issuer_pubkey),
            ("issuer_to", &signatures.issuer_to, &msg_to, issuer_pubkey),
            (
                "from_recipient_prior",
                &signatures.from_recipient_prior,
                &msg_from_prior,
                from_recipient_pubkey,
            ),
            ("from_recipient", &signatures.from_recipient, &msg_from, from_recipient_pubkey),
            ("from_recipient_to", &signatures.from_recipient_to, &msg_to, from_recipient_pubkey),
            (
                "to_recipient_prior",
                &signatures.to_recipient_prior,
                &msg_to_prior,
                to_recipient_pubkey,
            ),
            ("to_recipient", &signatures.to_recipient, &msg_to, to_recipient_pubkey),
        ] {
            if verifier.verify_signature(sig, msg, key).is_err() {
                // The slot name only: never a signature or a message.
                tracing::warn!("Debt transfer refused: the {} signature does not verify", slot);
                return Err(NoteError::InvalidSignature);
            }
        }

        Ok(TransferPlan {
            from_note: IouNote {
                recipient_pubkey: *from_recipient_pubkey,
                amount_collected: from_new_total,
                amount_redeemed: from_note.amount_redeemed,
                timestamp,
                signature: signatures.issuer_from,
                tracker_nft_id,
                denomination: *denomination,
            },
            to_note: IouNote {
                recipient_pubkey: *to_recipient_pubkey,
                amount_collected: to_new_total,
                amount_redeemed: to_note_opt.as_ref().map(|n| n.amount_redeemed).unwrap_or(0),
                timestamp,
                signature: signatures.issuer_to,
                tracker_nft_id,
                denomination: *denomination,
            },
            totals: TransferTotals {
                from_new_total,
                to_new_total,
                from_prior_total: from_prior,
                to_prior_total: to_prior,
            },
        })
    }

    /// Commit a transfer that passed [`Self::check_transfer`] (see ATOMICITY in
    /// [`Self::transfer_debt`]). On a storage error nothing was written.
    fn apply_transfer(
        &mut self,
        issuer_pubkey: &PubKey,
        plan: TransferPlan,
    ) -> Result<TransferTotals, NoteError> {
        let TransferPlan {
            from_note,
            to_note,
            totals,
        } = plan;
        let from_key =
            NoteKey::from_assets(issuer_pubkey, &from_note.recipient_pubkey, &from_note.denomination);
        let to_key =
            NoteKey::from_assets(issuer_pubkey, &to_note.recipient_pubkey, &to_note.denomination);
        let from_confirmation =
            self.recomputed_confirmation(&from_key.key_hash, totals.from_new_total);
        let to_confirmation = self.recomputed_confirmation(&to_key.key_hash, totals.to_new_total);
        self.require_later_timestamp(issuer_pubkey, &from_note)?;
        self.require_later_timestamp(issuer_pubkey, &to_note)?;
        self.storage.commit_note_batch(
            &[
                (*issuer_pubkey, from_note, from_confirmation.clone()),
                (*issuer_pubkey, to_note, to_confirmation.clone()),
            ],
            &[from_key.key_hash, to_key.key_hash],
        )?;
        self.confirmations.insert(from_key.key_hash, from_confirmation);
        self.confirmations.insert(to_key.key_hash, to_confirmation);

        tracing::info!(
            "Debt transferred: {} of the issuer's debt moved from one creditor (total {} -> {}) to another (total {} -> {})",
            totals.from_prior_total - totals.from_new_total,
            totals.from_prior_total,
            totals.from_new_total,
            totals.to_prior_total,
            totals.to_new_total
        );

        // Both edges' timestamps moved: keep the live tree in the canonical
        // (timestamp-sorted) insertion order, as after an exchange.
        if let Err(e) = self.rebuild_tree_canonically() {
            tracing::error!(
                "Transfer committed but the live tree rebuild failed; retrying before the next digest update: {:?}",
                e
            );
        }
        self.update_state();
        Ok(totals)
    }

    /// Multilateral set-off (obligation clearing), version 3: lower several
    /// edges at once so that every participant's net position is unchanged
    /// while gross debt falls. Typical use: a cycle (A owes B, B owes C, C owes
    /// A) cancelled by lowering each edge by the same amount; any circulation
    /// of decreases is accepted.
    ///
    /// SIGNATURES: two per edge, both over the standard 80-byte note message
    /// M(I,R,P-d,ts_e) at the edge's NEW total (its stored total P minus its
    /// decrease d) and the edge's own timestamp ts_e (see [`SetOffEdge`]): the
    /// issuer (so the lowered total is still a note the issuer signed,
    /// redeemable as before) and the creditor (the consent the S3 rule requires
    /// for lowering an edge). There are no prior acknowledgments: balance
    /// already makes every accepted set-off net-neutral for every key, and the
    /// signatures pin each edge's final total.
    ///
    /// BALANCE (flow conservation): for every key, the decreases on edges where it
    /// is the creditor sum exactly to the decreases on edges where it is the
    /// issuer. This is what makes consent safe to automate: any set-off this
    /// tracker accepts is net-neutral for every participant, so a participant's
    /// signatures can only ever be used in combinations that leave its net
    /// position unchanged.
    ///
    /// PER-EDGE TIMESTAMPS: each edge carries its own timestamp, so an edge that
    /// moved after the set-off was signed is re-signed on its own by its two
    /// parties while the other edges keep their signatures. Every edge's
    /// timestamp ends in 998 ms, the set-off residue [`SETOFF_TS_MS`], which
    /// POST /notes and transfers refuse: set-off signatures are never usable in
    /// another operation, and no other operation's signatures in a set-off.
    /// Each edge's timestamp strictly increases, so a set-off cannot be
    /// replayed, and the maximum age bounds how long unapplied signatures stay
    /// usable.
    ///
    /// CHECKS, all before anything is written, in this order:
    /// 1. 2 to [`MAX_SETOFF_EDGES`] edges (`InvalidSetOff`);
    /// 2. edge by edge, its timestamp: ends in 998 ms (`InvalidSetOff`), is at
    ///    most now (`FutureTimestamp`) and at least now - [`NOVATION_MAX_AGE_MS`]
    ///    (`NovationExpired`);
    /// 3. no self-edges, positive decreases, distinct (issuer, creditor) pairs
    ///    (`InvalidSetOff`);
    /// 4. balance for every key (`InvalidSetOff`);
    /// 5. no exchange is staged on any of the pairs (`StagedExchange`);
    /// 6. edge by edge: the edge exists (`NotFound`); it has no committed or
    ///    pending on-chain total (`OnchainScope`; also refused while the
    ///    confirmation records failed to load); the decrease is at most its
    ///    total and leaves at least its redeemed amount (`InvalidSetOff`); its
    ///    timestamp is newer than the edge's stored one (`PastTimestamp`); it
    ///    is bound to the same tracker NFT as the other edges
    ///    (`TrackerIdMismatch`); both signatures verify (`InvalidSignature`; the
    ///    failing edge and slot are logged at warn, never a signature).
    ///
    /// SCOPE and ATOMICITY as for [`Self::transfer_debt`]: an edge with a
    /// committed or pending on-chain total is refused (the two-phase path is
    /// future work); all notes (each with its issuer's signature, at the edge's
    /// timestamp), confirmation records and the bound-only marks of every edge
    /// land in one storage batch. Afterwards new notes on any of the edges must
    /// be bound notes.
    pub fn set_off(
        &mut self,
        denomination: &DenomKey,
        edges: &[SetOffEdge],
    ) -> Result<SetOffResult, NoteError> {
        // 1. Size.
        if edges.len() < 2 {
            return Err(NoteError::InvalidSetOff(
                "a set-off lowers at least two edges".to_string(),
            ));
        }
        if edges.len() > MAX_SETOFF_EDGES {
            return Err(NoteError::InvalidSetOff(format!(
                "a set-off lowers at most {} edges",
                MAX_SETOFF_EDGES
            )));
        }

        // 2. Every edge's own timestamp: the set-off residue, not in the future,
        // not older than the maximum novation age.
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;
        for e in edges {
            if !is_setoff_timestamp(e.timestamp) {
                return Err(NoteError::InvalidSetOff(
                    "every edge's timestamp must end in 998 ms (the set-off timestamp residue)".to_string(),
                ));
            }
            if e.timestamp > current_time {
                return Err(NoteError::FutureTimestamp);
            }
            if e.timestamp < current_time.saturating_sub(NOVATION_MAX_AGE_MS) {
                return Err(NoteError::NovationExpired);
            }
        }

        // 3. Distinct edges, no self-edges, positive decreases.
        let mut seen = std::collections::HashSet::new();
        for e in edges {
            if e.issuer_pubkey == e.recipient_pubkey {
                return Err(NoteError::InvalidSetOff(
                    "an edge's issuer and creditor must differ".to_string(),
                ));
            }
            if e.decrease == 0 {
                return Err(NoteError::InvalidSetOff(
                    "every decrease must be positive".to_string(),
                ));
            }
            if !seen.insert((e.issuer_pubkey, e.recipient_pubkey)) {
                return Err(NoteError::InvalidSetOff(
                    "an edge appears more than once".to_string(),
                ));
            }
        }

        // 4. Balance: for every key, decreases as creditor == decreases as issuer.
        let mut as_creditor: std::collections::HashMap<PubKey, u128> = std::collections::HashMap::new();
        let mut as_issuer: std::collections::HashMap<PubKey, u128> = std::collections::HashMap::new();
        for e in edges {
            *as_creditor.entry(e.recipient_pubkey).or_insert(0) += e.decrease as u128;
            *as_issuer.entry(e.issuer_pubkey).or_insert(0) += e.decrease as u128;
        }
        for key in as_creditor.keys().chain(as_issuer.keys()) {
            let inn = as_creditor.get(key).copied().unwrap_or(0);
            let out = as_issuer.get(key).copied().unwrap_or(0);
            if inn != out {
                return Err(NoteError::InvalidSetOff(format!(
                    "the decreases do not balance for one participant: {} as creditor, {} as issuer",
                    inn, out
                )));
            }
        }

        // 5. A staged exchange on any of these pairs would compute its totals
        // against the state this set-off is about to change.
        for (_, existing) in self.storage.get_all_pending_exchanges()? {
            if matches!(existing.status, PendingExchangeStatus::Staged)
                && edges.iter().any(|e| {
                    e.issuer_pubkey == existing.issuer_pubkey
                        && e.recipient_pubkey == existing.recipient_pubkey
                })
            {
                return Err(NoteError::StagedExchange(
                    "an exchange on one of these pairs is staged".to_string(),
                ));
            }
        }

        // 6. Per edge: current note, scope, new total, time, tracker binding and
        // both signatures, all BEFORE any state changes.
        let verifier = SchnorrVerifier;
        let mut tracker_nft_id: Option<[u8; 32]> = None;
        let mut entries: Vec<(PubKey, IouNote, NoteConfirmation)> = Vec::with_capacity(edges.len());
        let mut keys: Vec<NoteKeyBytes> = Vec::with_capacity(edges.len());
        let mut new_totals = Vec::with_capacity(edges.len());
        for (index, e) in edges.iter().enumerate() {
            let note = self.lookup_note(&e.issuer_pubkey, &e.recipient_pubkey, denomination)?;
            let key = NoteKey::from_assets(&e.issuer_pubkey, &e.recipient_pubkey, denomination);
            // Unknown while the confirmation records failed to load: then refused.
            self.require_known_onchain_state()?;
            if let Some(c) = self.confirmations.get(&key.key_hash) {
                if c.confirmed_total_debt.is_some() || c.pending_total_debt.is_some() {
                    return Err(NoteError::OnchainScope(
                        "an edge has a committed or pending on-chain total; lowering it needs the two-phase path, which is not implemented yet".to_string(),
                    ));
                }
            }
            let new_total = note.amount_collected.checked_sub(e.decrease).ok_or_else(|| {
                NoteError::InvalidSetOff(format!(
                    "a decrease of {} exceeds the edge's total of {}",
                    e.decrease, note.amount_collected
                ))
            })?;
            if new_total < note.amount_redeemed {
                return Err(NoteError::InvalidSetOff(format!(
                    "a decrease would lower an edge's total ({}) below its already-redeemed amount ({})",
                    new_total, note.amount_redeemed
                )));
            }
            if e.timestamp <= note.timestamp {
                return Err(NoteError::PastTimestamp);
            }
            match tracker_nft_id {
                None => tracker_nft_id = Some(note.tracker_nft_id),
                Some(id) if id != note.tracker_nft_id => return Err(NoteError::TrackerIdMismatch),
                _ => {}
            }
            let msg = schnorr::signing_message(
                &e.issuer_pubkey,
                &e.recipient_pubkey,
                denomination,
                new_total,
                e.timestamp,
                &note.tracker_nft_id,
            );
            for (slot, sig, signer) in [
                ("issuer", &e.issuer_signature, &e.issuer_pubkey),
                ("recipient", &e.recipient_signature, &e.recipient_pubkey),
            ] {
                if verifier.verify_signature(sig, &msg, signer).is_err() {
                    // The edge's index and the slot name only: never a signature or a message.
                    tracing::warn!(
                        "Set-off refused: the {} signature of edge {} does not verify",
                        slot,
                        index
                    );
                    return Err(NoteError::InvalidSignature);
                }
            }
            let new_note = IouNote {
                recipient_pubkey: e.recipient_pubkey,
                amount_collected: new_total,
                amount_redeemed: note.amount_redeemed,
                timestamp: e.timestamp,
                signature: e.issuer_signature,
                tracker_nft_id: note.tracker_nft_id,
                denomination: *denomination,
            };
            let confirmation = self.recomputed_confirmation(&key.key_hash, new_total);
            entries.push((e.issuer_pubkey, new_note, confirmation));
            keys.push(key.key_hash);
            new_totals.push(SetOffEdgeTotal {
                issuer_pubkey: e.issuer_pubkey,
                recipient_pubkey: e.recipient_pubkey,
                new_total,
                timestamp: e.timestamp,
            });
        }

        for (issuer_pubkey, note, _) in &entries {
            self.require_later_timestamp(issuer_pubkey, note)?;
        }
        self.storage.commit_note_batch(&entries, &keys)?;
        for (key, (_, _, confirmation)) in keys.iter().zip(entries.into_iter()) {
            self.confirmations.insert(*key, confirmation);
        }
        let total_cleared: u128 = edges.iter().map(|e| e.decrease as u128).sum();
        let total_cleared = u64::try_from(total_cleared).unwrap_or(u64::MAX);
        tracing::info!(
            "Set-off applied: {} edges lowered, {} of gross debt cleared",
            edges.len(),
            total_cleared
        );
        if let Err(e) = self.rebuild_tree_canonically() {
            tracing::error!(
                "Set-off committed but the live tree rebuild failed; retrying before the next digest update: {:?}",
                e
            );
        }
        self.update_state();
        Ok(SetOffResult {
            new_totals,
            total_cleared,
        })
    }

    /// List staged exchanges for the tracker box updater's phase-2 pass
    /// (issue #17). The updater observes each record's reserve box on-chain and
    /// feeds the observations back through [`Self::process_staged_exchanges`].
    pub fn list_staged_exchanges(&self) -> Result<Vec<StagedExchangeInfo>, NoteError> {
        let mut out = Vec::new();
        for (key, record) in self.storage.get_all_pending_exchanges()? {
            if matches!(record.status, PendingExchangeStatus::Staged) {
                out.push(StagedExchangeInfo {
                    key,
                    reserve_box_id: record.reserve_box_id,
                });
            }
        }
        Ok(out)
    }

    /// Phase 2 of a two-phase exchange (issue #17): apply or cancel staged
    /// to-edge increases using the on-chain observations gathered by the
    /// tracker box updater.
    ///
    /// A staged exchange is applied only when all of the following hold:
    /// 1. The from-edge decrease is exactly what got committed
    ///    (`Confirmed` with `confirmed_total_debt == from_new_total`), so the
    ///    old from-edge note (signed for `from_old_total`) is dead on-chain.
    /// 2. The observed reserve box is still unspent. Any redemption, top-up, or
    ///    refund completion spends the reserve box, so a missing box is
    ///    conclusive evidence of an on-chain reserve action.
    /// 3. The observed reserve R5 digest equals the tracker's reserve-tree
    ///    digest for `(issuer, from_denomination)` — reported redemptions
    ///    advance both sides in lockstep, so a mismatch means an unreported
    ///    on-chain redemption drained the reserve during staging.
    /// 4. The to-edge base is unchanged, so applying the staged note cannot
    ///    lower an edge that grew meanwhile, and the to-edge's stored timestamp
    ///    is still earlier than the staged note's, so applying it cannot move
    ///    the edge's timestamp back (an edge's timestamp strictly increases with
    ///    every write; a note written on the to-edge at the same total while the
    ///    exchange was staged, at a later timestamp, cancels it). The applied
    ///    note carries the to-edge's current `amount_redeemed` forward.
    /// 5. The staged to-note's signature re-verifies.
    ///
    /// Otherwise the exchange is cancelled with the reason persisted on the
    /// record (kept for observability; the from-edge decrease stays applied).
    /// Records without an observation are left staged (wait — the updater could
    /// not observe their reserve this cycle, e.g. a node error).
    pub fn process_staged_exchanges(
        &mut self,
        observations: &[ExchangeObservation],
    ) -> Result<ProcessedExchanges, NoteError> {
        let mut result = ProcessedExchanges::default();
        let staged = self.storage.get_all_pending_exchanges()?;

        for (key, mut record) in staged {
            if !matches!(record.status, PendingExchangeStatus::Staged) {
                continue;
            }
            let observation = observations.iter().find(|o| o.key == key);
            let decision = self.decide_staged_exchange(&key, &record, observation)?;
            match decision {
                StagedExchangeDecision::Wait => continue,
                StagedExchangeDecision::Apply => {
                    self.apply_staged_exchange(&key, &record)?;
                    result.applied += 1;
                }
                StagedExchangeDecision::Cancel(reason) => {
                    record.status = PendingExchangeStatus::Cancelled {
                        reason: reason.clone(),
                    };
                    self.storage.store_pending_exchange(&key, &record)?;
                    result.cancelled += 1;
                    tracing::error!(
                        "Staged exchange ({} -> {}) cancelled: {}",
                        record.from_denomination.denomination_id(),
                        record.to_denomination.denomination_id(),
                        reason
                    );
                }
            }
        }

        if result.applied > 0 || result.cancelled > 0 {
            self.update_state();
        }
        Ok(result)
    }

    /// Evaluate one staged exchange against the current confirmation state and
    /// the updater's on-chain observation.
    fn decide_staged_exchange(
        &self,
        key: &[u8; 32],
        record: &PendingExchangeRecord,
        observation: Option<&ExchangeObservation>,
    ) -> Result<StagedExchangeDecision, NoteError> {
        // 1. The from-edge decrease must be exactly what got committed.
        let confirmation = self.confirmations.get(key);
        let confirmed = match confirmation {
            Some(c) if c.status == NoteConfirmationStatus::Confirmed => c.confirmed_total_debt,
            _ => return Ok(StagedExchangeDecision::Wait),
        };
        match confirmed {
            Some(value) if value == record.from_new_total => {}
            Some(value) => {
                return Ok(StagedExchangeDecision::Cancel(format!(
                    "from-edge committed total is {} but the exchange decreased it to {}; the tracker state moved past the exchange",
                    value, record.from_new_total
                )))
            }
            None => return Ok(StagedExchangeDecision::Wait),
        }

        // 2./3. The reserve must still be unspent and its R5 digest must match
        // the tracker's reserve-tree digest for (issuer, from_denomination).
        let Some(observation) = observation else {
            return Ok(StagedExchangeDecision::Wait);
        };
        let Some(observed_digest) = observation.reserve_r5_digest else {
            return Ok(StagedExchangeDecision::Cancel(format!(
                "from-denomination reserve box {} is no longer unspent; an on-chain reserve action (redemption, top-up, or refund) occurred while the exchange was staged",
                record.reserve_box_id
            )));
        };
        let expected_digest =
            self.reserve_state_digest(&record.issuer_pubkey, &record.from_denomination);
        if observed_digest.to_vec() != expected_digest {
            return Ok(StagedExchangeDecision::Cancel(format!(
                "on-chain reserve R5 digest {} does not match the tracker's reserve-tree digest {}; an unreported on-chain redemption may have occurred while the exchange was staged",
                hex::encode(observed_digest),
                hex::encode(expected_digest)
            )));
        }

        // 4. The to-edge base must be unchanged; carry the current redeemed
        // amount forward.
        let current_to = self.get_note_optional(
            &record.issuer_pubkey,
            &record.recipient_pubkey,
            &record.to_denomination,
        )?;
        let base_unchanged = match (&record.to_base_collected, &current_to) {
            (None, None) => true,
            (Some(base), Some(note)) => note.amount_collected == *base,
            _ => false,
        };
        if !base_unchanged {
            return Ok(StagedExchangeDecision::Cancel(
                "the to-edge changed while the exchange was staged (a new note was issued on it); the staged increase no longer matches the agreed state".to_string(),
            ));
        }
        // The staged note must still be later than the to-edge's stored note:
        // a note written on the to-edge at the same total, at a timestamp at or
        // after the exchange's, would otherwise be followed by an earlier
        // timestamp, and the edge's state (total, timestamp) could recur.
        if let Some(note) = &current_to {
            if note.timestamp >= record.to_note.timestamp {
                return Ok(StagedExchangeDecision::Cancel(format!(
                    "the to-edge was written at timestamp {} while the exchange was staged, not earlier than the exchange's timestamp {}; applying the staged increase would move the edge's timestamp back",
                    note.timestamp, record.to_note.timestamp
                )));
            }
        }

        // 5. Re-verify the staged to-note's issuer signature.
        let msg_to = schnorr::signing_message(
            &record.issuer_pubkey,
            &record.recipient_pubkey,
            &record.to_denomination,
            record.to_note.amount_collected,
            record.to_note.timestamp,
            &record.to_note.tracker_nft_id,
        );
        let verifier = SchnorrVerifier;
        if verifier
            .verify_signature(&record.to_note.signature, &msg_to, &record.issuer_pubkey)
            .is_err()
        {
            return Ok(StagedExchangeDecision::Cancel(
                "staged to-edge note signature failed re-verification".to_string(),
            ));
        }

        Ok(StagedExchangeDecision::Apply)
    }

    /// Apply a staged exchange's to-edge increase (issue #17 phase 2).
    ///
    /// ATOMICITY (issue #20): the to-edge note, its index entries, its
    /// confirmation record and the staged record's removal commit in one fjall
    /// batch. On failure nothing is written, so the exchange stays staged and
    /// the apply is retried on the next confirmation cycle (previously a failed
    /// staged-record delete led to a spurious cancellation of an applied
    /// exchange).
    fn apply_staged_exchange(
        &mut self,
        key: &[u8; 32],
        record: &PendingExchangeRecord,
    ) -> Result<(), NoteError> {
        let current_to = self.get_note_optional(
            &record.issuer_pubkey,
            &record.recipient_pubkey,
            &record.to_denomination,
        )?;
        let mut to_note = record.to_note.clone();
        to_note.amount_redeemed = current_to.as_ref().map(|n| n.amount_redeemed).unwrap_or(0);

        let to_key = NoteKey::from_assets(
            &record.issuer_pubkey,
            &record.recipient_pubkey,
            &record.to_denomination,
        );
        let new_confirmation =
            self.recomputed_confirmation(&to_key.key_hash, to_note.amount_collected);
        // Not written back while the stored confirmation records failed to load
        // (see `confirmation_write_back_allowed`): the stored record stays.
        let write_back = self.confirmation_write_back_allowed();
        self.require_later_timestamp(&record.issuer_pubkey, &to_note)?;
        self.storage.commit_exchange_apply(
            &record.issuer_pubkey,
            &to_note,
            write_back.then_some(&new_confirmation),
            key,
        )?;
        if !write_back {
            Self::log_write_back_suspended("apply_staged_exchange", 1);
        }
        self.confirmations.insert(to_key.key_hash, new_confirmation);

        tracing::info!(
            "Staged exchange applied: to-edge {} increased to {} after the from-edge decrease was confirmed (issue #17)",
            record.to_denomination.denomination_id(),
            to_note.amount_collected
        );

        // Same canonical rebuild as `exchange_debt`: the to-edge note's
        // timestamp moved forward, so recreate the live tree in the order every
        // rebuild uses to keep its shape reproducible (see the comment there).
        // Storage is already consistent; a rebuild failure keeps the previous
        // valid tree and is retried before the next digest update (issue #20).
        if let Err(e) = self.rebuild_tree_canonically() {
            tracing::error!(
                "Staged exchange applied but the live tree rebuild failed; retrying before the next digest update (issue #20): {:?}",
                e
            );
        }
        Ok(())
    }

    /// List pending (staged or cancelled) exchanges, optionally filtered by
    /// issuer and/or recipient, for the `/exchanges/pending` endpoint.
    ///
    /// Cancelled records older than 30 days are pruned on access; applied
    /// records are deleted at apply time (the to-edge note's existence is the
    /// completion signal).
    pub fn list_pending_exchanges(
        &self,
        issuer: Option<&PubKey>,
        recipient: Option<&PubKey>,
    ) -> Result<Vec<([u8; 32], PendingExchangeRecord)>, NoteError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        const CANCELLED_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;

        let mut out = Vec::new();
        for (key, record) in self.storage.get_all_pending_exchanges()? {
            if let Some(issuer) = issuer {
                if record.issuer_pubkey != *issuer {
                    continue;
                }
            }
            if let Some(recipient) = recipient {
                if record.recipient_pubkey != *recipient {
                    continue;
                }
            }
            if let PendingExchangeStatus::Cancelled { .. } = record.status {
                if now_ms.saturating_sub(record.created_at) > CANCELLED_RETENTION_MS {
                    let _ = self.storage.delete_pending_exchange(&key);
                    continue;
                }
            }
            out.push((key, record));
        }
        Ok(out)
    }

    /// Record that `amount_redeemed` was redeemed against an existing note.
    ///
    /// This is redemption accounting, NOT a debt update, so it deliberately bypasses
    /// `update_note`'s requirements:
    ///
    /// * `update_note` demands a strictly newer timestamp and a valid issuer signature, because it
    ///   represents a new signed note that changes the committed cumulative debt. A redemption
    ///   changes neither the debt nor the signature.
    /// * The timestamp is part of the signed message, so refreshing it here invalidated the stored
    ///   issuer signature and left the creditor's note unredeemable. It is now preserved exactly.
    /// * `amount_collected` -- the value committed in the AVL tree and checked by the contract --
    ///   is untouched, so no AVL write is needed.
    ///
    /// The cumulative redeemed amount may only grow; going backwards would let a caller reset a
    /// note's headroom and redeem the same debt again.
    pub fn record_redemption(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        new_amount_redeemed: u64,
    ) -> Result<(), NoteError> {
        let mut note = self
            .lookup_note(issuer_pubkey, recipient_pubkey, denomination)
            .map_err(|_| NoteError::StorageError("note not found".to_string()))?;

        if new_amount_redeemed < note.amount_redeemed {
            return Err(NoteError::StorageError(format!(
                "cumulative redeemed amount cannot decrease (stored {}, requested {})",
                note.amount_redeemed, new_amount_redeemed
            )));
        }
        // Redeeming more than was ever recorded as owed would leave the note permanently unusable.
        if new_amount_redeemed > note.amount_collected {
            return Err(NoteError::AmountOverflow);
        }

        if new_amount_redeemed == note.amount_redeemed {
            return Ok(());
        }

        note.amount_redeemed = new_amount_redeemed;
        self.storage.store_note(issuer_pubkey, &note)
    }

    /// Get the total debt for a specific (issuer, receiver, denomination) triple
    /// from the AVL tree.
    /// Returns the cumulative debt amount (totalDebt) stored in the tracker's AVL tree
    pub fn get_total_debt(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<u64, NoteError> {
        let key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, denomination);
        let key_bytes = key.to_bytes();

        // Lookup value in AVL tree
        let value_bytes = self.avl_state.get(&key_bytes).ok_or_else(|| {
            NoteError::StorageError("Debt record not found in AVL tree".to_string())
        })?;

        // Convert 8-byte big-endian to u64
        if value_bytes.len() != 8 {
            return Err(NoteError::StorageError(
                "Invalid debt value format in AVL tree".to_string(),
            ));
        }

        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&value_bytes);
        Ok(u64::from_be_bytes(bytes))
    }

    /// Generate a tracker lookup proof for context var #8
    /// This proof verifies that totalDebt exists in the tracker's AVL tree.
    ///
    /// NOTE: this proof is generated against the LIVE local tree, which may
    /// contain notes not yet committed on-chain. It is suitable for informational
    /// endpoints (`GET /tracker/proof`) but NOT for redemption transactions:
    /// redemptions must use `generate_committed_tracker_lookup_proof`, which
    /// proves against the latest confirmed on-chain digest.
    pub fn generate_tracker_lookup_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<TrackerLookupProof, NoteError> {
        let key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, denomination);
        let key_bytes = key.to_bytes();

        // Get the total debt value
        let total_debt = self.get_total_debt(issuer_pubkey, recipient_pubkey, denomination)?;

        // Generate AVL proof for the lookup of this specific key
        let (avl_proof, _returned_value) = self.avl_state.generate_lookup_proof(key_bytes.to_vec());

        Ok(TrackerLookupProof {
            key: key_bytes,
            value: total_debt.to_be_bytes().to_vec(),
            proof: avl_proof,
        })
    }

    /// The AVL root digest of the latest confirmed on-chain tracker box, if known.
    pub fn confirmed_digest(&self) -> Option<[u8; 33]> {
        self.confirmed_digest
    }

    /// Generate a tracker lookup proof for context var #8 against the COMMITTED
    /// on-chain state (the latest confirmed tracker box's R5), not the live tree.
    ///
    /// Redemption transactions must use this: the proof is verified on-chain
    /// against the tracker box's committed digest, so any note posted after the
    /// last commitment must not alter the proof's starting digest (issue #15 S6).
    ///
    /// The committed tree is rebuilt from the per-note `confirmed_total_debt`
    /// values in chronological order (same ordering as `rebuild_avl_tree`), and
    /// the rebuilt digest is asserted to equal the recorded confirmed digest.
    ///
    /// Returns `NoteError::NoteNotConfirmed` when the note has no confirmed value
    /// (i.e. it is not committed on-chain yet) and `NoteError::StorageError` when
    /// no confirmed on-chain digest is known or the rebuilt tree does not
    /// reproduce it.
    pub fn generate_committed_tracker_lookup_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<TrackerLookupProof, NoteError> {
        let key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, denomination);
        let key_bytes = key.to_bytes();

        // The note being redeemed must be committed on-chain with a known value.
        let confirmation = self.get_confirmation(issuer_pubkey, recipient_pubkey, denomination);

        // Defense in depth for issue #17: a committed total GREATER than the
        // note's current local total means a consented debt decrease (exchange)
        // has not been committed yet. A redemption built against the committed
        // value would redeem more than the issuer currently owes on the edge,
        // which is exactly the double-pay the two-phase exchange prevents.
        // Checked before the status filter: right after an exchange the note's
        // status is `LocalOnly` (local != confirmed) while the stale committed
        // value is still what the chain would accept.
        if let Some(stale_committed) = confirmation
            .as_ref()
            .and_then(|c| c.confirmed_total_debt)
            .filter(|committed| {
                self.get_note_optional(issuer_pubkey, recipient_pubkey, denomination)
                    .map(|note| note.is_some_and(|n| *committed > n.amount_collected))
                    .unwrap_or(false)
            })
        {
            let local = self
                .get_note_optional(issuer_pubkey, recipient_pubkey, denomination)
                .ok()
                .flatten()
                .map(|n| n.amount_collected)
                .unwrap_or_default();
            tracing::warn!(
                "Committed proof requested for note {} -> {} with stale committed total {} > local {} (uncommitted exchange decrease)",
                hex::encode(issuer_pubkey),
                hex::encode(recipient_pubkey),
                stale_committed,
                local
            );
            return Err(NoteError::StaleCommittedDebt {
                committed: stale_committed,
                local,
            });
        }

        let confirmed_total_debt = confirmation
            .and_then(|c| {
                if c.status == NoteConfirmationStatus::Confirmed {
                    c.confirmed_total_debt
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                tracing::warn!(
                    "Committed proof requested for unconfirmed note {} -> {}",
                    hex::encode(issuer_pubkey),
                    hex::encode(recipient_pubkey)
                );
                NoteError::NoteNotConfirmed
            })?;

        let confirmed_digest = self.confirmed_digest.ok_or_else(|| {
            NoteError::StorageError(
                "No confirmed on-chain tracker digest known yet; wait for the tracker box updater"
                    .to_string(),
            )
        })?;

        // Rebuild the committed tree from every note's confirmed value, in
        // chronological order by note timestamp (same ordering as
        // `rebuild_avl_tree`). Notes without a confirmed value are not part of
        // the committed state.
        let mut notes_with_issuer = self
            .storage
            .get_all_notes_with_issuer()
            .map_err(|e| NoteError::StorageError(format!("Failed to get all notes: {:?}", e)))?;
        notes_with_issuer.sort_by_key(|(_, note)| note.timestamp);

        let mut committed_tree = basis_trees::BasisAvlTree::new().map_err(|e| {
            NoteError::StorageError(format!("Failed to create committed AVL tree: {:?}", e))
        })?;
        for (issuer, note) in &notes_with_issuer {
            let note_key =
                Self::confirmation_key(issuer, &note.recipient_pubkey, &note.denomination);
            let confirmed_value = self
                .confirmations
                .get(&note_key)
                .and_then(|c| c.confirmed_total_debt);
            if let Some(value) = confirmed_value {
                committed_tree
                    .update(note_key.to_vec(), value.to_be_bytes().to_vec())
                    .map_err(|e| {
                        NoteError::StorageError(format!(
                            "Committed AVL tree rebuild failed: {:?}",
                            e
                        ))
                    })?;
            }
        }

        // The rebuilt committed tree must reproduce the confirmed on-chain digest;
        // otherwise local confirmation metadata is inconsistent with the chain and
        // generating a proof would produce something that fails on-chain anyway.
        let rebuilt_digest = committed_tree.root_digest();
        if rebuilt_digest != confirmed_digest {
            return Err(NoteError::StorageError(format!(
                "Committed tree digest mismatch: rebuilt {} != confirmed on-chain {}",
                hex::encode(rebuilt_digest),
                hex::encode(confirmed_digest)
            )));
        }

        let (avl_proof, _returned_value) = committed_tree.generate_lookup_proof(key_bytes.to_vec());

        Ok(TrackerLookupProof {
            key: key_bytes,
            value: confirmed_total_debt.to_be_bytes().to_vec(),
            proof: avl_proof,
        })
    }

    /// Get or create the mutable reserve AVL tree for an (issuer, denomination) pair.
    /// Trees are created lazily so a fresh pair starts from the empty tree digest,
    /// matching a newly created reserve's R5.
    fn reserve_tree_mut(
        &mut self,
        issuer_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<&mut basis_trees::BasisAvlTree, NoteError> {
        match self
            .reserve_avl_states
            .entry((*issuer_pubkey, *denomination))
        {
            std::collections::hash_map::Entry::Occupied(e) => Ok(e.into_mut()),
            std::collections::hash_map::Entry::Vacant(e) => {
                let tree = basis_trees::BasisAvlTree::new().map_err(|e| {
                    NoteError::StorageError(format!(
                        "Failed to create reserve AVL tree for issuer: {:?}",
                        e
                    ))
                })?;
                Ok(e.insert(tree))
            }
        }
    }

    /// Get the already_redeemed amount for a specific (issuer, receiver) edge from the
    /// reserve AVL tree of the given denomination.
    /// Returns the cumulative redeemed amount stored in the reserve's AVL tree
    pub fn get_already_redeemed(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<u64, NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        let reserve_tree = match self
            .reserve_avl_states
            .get(&(*issuer_pubkey, *denomination))
        {
            Some(tree) => tree,
            None => return Ok(0u64), // Fresh pair: no reserve tree yet, first redemption.
        };

        // Lookup value in issuer's reserve AVL tree
        let value_bytes = match reserve_tree.get(&key_bytes) {
            Some(bytes) => bytes,
            None => return Ok(0u64), // First redemption - no already_redeemed amount
        };

        // Value format: timestamp (8 bytes BE) || redeemedAmount (8 bytes BE) = 16 bytes total
        if value_bytes.len() != 16 {
            return Err(NoteError::StorageError(format!(
                "Invalid reserve tree value format: expected 16 bytes (timestamp || redeemedAmount), got {}",
                value_bytes.len()
            )));
        }

        let mut redeemed_bytes = [0u8; 8];
        redeemed_bytes.copy_from_slice(&value_bytes[8..16]);
        Ok(u64::from_be_bytes(redeemed_bytes))
    }

    /// Generate a reserve lookup proof for context var #7
    /// This proof verifies that already_redeemed exists in the reserve's AVL tree
    /// Returns None proof for first redemption (no lookup proof needed)
    pub fn generate_reserve_lookup_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<ReserveLookupProof, NoteError> {
        // Reserve-tree content keys are denomination-free (blake2b256(owner ‖ receiver))
        // in both contracts; only which tree is chosen is namespaced.
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Get the already_redeemed value
        let already_redeemed =
            self.get_already_redeemed(issuer_pubkey, recipient_pubkey, denomination)?;

        // For first redemption, no lookup proof is needed (per spec)
        let is_first_redemption = already_redeemed == 0;

        // Value: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes
        let mut value_bytes = Vec::with_capacity(16);
        // For lookup, use the stored timestamp if available; otherwise 0 for first redemption.
        // The persistent tree stores timestamp || already_redeemed, so retrieve the actual value.
        let reserve_tree = match self
            .reserve_avl_states
            .get(&(*issuer_pubkey, *denomination))
        {
            Some(tree) => tree,
            None => {
                // Fresh issuer: value is 0 || already_redeemed.
                value_bytes.extend_from_slice(&0u64.to_be_bytes());
                value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());
                return Ok(ReserveLookupProof {
                    key: key_bytes,
                    value: value_bytes,
                    proof: None, // Omitted for first redemption
                });
            }
        };
        let stored_value = reserve_tree.get(&key_bytes).unwrap_or_else(|| {
            let mut empty_value = vec![0u8; 8]; // timestamp = 0
            empty_value.extend_from_slice(&already_redeemed.to_be_bytes());
            empty_value
        });
        if stored_value.len() == 16 {
            value_bytes.extend_from_slice(&stored_value);
        } else {
            // Fallback for old-format entries or first redemption: 0 timestamp
            value_bytes.extend_from_slice(&0u64.to_be_bytes());
            value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());
        }

        if is_first_redemption {
            Ok(ReserveLookupProof {
                key: key_bytes,
                value: value_bytes,
                proof: None, // Omitted for first redemption
            })
        } else {
            // Generate AVL proof for the lookup of this specific key from the pair's tree
            let reserve_tree = match self
                .reserve_avl_states
                .get_mut(&(*issuer_pubkey, *denomination))
            {
                Some(tree) => tree,
                None => {
                    // No tree means no prior redemptions; this should have been a first redemption.
                    return Ok(ReserveLookupProof {
                        key: key_bytes,
                        value: value_bytes,
                        proof: None,
                    });
                }
            };
            let (avl_proof, _returned_value) =
                reserve_tree.generate_lookup_proof(key_bytes.to_vec());

            Ok(ReserveLookupProof {
                key: key_bytes,
                value: value_bytes,
                proof: Some(avl_proof),
            })
        }
    }

    /// Generate a reserve insert proof for context var #5 and return updated tree digest for R5.
    ///
    /// This operates on a temporary clone of the reserve AVL tree so that proof generation
    /// is idempotent and does not mutate the persistent tracker state. The persistent tree
    /// is only updated when `update_already_redeemed` is called after a successful on-chain
    /// redemption.
    ///
    /// Value format: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes total
    ///
    /// # Returns
    /// * `(insert_proof, updated_tree_digest)` - Proof bytes and serialized tree digest
    pub fn generate_reserve_insert_proof(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        timestamp: u64,
        new_already_redeemed: u64,
    ) -> Result<(Vec<u8>, Vec<u8>), NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();
        // Value: timestamp (8 bytes BE) || already_redeemed (8 bytes BE)
        let mut value_bytes = Vec::with_capacity(16);
        value_bytes.extend_from_slice(&timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&new_already_redeemed.to_be_bytes());

        // Use the (issuer, denomination) reserve tree (or an empty one if this is the
        // first redemption for the pair). The proof's starting digest must match the
        // on-chain reserve R5.
        let reserve_tree = match self
            .reserve_avl_states
            .get(&(*issuer_pubkey, *denomination))
        {
            Some(tree) => tree,
            None => {
                // Fresh issuer: generate proof against an empty tree.
                let empty_tree = basis_trees::BasisAvlTree::new().map_err(|e| {
                    NoteError::StorageError(format!(
                        "Failed to create empty reserve tree for issuer: {:?}",
                        e
                    ))
                })?;
                let (insert_proof, updated_digest) = empty_tree
                    .generate_insert_proof(key_bytes, value_bytes)
                    .map_err(|e| {
                        NoteError::StorageError(format!(
                            "Reserve AVL tree insert proof failed: {}",
                            e
                        ))
                    })?;
                return Ok((insert_proof, updated_digest.to_vec()));
            }
        };

        // Use the non-mutating proof generator so repeated calls return the same proof.
        let (insert_proof, updated_digest) = reserve_tree
            .generate_insert_proof(key_bytes, value_bytes)
            .map_err(|e| {
                NoteError::StorageError(format!("Reserve AVL tree insert proof failed: {}", e))
            })?;

        Ok((insert_proof, updated_digest.to_vec()))
    }

    /// Current reserve AVL tree root digest (33 bytes) for the given (issuer, denomination)
    /// pair. The on-chain reserve box being spent must have exactly this R5 digest for the
    /// insert proof to verify on-chain.
    pub fn reserve_state_digest(&self, issuer_pubkey: &PubKey, denomination: &DenomKey) -> Vec<u8> {
        match self
            .reserve_avl_states
            .get(&(*issuer_pubkey, *denomination))
        {
            Some(tree) => tree.root_digest().to_vec(),
            None => {
                // Fresh issuer: return the empty-tree digest.
                basis_trees::BasisAvlTree::new()
                    .map(|t| t.root_digest().to_vec())
                    .unwrap_or_else(|_| vec![0u8; 33])
            }
        }
    }

    /// Apply a reserve AVL tree update in memory without writing to the journal.
    /// Used during journal replay on startup.
    fn apply_reserve_avl_update(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        timestamp: u64,
        already_redeemed: u64,
    ) -> Result<(), NoteError> {
        let reserve_tree = self.reserve_tree_mut(issuer_pubkey, denomination)?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();
        let mut value_bytes = Vec::with_capacity(16);
        value_bytes.extend_from_slice(&timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());

        reserve_tree
            .update(key_bytes, value_bytes)
            .map_err(|e| NoteError::StorageError(format!("Reserve AVL tree update failed: {}", e)))
    }

    /// Rebuild per-(issuer, denomination) reserve AVL trees from the persisted journal.
    /// This must be called after `rebuild_avl_tree` so the note keys exist, but
    /// it only operates on the reserve trees, not the note tracker tree.
    pub fn rebuild_reserve_avl_trees(&mut self) -> Result<(), NoteError> {
        tracing::info!("Rebuilding reserve AVL trees from journal...");

        let updates = self.storage.iter_reserve_avl_updates().map_err(|e| {
            NoteError::StorageError(format!("Failed to read reserve AVL journal: {:?}", e))
        })?;

        if updates.is_empty() {
            tracing::info!("No reserve AVL journal entries found");
            return Ok(());
        }

        tracing::info!("Replaying {} reserve AVL journal entries...", updates.len());

        for (issuer_pubkey, note_key, denomination, timestamp, already_redeemed) in updates {
            // Reconstruct recipient pubkey from the note key. The note key is the
            // denomination-derived debt key, so we look up the stored note to obtain
            // the recipient pubkey.
            let note = match self.storage.get_note_by_key(&note_key)? {
                Some(note) => note,
                None => {
                    tracing::warn!(
                        "Reserve AVL journal references unknown note key {}; skipping",
                        hex::encode(note_key.to_bytes())
                    );
                    continue;
                }
            };

            if let Err(e) = self.apply_reserve_avl_update(
                &issuer_pubkey,
                &note.recipient_pubkey,
                &denomination,
                timestamp,
                already_redeemed,
            ) {
                tracing::warn!(
                    "Failed to replay reserve AVL update for issuer {}: {:?}",
                    hex::encode(issuer_pubkey),
                    e
                );
            }
        }

        tracing::info!("Reserve AVL trees rebuilt successfully");
        Ok(())
    }

    /// Update the already_redeemed amount in the (issuer, denomination) reserve AVL tree.
    /// Called after a successful redemption to prevent double-spending.
    /// Value format: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes total
    pub fn update_already_redeemed(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
        timestamp: u64,
        already_redeemed: u64,
    ) -> Result<(), NoteError> {
        self.apply_reserve_avl_update(
            issuer_pubkey,
            recipient_pubkey,
            denomination,
            timestamp,
            already_redeemed,
        )?;

        // Persist the update so the reserve tree can be rebuilt after a restart.
        self.storage.append_reserve_avl_update(
            issuer_pubkey,
            recipient_pubkey,
            denomination,
            timestamp,
            already_redeemed,
        )
    }

    /// Generate proof for a specific note
    pub fn generate_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<NoteProof, NoteError> {
        let key = NoteKey::from_assets(issuer_pubkey, recipient_pubkey, denomination);
        let key_bytes = key.to_bytes();

        // Generate a lookup proof for the key in the AVL tree
        // This captures the path to the key, which can be verified against the root digest
        let (avl_proof, _value) = self.avl_state.generate_lookup_proof(key_bytes.to_vec());

        // Lookup the note to include in proof
        let note = self.lookup_note(issuer_pubkey, recipient_pubkey, denomination)?;

        Ok(NoteProof {
            note,
            avl_proof,
            operations: Vec::new(),
        })
    }

    /// Lookup a note by issuer, recipient and denomination
    pub fn lookup_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<IouNote, NoteError> {
        self.storage
            .get_note(issuer_pubkey, recipient_pubkey, denomination)?
            .ok_or(NoteError::NotFound)
    }

    /// Look up a note by issuer, recipient and denomination, returning `Ok(None)`
    /// when it does not exist (unlike `lookup_note`, which errors on missing notes).
    pub fn get_note_optional(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        denomination: &DenomKey,
    ) -> Result<Option<IouNote>, NoteError> {
        self.storage
            .get_note(issuer_pubkey, recipient_pubkey, denomination)
    }

    /// Get all notes for a specific issuer
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError> {
        self.storage.get_issuer_notes(issuer_pubkey)
    }

    /// Get all notes for a specific recipient
    pub fn get_recipient_notes(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        self.storage.get_recipient_notes(recipient_pubkey)
    }

    /// Get all notes for a specific recipient with issuer information
    pub fn get_recipient_notes_with_issuer(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.storage
            .get_recipient_notes_with_issuer(recipient_pubkey)
    }

    /// Get all notes in the tracker
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError> {
        self.storage.get_all_notes()
    }

    /// Get all notes in the tracker with issuer information
    pub fn get_all_notes_with_issuer(&self) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.storage.get_all_notes_with_issuer()
    }

    /// Update the current state with latest AVL tree root
    fn update_state(&mut self) {
        if self.tree_dirty {
            // The live tree is stale relative to storage (a rebuild failed
            // after an atomic exchange commit, issue #20). Retry the rebuild
            // before publishing a digest; while it keeps failing, the last
            // valid tree's digest is kept — a liveness delay, never a
            // half-applied state.
            let _ = self.rebuild_tree_canonically();
        }
        self.current_state.avl_root_digest = self.avl_state.root_digest();
        // Update timestamp would be set to current time in real implementation
        self.current_state.last_update_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState {
        &self.current_state
    }
}

impl TrackerStateManager {
    /// Find the reserve box ID for an issuer and denomination using key matching
    pub fn find_reserve_box_id_for_issuer(
        &self,
        issuer_pubkey_hex: &str,
        denomination: &DenomKey,
        reserve_tracker: &ReserveTracker,
    ) -> Result<String, NoteError> {
        // Get all reserves from the reserve tracker
        let all_reserves = reserve_tracker.get_all_reserves();

        // Since we now strip the 0x07 prefix when reading from registers,
        // we can do a direct match (with normalization for any remaining edge cases)
        for reserve in all_reserves {
            if (issuer_pubkey_hex == reserve.owner_pubkey
                || normalize_public_key(issuer_pubkey_hex)
                    == normalize_public_key(&reserve.owner_pubkey))
                && reserve.denomination().as_ref() == Some(denomination)
            {
                return Ok(reserve.box_id);
            }
        }

        // If no matching reserve is found, return an error
        Err(NoteError::StorageError(format!(
            "No reserve found for issuer {} in denomination {}",
            issuer_pubkey_hex,
            denomination.denomination_id()
        )))
    }
}

impl IouNote {
    /// Create a new IOU note
    pub fn new(
        recipient_pubkey: PubKey,
        amount_collected: u64,
        amount_redeemed: u64,
        timestamp: u64,
        signature: Signature,
        tracker_nft_id: [u8; 32],
        denomination: DenomKey,
    ) -> Self {
        Self {
            recipient_pubkey,
            amount_collected,
            amount_redeemed,
            timestamp,
            signature,
            tracker_nft_id,
            denomination,
        }
    }

    /// Get the current outstanding debt (collected - redeemed)
    pub fn outstanding_debt(&self) -> u64 {
        self.amount_collected.saturating_sub(self.amount_redeemed)
    }

    /// Check if the note is fully redeemed
    pub fn is_fully_redeemed(&self) -> bool {
        self.amount_collected == self.amount_redeemed
    }

    /// Create and sign a new IOU note
    ///
    /// Message format: debtKey || totalDebt || timestamp || trackerNftId (80 bytes)
    /// where debtKey is the denomination-derived debt-tree key (`DenomKey::debt_key`)
    pub fn create_and_sign(
        recipient_pubkey: PubKey,
        amount_collected: u64,
        _timestamp: u64, // Kept for API compatibility but not used in signing message
        issuer_secret_key: &[u8; 32],
        tracker_nft_id: &[u8; 32],
        denomination: DenomKey,
    ) -> Result<Self, NoteError> {
        use secp256k1::{Secp256k1, SecretKey};

        let secp = Secp256k1::new();

        // Parse the secret key
        let secret_key =
            SecretKey::from_slice(issuer_secret_key).map_err(|_| NoteError::InvalidSignature)?;

        // Generate the corresponding public key
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = public_key.serialize();

        // Generate the signing message: debtKey || totalDebt || timestamp || trackerNftId
        let message = schnorr::signing_message(
            &issuer_pubkey,
            &recipient_pubkey,
            &denomination,
            amount_collected,
            _timestamp,
            tracker_nft_id,
        );

        // Use the chaincash-rs approach for Schnorr signing
        let signature =
            schnorr::schnorr_sign(&message, &secret_key.secret_bytes(), &issuer_pubkey)?;

        Ok(Self {
            recipient_pubkey,
            amount_collected,
            amount_redeemed: 0, // Start with no redemptions
            timestamp: _timestamp,
            signature,
            tracker_nft_id: *tracker_nft_id,
            denomination,
        })
    }

    /// Generate the message that should be signed following the Basis protocol specification.
    ///
    /// message = debtKey || longToByteArray(totalDebt) || longToByteArray(timestamp) || trackerNftId
    ///
    /// Total: 80 bytes
    ///
    /// # Arguments
    /// * `owner_pubkey` - Reserve owner's public key (the issuer of the IOU note)
    pub fn signing_message(&self, owner_pubkey: &PubKey) -> Vec<u8> {
        crate::schnorr::signing_message(
            owner_pubkey,
            &self.recipient_pubkey,
            &self.denomination,
            self.amount_collected,
            self.timestamp,
            &self.tracker_nft_id,
        )
    }

    /// Verify the signature against an issuer public key using Schnorr signature verification
    /// This follows the chaincash-rs approach for Schnorr signature verification
    pub fn verify_signature(&self, issuer_pubkey: &PubKey) -> Result<(), NoteError> {
        let message = self.signing_message(issuer_pubkey);

        // Use the canonical Schnorr verification from basis_core
        let verifier = SchnorrVerifier;
        match verifier.verify_signature(&self.signature, &message, issuer_pubkey) {
            Ok(()) => Ok(()),
            Err(basis_core::traits::CryptoError::InvalidSignature) => {
                Err(NoteError::InvalidSignature)
            }
            Err(basis_core::traits::CryptoError::InvalidPublicKey) => {
                Err(NoteError::InvalidSignature)
            }
            Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => {
                Err(NoteError::InvalidSignature)
            }
            Err(basis_core::traits::CryptoError::InternalError(_)) => {
                Err(NoteError::InvalidSignature)
            }
        }
    }

    /// Get the recipient public key as a hex-encoded string
    pub fn recipient_pubkey_hex(&self) -> String {
        hex::encode(self.recipient_pubkey)
    }
}

/// Blake2b256 hash function for cryptographic hashing
pub fn blake2b256_hash(data: &[u8]) -> [u8; 32] {
    use blake2::{Blake2b, Digest};
    use generic_array::typenum::U32;
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    let result = hasher.finalize();
    result[..32]
        .try_into()
        .expect("Blake2b should produce at least 32 bytes")
}

/// Normalize public key representations to handle different Ergo register formats.
/// This function exists for backward compatibility and handles any remaining edge cases
/// where public keys may still have prefixes that weren't stripped at source.
pub fn normalize_public_key(pubkey_hex: &str) -> String {
    // Since we now strip the 0x07 prefix when reading from registers,
    // this function mainly exists for backward compatibility
    // and handles any remaining edge cases
    let pubkey_bytes = match hex::decode(pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => return pubkey_hex.to_string(), // Return original if invalid hex
    };

    if pubkey_bytes.is_empty() {
        return pubkey_hex.to_string();
    }

    // If it starts with 07 (GroupElement), it's likely a prefixed version
    // where the actual public key starts from the 2nd byte
    // This handles any remaining cases where prefix wasn't stripped at source
    if pubkey_bytes[0] == 0x07 && pubkey_bytes.len() >= 34 {
        // Extract the actual public key (33 bytes after the 0x07 prefix)
        let actual_pubkey = &pubkey_bytes[1..34]; // 33 bytes after the prefix
        hex::encode(actual_pubkey)
    } else {
        // For standard formats, return as is
        pubkey_hex.to_string()
    }
}

// Re-export reserve tracker types
pub use reserve_tracker::{ExtendedReserveInfo, ReserveTracker, ReserveTrackerError};

// Re-export denomination types
pub use basis_core::types::{debt_tree_key, DenomKey, DenomKeyParseError};

// Re-export ergo scanner types
pub use ergo_scanner::{
    create_default_scanner, start_scanner, ErgoBox, NodeConfig, ReserveEvent, ScanType,
    ScannerError, ServerState,
};

// Re-export redemption types
pub use redemption::{RedemptionData, RedemptionError, RedemptionManager, RedemptionRequest};

// Re-export reqwest for use in dependent crates
pub use reqwest;
