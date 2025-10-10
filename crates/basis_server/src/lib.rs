//! Basis Server library

pub mod api;
pub mod config;
pub mod models;
pub mod reserve_api;
pub mod store;

use tokio::sync::Mutex;

// Re-export main types for external use
pub use api::*;
pub use config::*;
pub use models::*;
pub use reserve_api::*;
pub use store::*;

// Application state that holds a channel to communicate with the tracker thread
#[derive(Clone)]
pub struct AppState {
    pub tx: tokio::sync::mpsc::Sender<TrackerCommand>,
    pub event_store: std::sync::Arc<EventStore>,
    pub ergo_scanner: std::sync::Arc<Mutex<basis_store::ergo_scanner::ServerState>>,
    pub reserve_tracker: std::sync::Arc<Mutex<basis_store::ReserveTracker>>,
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
    GetNotesByRecipient {
        recipient_pubkey: basis_store::PubKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<Vec<basis_store::IouNote>, basis_store::NoteError>>,
    },
    GetNoteByIssuerAndRecipient {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<Option<basis_store::IouNote>, basis_store::NoteError>,
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
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::RedemptionError>>,
    },
}