//! Basis Server library
//!
//! The legacy proof commands are structurally absent from the production
//! actor rather than hidden behind runtime branches:
//!
//! ```compile_fail
//! fn removed(command: basis_server::TrackerCommand) {
//!     if let basis_server::TrackerCommand::GenerateProof { .. } = command {}
//! }
//! ```
//!
//! ```compile_fail
//! fn removed(command: basis_server::TrackerCommand) {
//!     if let basis_server::TrackerCommand::GetTrackerLookupProof { .. } = command {}
//! }
//! ```
//!
//! ```compile_fail
//! fn removed(command: basis_server::TrackerCommand) {
//!     if let basis_server::TrackerCommand::GetReserveLookupProof { .. } = command {}
//! }
//! ```
//!
//! ```compile_fail
//! fn removed(command: basis_server::TrackerCommand) {
//!     if let basis_server::TrackerCommand::GetReserveInsertProof { .. } = command {}
//! }
//! ```
//!
//! ```compile_fail
//! fn removed(command: basis_server::TrackerCommand) {
//!     if let basis_server::TrackerCommand::GetReserveStateDigest { .. } = command {}
//! }
//! ```

pub mod acceptance;
pub mod api;
mod bounded_http;
pub mod config;
pub mod models;
pub mod redemption_build;
pub mod reserve_api;
pub mod store;
pub mod tracker_box_updater;

#[cfg(test)]
mod create_reserve_tests;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use tokio::sync::Mutex;

pub const TRACKER_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrackerRequestError {
    #[error("tracker request queue is full")]
    QueueFull,
    #[error("tracker request worker is unavailable")]
    QueueClosed,
    #[error("tracker request timed out")]
    Timeout,
    #[error("tracker response channel closed")]
    ResponseClosed,
}

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

pub(crate) const V1_REDEMPTION_RETIRED: &str =
    "Basis v1 redemption is retired; v2 remains disabled until confirmed-chain authority and exact manifest admission are integrated";

pub(crate) fn reject_retired_v1_redemption<T>() -> (StatusCode, Json<ApiResponse<T>>) {
    (
        StatusCode::GONE,
        Json(models::error_response(V1_REDEMPTION_RETIRED.to_string())),
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
            "/config/reserve-contract-p2s",
            get(api::get_basis_reserve_contract_p2s),
        )
}

/// Compatibility tombstones for every retired v1 redemption endpoint.
///
/// These routes intentionally remain visible as HTTP 410 responses so stale
/// clients cannot fall through to an older proof, signing, build, or broadcast
/// path. This router contains no v2 activation path.
pub fn retired_v1_redemption_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/redeem",
            post(api::initiate_redemption).options(handle_options),
        )
        .route(
            "/redeem/complete",
            post(api::complete_redemption).options(handle_options),
        )
        .route("/proof/redemption", get(api::get_redemption_proof))
        .route("/tracker/proof", get(api::get_tracker_proof))
        .route("/reserve/proof", get(api::get_reserve_proof))
        .route(
            "/tracker/signature",
            post(api::request_tracker_signature).options(handle_options),
        )
        .route(
            "/redemption/prepare",
            post(api::prepare_redemption).options(handle_options),
        )
        .route(
            "/redemption/build",
            post(redemption_build::build_redemption).options(handle_options),
        )
        .route(
            "/redemption/submit",
            post(redemption_build::submit_redemption).options(handle_options),
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

impl TrackerCommand {
    /// Return true when the HTTP/request-side receiver has already gone away.
    /// The single worker checks this before starting potentially expensive work,
    /// so timed-out requests do not build an unbounded stale-work backlog.
    pub fn response_is_closed(&self) -> bool {
        match self {
            Self::AddNote { response_tx, .. } => response_tx.is_closed(),
            Self::GetNotesByIssuer { response_tx, .. } => response_tx.is_closed(),
            Self::GetProjectedIssuerGrossDebt { response_tx, .. } => response_tx.is_closed(),
            Self::GetNotesByRecipient { response_tx, .. } => response_tx.is_closed(),
            Self::GetNotesByRecipientWithIssuer { response_tx, .. } => response_tx.is_closed(),
            Self::GetNoteByIssuerAndRecipient { response_tx, .. } => response_tx.is_closed(),
            Self::GetNotes { response_tx } => response_tx.is_closed(),
            Self::GetValidatedState { response_tx } => response_tx.is_closed(),
            Self::GetConfirmation { response_tx, .. } => response_tx.is_closed(),
            Self::GetAllConfirmations { response_tx } => response_tx.is_closed(),
            Self::BeginPublication { response_tx, .. } => response_tx.is_closed(),
            Self::RecordPublicationAttempt { response_tx, .. } => response_tx.is_closed(),
            Self::ConfirmPublication { response_tx, .. } => response_tx.is_closed(),
            Self::AbortPublication { response_tx, .. } => response_tx.is_closed(),
        }
    }
}

pub async fn tracker_request<T>(
    tx: &tokio::sync::mpsc::Sender<TrackerCommand>,
    make_command: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> TrackerCommand,
) -> Result<T, TrackerRequestError> {
    tracker_request_with_timeout(tx, TRACKER_REQUEST_TIMEOUT, make_command).await
}

async fn tracker_request_with_timeout<T>(
    tx: &tokio::sync::mpsc::Sender<TrackerCommand>,
    timeout: std::time::Duration,
    make_command: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> TrackerCommand,
) -> Result<T, TrackerRequestError> {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    match tx.try_send(make_command(response_tx)) {
        Ok(()) => {}
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            return Err(TrackerRequestError::QueueFull)
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            return Err(TrackerRequestError::QueueClosed)
        }
    }

    match tokio::time::timeout(timeout, response_rx).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) => Err(TrackerRequestError::ResponseClosed),
        Err(_) => Err(TrackerRequestError::Timeout),
    }
}

#[cfg(test)]
mod tracker_request_tests {
    use super::*;

    fn get_notes(
        response_tx: tokio::sync::oneshot::Sender<
            Result<Vec<(basis_store::PubKey, basis_store::IouNote)>, basis_store::NoteError>,
        >,
    ) -> TrackerCommand {
        TrackerCommand::GetNotes { response_tx }
    }

    #[tokio::test]
    async fn tracker_request_rejects_full_and_closed_queues_without_waiting() {
        let (full_tx, mut full_rx) = tokio::sync::mpsc::channel(1);
        let (occupied_tx, _occupied_rx) = tokio::sync::oneshot::channel();
        full_tx.try_send(get_notes(occupied_tx)).unwrap();
        assert!(matches!(
            tracker_request_with_timeout(
                &full_tx,
                std::time::Duration::from_millis(10),
                get_notes,
            )
            .await,
            Err(TrackerRequestError::QueueFull)
        ));
        assert!(full_rx.recv().await.is_some());

        let (closed_tx, closed_rx) = tokio::sync::mpsc::channel(1);
        drop(closed_rx);
        assert!(matches!(
            tracker_request_with_timeout(
                &closed_tx,
                std::time::Duration::from_millis(10),
                get_notes,
            )
            .await,
            Err(TrackerRequestError::QueueClosed)
        ));
    }

    #[tokio::test]
    async fn tracker_request_times_out_and_marks_stale_command_closed() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        assert!(matches!(
            tracker_request_with_timeout(&tx, std::time::Duration::from_millis(1), get_notes,)
                .await,
            Err(TrackerRequestError::Timeout)
        ));
        let command = rx.recv().await.unwrap();
        assert!(command.response_is_closed());
    }

    #[tokio::test]
    async fn tracker_request_returns_the_worker_response() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let worker = tokio::spawn(async move {
            let TrackerCommand::GetNotes { response_tx } = rx.recv().await.unwrap() else {
                panic!("unexpected command")
            };
            response_tx.send(Ok(Vec::new())).unwrap();
        });
        let response =
            tracker_request_with_timeout(&tx, std::time::Duration::from_millis(50), get_notes)
                .await
                .unwrap()
                .unwrap();
        assert!(response.is_empty());
        worker.await.unwrap();
    }
}
