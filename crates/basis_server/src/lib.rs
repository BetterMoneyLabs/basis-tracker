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
    // Note: tracker_scanner is not stored here due to Send trait bounds
    // Tracker box ID is fetched from tracker_storage directly
}

// Commands that can be sent to the tracker thread
#[derive(Debug)]
#[allow(clippy::type_complexity)]
pub enum TrackerCommand {
    AddNote {
        issuer_pubkey: basis_store::PubKey,
        note: basis_store::IouNote,
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
        response_tx: tokio::sync::oneshot::Sender<
            Result<Option<basis_store::IouNote>, basis_store::NoteError>,
        >,
    },
    GetNotes {
        response_tx: tokio::sync::oneshot::Sender<
            Result<Vec<(basis_store::PubKey, basis_store::IouNote)>, basis_store::NoteError>,
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
        redeemed_amount: u64,
        /// Explicit cumulative reserve-tree value to sync (from the on-chain build).
        /// Falls back to the note's cumulative redeemed amount when `None`.
        new_already_redeemed: Option<u64>,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::RedemptionError>>,
    },
    GenerateProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<basis_store::NoteProof, basis_store::NoteError>>,
    },
    GetTrackerLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::TrackerLookupProof, basis_store::NoteError>,
        >,
    },
    GetReserveLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::ReserveLookupProof, basis_store::NoteError>,
        >,
    },
    GetReserveInsertProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        timestamp: u64,
        new_already_redeemed: u64,
        response_tx:
            tokio::sync::oneshot::Sender<Result<(Vec<u8>, Vec<u8>), basis_store::NoteError>>,
    },
    /// Get the current reserve AVL tree root digest (33 bytes) for the given issuer.
    GetReserveStateDigest {
        issuer_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<Vec<u8>>,
    },
    /// Get the confirmation record for a single note.
    GetConfirmation {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
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
    ConfirmPendingNotes {
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
}
