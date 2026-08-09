//! Basis Server library

pub mod acceptance;
pub mod api;
pub mod config;
pub mod models;
pub mod redemption_build;
pub mod reserve_api;
pub mod store;
pub mod tracker_box_updater;

#[cfg(test)]
mod create_reserve_tests;

use axum::{
    routing::{get, post},
    Router,
};
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

/// Opaque actor-issued fence held across tracker commitment signing and
/// broadcast. While a lease is active, the tracker actor rejects every other
/// command so no state transition can race the external effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationLease {
    pub id: u64,
    pub digest: [u8; 33],
}

/// Handle OPTIONS preflight requests for CORS.
pub async fn handle_options() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::OK,
        [("Access-Control-Allow-Origin", "*")],
        "",
    )
}

/// The generation-sensitive construction routes used by the production
/// server. Keeping their wiring here lets integration tests exercise the same
/// router that `main` merges into the application.
pub fn reserve_construction_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/reserves/create",
            post(api::create_reserve_payload).options(handle_options),
        )
        .route(
            "/redemption/build",
            post(redemption_build::build_redemption).options(handle_options),
        )
        .route(
            "/config/reserve-contract-p2s",
            get(api::get_basis_reserve_contract_p2s),
        )
}

// Commands that can be sent to the tracker thread
#[derive(Debug)]
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
    GetProjectedIssuerGrossDebt {
        issuer_pubkey: basis_store::PubKey,
        candidate_recipient: Option<basis_store::PubKey>,
        candidate_total_debt: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<u64, basis_store::NoteError>>,
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
    GenerateProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<(basis_store::NoteProof, basis_store::TrackerState), basis_store::NoteError>,
        >,
    },
    GetTrackerLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<
                (basis_store::TrackerLookupProof, basis_store::TrackerState),
                basis_store::NoteError,
            >,
        >,
    },
    GetReserveLookupProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<(basis_store::ReserveLookupProof, Vec<u8>), basis_store::NoteError>,
        >,
    },
    GetReserveInsertProof {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        timestamp: u64,
        new_already_redeemed: u64,
        response_tx: tokio::sync::oneshot::Sender<
            Result<(Vec<u8>, Vec<u8>, Vec<u8>), basis_store::NoteError>,
        >,
    },
    /// Get the current reserve AVL tree root digest (33 bytes).
    GetReserveStateDigest {
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<u8>, basis_store::NoteError>>,
    },
    /// Get the current BNS2-backed tracker state through its owning actor.
    GetValidatedState {
        response_tx:
            tokio::sync::oneshot::Sender<Result<basis_store::TrackerState, basis_store::NoteError>>,
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
            Result<
                std::collections::HashMap<[u8; 32], basis_store::NoteConfirmation>,
                basis_store::NoteError,
            >,
        >,
    },
    /// Validate and reconcile an observed tracker generation, then freeze the
    /// actor until the external publication attempt is resolved.
    BeginPublication {
        tracker_nft_id: [u8; 32],
        observed_root: [u8; 33],
        box_id: String,
        height: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<PublicationLease, basis_store::NoteError>>,
    },
    /// Durably bind the exact transaction identity before the broadcast request
    /// crosses the node boundary. The actor fence remains held.
    RecordPublicationAttempt {
        lease: PublicationLease,
        tx_id: String,
        submitted_height: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<usize, basis_store::NoteError>>,
    },
    /// Promote the durable attempt after active-chain confirmation and release
    /// the actor fence.
    ConfirmPublication {
        tx_id: String,
        box_id: String,
        height: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<usize, basis_store::NoteError>>,
    },
    /// Release an actor fence after a no-op or failed publication attempt.
    AbortPublication {
        lease: PublicationLease,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::NoteError>>,
    },
}
