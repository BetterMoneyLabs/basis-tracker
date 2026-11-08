//! Basis Server library

pub mod acceptance;
pub mod api;
pub mod auth_middleware;
pub mod authorization;
pub mod config;
pub mod models;
pub mod redemption_build;
pub mod reserve_api;
pub mod store;
pub mod tracker_box_updater;

#[cfg(test)]
mod create_reserve_tests;

use tokio::sync::Mutex;

// Re-export main types for external use
pub use acceptance::*;
pub use api::*;
pub use config::*;
pub use models::*;
pub use redemption_build::*;
pub use reserve_api::*;
pub use store::*;
pub use tracker_box_updater::*;

// Re-export specific types needed by tests
pub use models::{Asset, CreateReserveRequest, ReserveCreationResponse, ReservePaymentRequest};

// Application state that holds a channel to communicate with the tracker thread
#[derive(Clone)]
pub struct AppState {
    pub tx: tokio::sync::mpsc::Sender<TrackerCommand>,
    pub event_store: std::sync::Arc<EventStore>,
    pub ergo_scanner: std::sync::Arc<Mutex<basis_store::ergo_scanner::ServerState>>,
    pub reserve_tracker: std::sync::Arc<Mutex<basis_store::ReserveTracker>>,
    pub config: std::sync::Arc<AppConfig>,
    pub shared_tracker_state:
        std::sync::Arc<tokio::sync::Mutex<tracker_box_updater::SharedTrackerState>>,
    pub tracker_storage: basis_store::persistence::TrackerStorage,
    pub acceptance_predicate: Option<std::sync::Arc<dyn acceptance::NotePredicate>>,
    pub policy_storage: basis_store::persistence::AcceptancePolicyStorage,
    /// Single-use pending redemptions recorded by `POST /redemption/build`, keyed by the
    /// SHA-256 of the unsigned transaction it returned.
    ///
    /// `POST /redemption/submit` only broadcasts a signed transaction whose echoed `unsigned_tx`
    /// matches a live entry, and it advances tracker state from the STORED entry values — never
    /// from caller-supplied amounts (PR #16 B6). See `redemption_build::PendingRedemptionRegistry`.
    pub pending_redemptions: std::sync::Arc<redemption_build::PendingRedemptionRegistry>,
    /// Per-(issuer, recipient) rate limiter for `POST /redemption/build`.
    ///
    /// The tracker pays the fee inputs of every build out of its own wallet, so an unbounded
    /// caller could drain it one fee at a time (PR #16 M1).
    pub redemption_build_rate: std::sync::Arc<redemption_build::BuildRateLimiter>,
    /// Why debt transfers and set-offs are refused, if they are: set at startup from the
    /// tracker store ([`tracker_startup`]) when its confirmation records failed to load, so it
    /// cannot tell which edges carry on-chain state. While set, `POST /notes/transfer` and
    /// `POST /notes/setoff` answer 503 with code `novations_disabled`, and `GET /capabilities`
    /// reports it. `None` normally.
    pub novations_disabled_reason: Option<String>,
    // Note: tracker_scanner is not stored here due to Send trait bounds
    // Tracker box ID is fetched from tracker_storage directly
}

/// The startup decision for the tracker store (design §3.7), made on the tracker thread right
/// after the store opens and before the server accepts requests.
///
/// With `pure_credit` the server refuses to start (`Err`, the error message) when the store
/// holds a stored committed or pending on-chain total, an undecodable note record, or notes or
/// confirmations that failed to load ([`basis_store::TrackerStateManager::check_pure_credit`]).
/// Otherwise (and after a passing check) it returns the reason novations must be refused, if
/// any ([`AppState::novations_disabled_reason`]).
pub fn tracker_startup(
    tracker: &basis_store::TrackerStateManager,
    pure_credit: bool,
) -> Result<Option<String>, String> {
    if pure_credit {
        tracker.check_pure_credit().map_err(|refusal| refusal.to_string())?;
    }
    Ok(tracker.novations_disabled_reason())
}

/// Redacted Debug rendering (design §3.8; signatures are never logged). Every command is
/// debug-logged by the tracker thread and `create_note` debug-logs its request, so
/// [`TrackerCommand`] and the note and novation request structs implement `Debug` by hand on
/// these helpers: public keys, totals and timestamps are shown, a signature only as its
/// fingerprint `sha256:<8 hex>`, the first 4 bytes of the SHA-256 of its bytes. A fingerprint
/// identifies a signature its holder already knows without revealing it.
pub mod redact {
    use std::fmt;

    /// The 8-hex-character fingerprint of a signature's bytes: the first 4 bytes of their SHA-256
    /// ([`basis_store::signature_fingerprint`], which the store types' redacted `Debug` uses too).
    pub fn fingerprint(signature: &[u8]) -> String {
        basis_store::signature_fingerprint(signature)
    }

    /// A signature, as `sha256:<fingerprint>`.
    pub struct Sig<'a>(pub &'a [u8; 65]);

    impl fmt::Debug for Sig<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "sha256:{}", fingerprint(self.0))
        }
    }

    /// A hex-encoded signature from a request: the fingerprint of its decoded bytes (as for
    /// [`Sig`]), or, when it is not 65 bytes of hex, only its length.
    pub struct SigHex<'a>(pub &'a str);

    impl fmt::Debug for SigHex<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match hex::decode(self.0) {
                Ok(bytes) if bytes.len() == 65 => write!(f, "sha256:{}", fingerprint(&bytes)),
                _ => write!(f, "<not a 65-byte hex signature, {} chars>", self.0.len()),
            }
        }
    }

    /// Bytes (a public key, an NFT id, a digest) as lowercase hex.
    pub struct Hex<'a>(pub &'a [u8]);

    impl fmt::Debug for Hex<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&hex::encode(self.0))
        }
    }

    /// A hex field of a request (a public key: 33 bytes, a tracker NFT id: 32 bytes), shown only
    /// when it is hex of exactly that length; anything else, which could hold any value, a
    /// signature included, only by its length.
    pub struct HexField<'a>(pub &'a str, pub usize);

    impl fmt::Debug for HexField<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let (s, bytes) = (self.0, self.1);
            if s.len() == 2 * bytes && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                f.write_str(s)
            } else {
                write!(f, "<not {} bytes of hex, {} chars>", bytes, s.len())
            }
        }
    }

    /// A denomination field of a request, shown only when it has a denomination's form (empty,
    /// "erg", or "token:" and 64 hex characters); anything else only by its length.
    pub struct Denom<'a>(pub &'a str);

    impl fmt::Debug for Denom<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let s = self.0;
            let token = s
                .strip_prefix("token:")
                .is_some_and(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()));
            if s.is_empty() || s.eq_ignore_ascii_case("erg") || token {
                write!(f, "{:?}", s)
            } else {
                write!(f, "<not a denomination, {} chars>", s.len())
            }
        }
    }

    /// A short free-text field (a box id, a transaction id), shown when at most 72 characters,
    /// else only by its length: a 130-character signature can never appear through it.
    pub struct Text<'a>(pub &'a str);

    impl fmt::Debug for Text<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0.chars().count() <= 72 {
                write!(f, "{:?}", self.0)
            } else {
                write!(f, "<{} chars>", self.0.chars().count())
            }
        }
    }

    /// A denomination, as its id ("erg" or "token:<64 hex>").
    pub struct DenomId<'a>(pub &'a basis_store::DenomKey);

    impl fmt::Debug for DenomId<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{:?}", self.0.denomination_id())
        }
    }

    /// A note, with its signature as a fingerprint.
    pub struct Note<'a>(pub &'a basis_store::IouNote);

    impl fmt::Debug for Note<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let n = self.0;
            f.debug_struct("IouNote")
                .field("recipient_pubkey", &Hex(&n.recipient_pubkey))
                .field("amount_collected", &n.amount_collected)
                .field("amount_redeemed", &n.amount_redeemed)
                .field("timestamp", &n.timestamp)
                .field("signature", &Sig(&n.signature))
                .field("tracker_nft_id", &Hex(&n.tracker_nft_id))
                .field("denomination", &DenomId(&n.denomination))
                .finish()
        }
    }

    /// A bound note's prior acknowledgment, with its signature as a fingerprint.
    pub struct Prior<'a>(pub &'a basis_store::PriorAck);

    impl fmt::Debug for Prior<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("PriorAck")
                .field("signature", &Sig(&self.0.signature))
                .field("prior_total", &self.0.prior_total)
                .finish()
        }
    }

    /// The seven signatures of a debt transfer, as fingerprints.
    pub struct TransferSigs<'a>(pub &'a basis_store::TransferSignatures);

    impl fmt::Debug for TransferSigs<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let s = self.0;
            f.debug_struct("TransferSignatures")
                .field("issuer_from", &Sig(&s.issuer_from))
                .field("issuer_to", &Sig(&s.issuer_to))
                .field("from_recipient_prior", &Sig(&s.from_recipient_prior))
                .field("from_recipient", &Sig(&s.from_recipient))
                .field("from_recipient_to", &Sig(&s.from_recipient_to))
                .field("to_recipient_prior", &Sig(&s.to_recipient_prior))
                .field("to_recipient", &Sig(&s.to_recipient))
                .finish()
        }
    }

    /// One set-off edge, with both signatures as fingerprints.
    pub struct Edge<'a>(pub &'a basis_store::SetOffEdge);

    impl fmt::Debug for Edge<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let e = self.0;
            f.debug_struct("SetOffEdge")
                .field("issuer_pubkey", &Hex(&e.issuer_pubkey))
                .field("recipient_pubkey", &Hex(&e.recipient_pubkey))
                .field("decrease", &e.decrease)
                .field("timestamp", &e.timestamp)
                .field("issuer_signature", &Sig(&e.issuer_signature))
                .field("recipient_signature", &Sig(&e.recipient_signature))
                .finish()
        }
    }

    /// A set-off's edges.
    pub struct Edges<'a>(pub &'a [basis_store::SetOffEdge]);

    impl fmt::Debug for Edges<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_list().entries(self.0.iter().map(Edge)).finish()
        }
    }

    /// A redemption request, with both signatures as fingerprints.
    pub struct Redemption<'a>(pub &'a basis_store::RedemptionRequest);

    impl fmt::Debug for Redemption<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let r = self.0;
            f.debug_struct("RedemptionRequest")
                .field("issuer_pubkey", &HexField(&r.issuer_pubkey, 33))
                .field("recipient_pubkey", &HexField(&r.recipient_pubkey, 33))
                .field("amount", &r.amount)
                .field("timestamp", &r.timestamp)
                .field("denomination", &DenomId(&r.denomination))
                .field("emergency", &r.emergency)
                .field("reserve_box_id", &Text(&r.reserve_box_id))
                .field("tracker_box_id", &Text(&r.tracker_box_id))
                .field("current_height", &r.current_height)
                .field("issuer_signature", &SigHex(&r.issuer_signature))
                .field(
                    "tracker_signature",
                    &r.tracker_signature.as_deref().map(SigHex),
                )
                .finish_non_exhaustive()
        }
    }
}

/// Boxed acceptance-policy check for an exchange (finding #8). The store
/// evaluates it against the pre-mutation state, atomically with the mutation;
/// `Err(reason)` aborts the exchange. Newtype only so `TrackerCommand`'s
/// `Debug` impl can print a placeholder for the opaque closure.
pub struct ExchangeAcceptanceCheck(pub basis_store::ExchangeAcceptanceCheckFn);

impl std::fmt::Debug for ExchangeAcceptanceCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExchangeAcceptanceCheck(..)")
    }
}

// Commands that can be sent to the tracker thread. `Debug` is implemented by
// hand (below): every command is debug-logged, and signatures are never logged.
#[allow(clippy::type_complexity)]
pub enum TrackerCommand {
    AddNote {
        issuer_pubkey: basis_store::PubKey,
        note: basis_store::IouNote,
        /// The issuer's prior acknowledgment of a bound note (timestamp residue
        /// 997); `None` for every other note.
        prior: Option<basis_store::PriorAck>,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::NoteError>>,
    },
    GetNotesByIssuer {
        issuer_pubkey: basis_store::PubKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<Vec<basis_store::IouNote>, basis_store::NoteError>>,
    },
    GetNotesByRecipient {
        recipient_pubkey: basis_store::PubKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<Vec<basis_store::IouNote>, basis_store::NoteError>>,
    },
    GetNotesByRecipientWithIssuer {
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<Vec<(basis_store::PubKey, basis_store::IouNote)>, basis_store::NoteError>,
        >,
    },
    GetNoteByIssuerAndRecipient {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<Option<basis_store::IouNote>, basis_store::NoteError>,
        >,
    },
    GetNotes {
        response_tx: tokio::sync::oneshot::Sender<
            Result<Vec<(basis_store::PubKey, basis_store::IouNote)>, basis_store::NoteError>,
        >,
    },
    /// Atomic cross-denomination debt exchange (triage/novation,
    /// specs/ergo/multi_collateral_reserves.md §10). All four signatures were
    /// verified by the API layer; the store re-verifies them before mutating.
    ///
    /// `acceptance_check` is evaluated by the store against the pre-mutation
    /// state, atomically with the mutation (finding #8): evaluating it in the
    /// API handler would race concurrent note mutations. The closure carries
    /// the predicate and the reserve-collateral snapshot; `Err(reason)` aborts
    /// the exchange with `NoteError::AcceptancePolicy` and nothing is mutated.
    ExchangeNotes {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        from_denomination: basis_store::DenomKey,
        from_amount: u64,
        to_denomination: basis_store::DenomKey,
        to_amount: u64,
        timestamp: u64,
        issuer_from_signature: basis_store::Signature,
        recipient_from_signature: basis_store::Signature,
        issuer_to_signature: basis_store::Signature,
        recipient_to_signature: basis_store::Signature,
        /// Box id of the issuer's canonical from-denomination reserve at
        /// request time; stored on the staged exchange so phase 2 can verify
        /// the reserve was not drained while the to-edge increase was staged
        /// (issue #17).
        from_reserve_box_id: String,
        acceptance_check: Option<ExchangeAcceptanceCheck>,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::NoteError>>,
    },
    /// Debt transfer (triad novation, specs/spec.md "Debt Transfer"), version 3:
    /// move `amount` of the issuer's debt from the edge to `from_recipient` to
    /// the edge to `to_recipient`, in one denomination. Pure-credit trackers
    /// only (the store refuses on a tracker with on-chain state). The store
    /// verifies all seven signatures, including both creditors' prior
    /// acknowledgments, before mutating; `acceptance_check` (the new creditor's
    /// policy) runs after them, atomically with the mutation, as for exchanges.
    TransferNotes {
        issuer_pubkey: basis_store::PubKey,
        from_recipient_pubkey: basis_store::PubKey,
        to_recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        amount: u64,
        timestamp: u64,
        signatures: basis_store::TransferSignatures,
        /// Optional and diagnostic: the from-edge total the old creditor
        /// acknowledged; when present it must equal the stored total.
        from_prior_total: Option<u64>,
        /// Optional and diagnostic: the to-edge total the new creditor
        /// acknowledged (0 for a new edge); when present it must equal the stored total.
        to_prior_total: Option<u64>,
        acceptance_check: Option<ExchangeAcceptanceCheck>,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::TransferTotals, basis_store::NoteError>,
        >,
    },
    /// Multilateral set-off (obligation clearing), version 3: lower several
    /// edges at once, net-neutral for every participant. Each edge carries its
    /// own timestamp (per-edge timestamps; the API fills in the request's
    /// timestamp for edges without one). The store verifies every edge's
    /// timestamp rules, both signatures of every edge and the per-participant
    /// balance before mutating.
    SetOff {
        denomination: basis_store::DenomKey,
        edges: Vec<basis_store::SetOffEdge>,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::SetOffResult, basis_store::NoteError>,
        >,
    },
    /// List staged two-phase exchanges whose to-edge increase awaits the
    /// from-edge decrease being committed (issue #17 phase 2). The tracker box
    /// updater observes each record's reserve box and reports back through
    /// [`TrackerCommand::ProcessStagedExchanges`].
    ListStagedExchanges {
        response_tx: tokio::sync::oneshot::Sender<
            Result<Vec<basis_store::StagedExchangeInfo>, basis_store::NoteError>,
        >,
    },
    /// Apply or cancel staged exchanges (issue #17 phase 2) using the updater's
    /// on-chain observations of each staged record's reserve box.
    ProcessStagedExchanges {
        observations: Vec<basis_store::ExchangeObservation>,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::ProcessedExchanges, basis_store::NoteError>,
        >,
    },
    /// List pending (staged or cancelled) exchanges for the
    /// `/exchanges/pending` endpoint (issue #17).
    ListPendingExchanges {
        issuer_pubkey: Option<basis_store::PubKey>,
        recipient_pubkey: Option<basis_store::PubKey>,
        response_tx: tokio::sync::oneshot::Sender<
            Result<Vec<([u8; 32], basis_store::PendingExchangeRecord)>, basis_store::NoteError>,
        >,
    },
    InitiateRedemption {
        request: basis_store::RedemptionRequest,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::RedemptionData, basis_store::RedemptionError>,
        >,
    },
    CompleteRedemption {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        redeemed_amount: u64,
        /// Explicit cumulative reserve-tree value to sync (from the on-chain build).
        /// Falls back to the note's cumulative redeemed amount when `None`.
        new_already_redeemed: Option<u64>,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::RedemptionError>>,
    },
    GenerateProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<basis_store::NoteProof, basis_store::NoteError>>,
    },
    GetTrackerLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::TrackerLookupProof, basis_store::NoteError>,
        >,
    },
    /// Tracker lookup proof against the COMMITTED on-chain state (latest confirmed
    /// tracker box digest). Redemption transactions must use this variant: the live
    /// tree may contain uncommitted notes that would invalidate the proof on-chain
    /// (issue #15 S6). Fails with `NoteError::NoteNotConfirmed` when the note is not
    /// committed on-chain yet.
    GetCommittedTrackerLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::TrackerLookupProof, basis_store::NoteError>,
        >,
    },
    GetReserveLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::ReserveLookupProof, basis_store::NoteError>,
        >,
    },
    GetReserveInsertProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        timestamp: u64,
        new_already_redeemed: u64,
        response_tx:
            tokio::sync::oneshot::Sender<Result<(Vec<u8>, Vec<u8>), basis_store::NoteError>>,
    },
    /// Get the current reserve AVL tree root digest (33 bytes) for the given
    /// (issuer, denomination) pair.
    GetReserveStateDigest {
        issuer_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx: tokio::sync::oneshot::Sender<Vec<u8>>,
    },
    /// Get the confirmation record for a single note.
    GetConfirmation {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        denomination: basis_store::DenomKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<Option<basis_store::NoteConfirmation>, basis_store::NoteError>,
        >,
    },
    /// Get a snapshot of all confirmation records keyed by note key.
    GetAllConfirmations {
        response_tx: tokio::sync::oneshot::Sender<
            std::collections::HashMap<[u8; 32], basis_store::NoteConfirmation>,
        >,
    },
    /// Mark all currently-local notes as pending for an in-flight update tx.
    MarkNotesPending {
        digest: [u8; 33],
        tx_id: String,
        submitted_height: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<usize, basis_store::NoteError>>,
    },
    /// Promote all pending notes to confirmed after an update tx confirms.
    /// `digest` is the AVL root committed by the confirming tracker box.
    ConfirmPendingNotes {
        digest: [u8; 33],
        box_id: String,
        height: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<usize, basis_store::NoteError>>,
    },
    /// Revert all pending notes back to local state (update tx dropped/rejected).
    RevertPendingNotes {
        response_tx: tokio::sync::oneshot::Sender<Result<usize, basis_store::NoteError>>,
    },
    /// Reconcile confirmation records with an observed on-chain digest.
    ReconcileWithConfirmedDigest {
        digest: [u8; 33],
        box_id: String,
        height: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<usize, basis_store::NoteError>>,
    },
    /// The number of edges whose confirmation record has a committed or pending
    /// on-chain total (`GET /capabilities` `onchain_edges`).
    OnchainEdgeCount {
        response_tx: tokio::sync::oneshot::Sender<usize>,
    },
}

/// Redacted: public keys, denominations, totals, timestamps and ids are shown;
/// every signature only as its fingerprint (see [`redact`]); response channels
/// are elided (`..`).
impl std::fmt::Debug for TrackerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use redact::{DenomId, Hex, Sig, Text};
        match self {
            TrackerCommand::AddNote {
                issuer_pubkey,
                note,
                prior,
                ..
            } => f
                .debug_struct("AddNote")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .field("note", &redact::Note(note))
                .field("prior", &prior.as_ref().map(redact::Prior))
                .finish_non_exhaustive(),
            TrackerCommand::GetNotesByIssuer { issuer_pubkey, .. } => f
                .debug_struct("GetNotesByIssuer")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .finish_non_exhaustive(),
            TrackerCommand::GetNotesByRecipient {
                recipient_pubkey, ..
            } => f
                .debug_struct("GetNotesByRecipient")
                .field("recipient_pubkey", &Hex(recipient_pubkey))
                .finish_non_exhaustive(),
            TrackerCommand::GetNotesByRecipientWithIssuer {
                recipient_pubkey, ..
            } => f
                .debug_struct("GetNotesByRecipientWithIssuer")
                .field("recipient_pubkey", &Hex(recipient_pubkey))
                .finish_non_exhaustive(),
            TrackerCommand::GetNoteByIssuerAndRecipient {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                ..
            } => edge_query(
                f,
                "GetNoteByIssuerAndRecipient",
                issuer_pubkey,
                recipient_pubkey,
                denomination,
            ),
            TrackerCommand::GetNotes { .. } => f.debug_struct("GetNotes").finish_non_exhaustive(),
            TrackerCommand::ExchangeNotes {
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
                acceptance_check,
                ..
            } => f
                .debug_struct("ExchangeNotes")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .field("recipient_pubkey", &Hex(recipient_pubkey))
                .field("from_denomination", &DenomId(from_denomination))
                .field("from_amount", from_amount)
                .field("to_denomination", &DenomId(to_denomination))
                .field("to_amount", to_amount)
                .field("timestamp", timestamp)
                .field("issuer_from_signature", &Sig(issuer_from_signature))
                .field("recipient_from_signature", &Sig(recipient_from_signature))
                .field("issuer_to_signature", &Sig(issuer_to_signature))
                .field("recipient_to_signature", &Sig(recipient_to_signature))
                .field("from_reserve_box_id", &Text(from_reserve_box_id))
                .field("acceptance_check", acceptance_check)
                .finish_non_exhaustive(),
            TrackerCommand::TransferNotes {
                issuer_pubkey,
                from_recipient_pubkey,
                to_recipient_pubkey,
                denomination,
                amount,
                timestamp,
                signatures,
                from_prior_total,
                to_prior_total,
                acceptance_check,
                ..
            } => f
                .debug_struct("TransferNotes")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .field("from_recipient_pubkey", &Hex(from_recipient_pubkey))
                .field("to_recipient_pubkey", &Hex(to_recipient_pubkey))
                .field("denomination", &DenomId(denomination))
                .field("amount", amount)
                .field("timestamp", timestamp)
                .field("signatures", &redact::TransferSigs(signatures))
                .field("from_prior_total", from_prior_total)
                .field("to_prior_total", to_prior_total)
                .field("acceptance_check", acceptance_check)
                .finish_non_exhaustive(),
            TrackerCommand::SetOff {
                denomination,
                edges,
                ..
            } => f
                .debug_struct("SetOff")
                .field("denomination", &DenomId(denomination))
                .field("edges", &redact::Edges(edges))
                .finish_non_exhaustive(),
            TrackerCommand::ListStagedExchanges { .. } => {
                f.debug_struct("ListStagedExchanges").finish_non_exhaustive()
            }
            TrackerCommand::ProcessStagedExchanges { observations, .. } => f
                .debug_struct("ProcessStagedExchanges")
                .field("observations", &observations.len())
                .finish_non_exhaustive(),
            TrackerCommand::ListPendingExchanges {
                issuer_pubkey,
                recipient_pubkey,
                ..
            } => f
                .debug_struct("ListPendingExchanges")
                .field("issuer_pubkey", &issuer_pubkey.as_ref().map(|k| Hex(k)))
                .field("recipient_pubkey", &recipient_pubkey.as_ref().map(|k| Hex(k)))
                .finish_non_exhaustive(),
            TrackerCommand::InitiateRedemption { request, .. } => f
                .debug_struct("InitiateRedemption")
                .field("request", &redact::Redemption(request))
                .finish_non_exhaustive(),
            TrackerCommand::CompleteRedemption {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                redeemed_amount,
                new_already_redeemed,
                ..
            } => f
                .debug_struct("CompleteRedemption")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .field("recipient_pubkey", &Hex(recipient_pubkey))
                .field("denomination", &DenomId(denomination))
                .field("redeemed_amount", redeemed_amount)
                .field("new_already_redeemed", new_already_redeemed)
                .finish_non_exhaustive(),
            TrackerCommand::GenerateProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                ..
            } => edge_query(f, "GenerateProof", issuer_pubkey, recipient_pubkey, denomination),
            TrackerCommand::GetTrackerLookupProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                ..
            } => edge_query(
                f,
                "GetTrackerLookupProof",
                issuer_pubkey,
                recipient_pubkey,
                denomination,
            ),
            TrackerCommand::GetCommittedTrackerLookupProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                ..
            } => edge_query(
                f,
                "GetCommittedTrackerLookupProof",
                issuer_pubkey,
                recipient_pubkey,
                denomination,
            ),
            TrackerCommand::GetReserveLookupProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                ..
            } => edge_query(
                f,
                "GetReserveLookupProof",
                issuer_pubkey,
                recipient_pubkey,
                denomination,
            ),
            TrackerCommand::GetReserveInsertProof {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                timestamp,
                new_already_redeemed,
                ..
            } => f
                .debug_struct("GetReserveInsertProof")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .field("recipient_pubkey", &Hex(recipient_pubkey))
                .field("denomination", &DenomId(denomination))
                .field("timestamp", timestamp)
                .field("new_already_redeemed", new_already_redeemed)
                .finish_non_exhaustive(),
            TrackerCommand::GetReserveStateDigest {
                issuer_pubkey,
                denomination,
                ..
            } => f
                .debug_struct("GetReserveStateDigest")
                .field("issuer_pubkey", &Hex(issuer_pubkey))
                .field("denomination", &DenomId(denomination))
                .finish_non_exhaustive(),
            TrackerCommand::GetConfirmation {
                issuer_pubkey,
                recipient_pubkey,
                denomination,
                ..
            } => edge_query(
                f,
                "GetConfirmation",
                issuer_pubkey,
                recipient_pubkey,
                denomination,
            ),
            TrackerCommand::GetAllConfirmations { .. } => {
                f.debug_struct("GetAllConfirmations").finish_non_exhaustive()
            }
            TrackerCommand::MarkNotesPending {
                digest,
                tx_id,
                submitted_height,
                ..
            } => f
                .debug_struct("MarkNotesPending")
                .field("digest", &Hex(digest))
                .field("tx_id", &Text(tx_id))
                .field("submitted_height", submitted_height)
                .finish_non_exhaustive(),
            TrackerCommand::ConfirmPendingNotes {
                digest,
                box_id,
                height,
                ..
            } => f
                .debug_struct("ConfirmPendingNotes")
                .field("digest", &Hex(digest))
                .field("box_id", &Text(box_id))
                .field("height", height)
                .finish_non_exhaustive(),
            TrackerCommand::RevertPendingNotes { .. } => {
                f.debug_struct("RevertPendingNotes").finish_non_exhaustive()
            }
            TrackerCommand::ReconcileWithConfirmedDigest {
                digest,
                box_id,
                height,
                ..
            } => f
                .debug_struct("ReconcileWithConfirmedDigest")
                .field("digest", &Hex(digest))
                .field("box_id", &Text(box_id))
                .field("height", height)
                .finish_non_exhaustive(),
            TrackerCommand::OnchainEdgeCount { .. } => {
                f.debug_struct("OnchainEdgeCount").finish_non_exhaustive()
            }
        }
    }
}

/// Debug of a command that names one edge (issuer, recipient, denomination).
fn edge_query(
    f: &mut std::fmt::Formatter<'_>,
    name: &str,
    issuer_pubkey: &basis_store::PubKey,
    recipient_pubkey: &basis_store::PubKey,
    denomination: &basis_store::DenomKey,
) -> std::fmt::Result {
    f.debug_struct(name)
        .field("issuer_pubkey", &redact::Hex(issuer_pubkey))
        .field("recipient_pubkey", &redact::Hex(recipient_pubkey))
        .field("denomination", &redact::DenomId(denomination))
        .finish_non_exhaustive()
}
