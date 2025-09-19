use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use basis_store::{IouNote, NoteError, PubKey, Signature};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod reserve_api;

// Application state that holds a channel to communicate with the tracker thread
#[derive(Clone)]
struct AppState {
    tx: tokio::sync::mpsc::Sender<TrackerCommand>,
}

// Commands that can be sent to the tracker thread
enum TrackerCommand {
    AddNote {
        issuer_pubkey: PubKey,
        note: IouNote,
        response_tx: tokio::sync::oneshot::Sender<Result<(), NoteError>>,
    },
    GetNotesByIssuer {
        issuer_pubkey: PubKey,
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<IouNote>, NoteError>>,
    },
    GetNoteByIssuerAndRecipient {
        issuer_pubkey: PubKey,
        recipient_pubkey: PubKey,
        response_tx: tokio::sync::oneshot::Sender<Result<Option<IouNote>, NoteError>>,
    },
    GetAllEventsPaginated {
        page: usize,
        page_size: usize,
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<TrackerEvent>, NoteError>>,
    },
}

// Request structure for creating a new IOU note
// Using Vec<u8> for arrays since fixed-size arrays don't implement Deserialize
#[derive(Debug, Deserialize)]
struct CreateNoteRequest {
    recipient_pubkey: Vec<u8>,
    amount: u64,
    timestamp: u64,
    signature: Vec<u8>,
    issuer_pubkey: Vec<u8>,
}

// Response structure for API responses
#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
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
struct SerializableIouNote {
    recipient_pubkey: String,
    amount: u64,
    timestamp: u64,
    signature: String,
}

impl From<IouNote> for SerializableIouNote {
    fn from(note: IouNote) -> Self {
        Self {
            recipient_pubkey: hex::encode(note.recipient_pubkey),
            amount: note.amount,
            timestamp: note.timestamp,
            signature: hex::encode(note.signature),
        }
    }
}

// Success response helper
fn success_response<T>(data: T) -> ApiResponse<T> {
    ApiResponse {
        success: true,
        data: Some(data),
        error: None,
    }
}

// Error response helper
fn error_response<T>(message: String) -> ApiResponse<T> {
    ApiResponse {
        success: false,
        data: None,
        error: Some(message),
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "basis_server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create channel for communicating with tracker thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);

    // Spawn tracker thread (using tokio::task::spawn_blocking for CPU-bound work)
    tokio::task::spawn_blocking(move || {
        use basis_store::TrackerStateManager;

        let mut tracker = TrackerStateManager::new();

        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                TrackerCommand::AddNote {
                    issuer_pubkey,
                    note,
                    response_tx,
                } => {
                    let result = tracker.add_note(&issuer_pubkey, &note);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNotesByIssuer {
                    issuer_pubkey,
                    response_tx,
                } => {
                    let result = tracker.get_issuer_notes(&issuer_pubkey);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNoteByIssuerAndRecipient {
                    issuer_pubkey,
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = tracker
                        .lookup_note(&issuer_pubkey, &recipient_pubkey)
                        .map(Some);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetAllEventsPaginated {
                    page,
                    page_size,
                    response_tx,
                } => {
                    let result = tracker.get_all_notes()
                        .map(|notes| {
                            // Convert notes to events and add other event types
                            let mut events = Vec::new();
                            
                            // Add note events
                            for note in &notes {
                                events.push(TrackerEvent {
                                    event_type: EventType::NoteUpdated,
                                    timestamp: note.timestamp,
                                    issuer_pubkey: None, // Will be filled from context
                                    recipient_pubkey: Some(hex::encode(note.recipient_pubkey)),
                                    amount: Some(note.amount),
                                    reserve_box_id: None,
                                    collateral_amount: None,
                                    redeemed_amount: None,
                                    height: None,
                                });
                            }
                            
                            // Add mock reserve events for demonstration
                            events.push(TrackerEvent {
                                event_type: EventType::ReserveCreated,
                                timestamp: 1234567890,
                                issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
                                recipient_pubkey: None,
                                amount: None,
                                reserve_box_id: Some("box1234567890abcdef".to_string()),
                                collateral_amount: Some(1000000000),
                                redeemed_amount: None,
                                height: Some(1000),
                            });
                            
                            events.push(TrackerEvent {
                                event_type: EventType::ReserveToppedUp,
                                timestamp: 1234567891,
                                issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
                                recipient_pubkey: None,
                                amount: None,
                                reserve_box_id: Some("box1234567890abcdef".to_string()),
                                collateral_amount: Some(500000000),
                                redeemed_amount: None,
                                height: Some(1001),
                            });
                            
                            events.push(TrackerEvent {
                                event_type: EventType::CollateralAlert { ratio: 0.8 },
                                timestamp: 1234567892,
                                issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
                                recipient_pubkey: None,
                                amount: None,
                                reserve_box_id: None,
                                collateral_amount: None,
                                redeemed_amount: None,
                                height: None,
                            });
                            
                            // Add NoteUpdated event
                            events.push(TrackerEvent {
                                event_type: EventType::NoteUpdated,
                                timestamp: 1234567893,
                                issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
                                recipient_pubkey: Some("020202020202020202020202020202020202020202020202020202020202020202".to_string()),
                                amount: Some(1500), // Updated amount
                                reserve_box_id: None,
                                collateral_amount: None,
                                redeemed_amount: None,
                                height: None,
                            });
                            
                            // Add ReserveRedeemed event
                            events.push(TrackerEvent {
                                event_type: EventType::ReserveRedeemed,
                                timestamp: 1234567894,
                                issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
                                recipient_pubkey: None,
                                amount: None,
                                reserve_box_id: Some("box1234567890abcdef".to_string()),
                                collateral_amount: None,
                                redeemed_amount: Some(250000000),
                                height: Some(1002),
                            });
                            
                            // Add Commitment event
                            events.push(TrackerEvent {
                                event_type: EventType::Commitment,
                                timestamp: 1234567895,
                                issuer_pubkey: None,
                                recipient_pubkey: None,
                                amount: None,
                                reserve_box_id: None,
                                collateral_amount: None,
                                redeemed_amount: None,
                                height: Some(1003),
                            });
                            
                            // Apply pagination
                            let start = page * page_size;
                            let end = std::cmp::min(start + page_size, events.len());
                            events[start..end].to_vec()
                        });
                    let _ = response_tx.send(result);
                }
            }
        }
    });

    let app_state = AppState { tx };

    // Build our application with routes
    let app = Router::new()
        .route("/", get(root))
        .route("/notes", post(create_note))
        .route("/notes/issuer/{pubkey}", get(get_notes_by_issuer))
        .route(
            "/notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}",
            get(get_note_by_issuer_and_recipient),
        )
        .route(
            "/reserves/issuer/{pubkey}",
            get(reserve_api::get_reserves_by_issuer),
        )
        .route("/events", get(get_events_paginated))
        .with_state(app_state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    tracing::debug!("Registered routes:");
    tracing::debug!("  GET /");
    tracing::debug!("  POST /notes");
    tracing::debug!("  GET /notes/issuer/{{pubkey}}");
    tracing::debug!("  GET /notes/issuer/{{issuer_pubkey}}/recipient/{{recipient_pubkey}}");
    tracing::debug!("  GET /reserves/issuer/{{pubkey}}");
    tracing::debug!("  GET /events (Paginated tracker events)");

    // Run our app with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, Basis Tracker API!"
}

// Create a new IOU note
#[axum::debug_handler]
async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNoteRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    tracing::debug!("Creating new note: {:?}", payload);

    // Validate and convert Vec<u8> to fixed-size arrays
    let recipient_pubkey: PubKey = match payload.recipient_pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(
                    "recipient_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    let signature: Signature = match payload.signature.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("signature must be 64 bytes".to_string())),
            )
        }
    };

    let issuer_pubkey: PubKey = match payload.issuer_pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("issuer_pubkey must be 33 bytes".to_string())),
            )
        }
    };

    // Create the IOU note
    let note = IouNote::new(
        recipient_pubkey,
        payload.amount,
        payload.timestamp,
        signature,
    );

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(_) = state
        .tx
        .send(TrackerCommand::AddNote {
            issuer_pubkey,
            note,
            response_tx,
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(())) => {
            tracing::info!(
                "Successfully created note from {} to {}",
                hex::encode(&issuer_pubkey),
                hex::encode(&recipient_pubkey)
            );
            (StatusCode::CREATED, Json(success_response(())))
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to create note: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
            };
            (StatusCode::BAD_REQUEST, Json(error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response("Internal server error".to_string())),
            )
        }
    }
}

// Get notes by issuer public key
#[axum::debug_handler]
async fn get_notes_by_issuer(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<SerializableIouNote>>>) {
    tracing::debug!("Getting notes for issuer: {}", pubkey_hex);

    // Decode hex string to bytes
    let issuer_pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("Invalid hex encoding".to_string())),
            )
        }
    };

    // Convert to fixed-size array
    let issuer_pubkey: PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("issuer_pubkey must be 33 bytes".to_string())),
            )
        }
    };

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(_) = state
        .tx
        .send(TrackerCommand::GetNotesByIssuer {
            issuer_pubkey,
            response_tx,
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(notes)) => {
            tracing::info!(
                "Successfully retrieved {} notes for issuer {}",
                notes.len(),
                pubkey_hex
            );
            // Convert to serializable format
            let serializable_notes: Vec<SerializableIouNote> =
                notes.into_iter().map(SerializableIouNote::from).collect();
            (StatusCode::OK, Json(success_response(serializable_notes)))
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get notes: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
            };
            (StatusCode::BAD_REQUEST, Json(error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response("Internal server error".to_string())),
            )
        }
    }
}
// Get a specific note by issuer and recipient public keys
#[axum::debug_handler]
async fn get_note_by_issuer_and_recipient(
    State(state): State<AppState>,
    axum::extract::Path((issuer_pubkey_hex, recipient_pubkey_hex)): axum::extract::Path<(
        String,
        String,
    )>,
) -> (StatusCode, Json<ApiResponse<Option<SerializableIouNote>>>) {
    tracing::debug!(
        "Getting note for issuer: {} and recipient: {}",
        issuer_pubkey_hex,
        recipient_pubkey_hex
    );

    // Decode hex strings to bytes
    let issuer_pubkey_bytes = match hex::decode(&issuer_pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(
                    "Invalid hex encoding for issuer public key".to_string(),
                )),
            )
        }
    };

    let recipient_pubkey_bytes = match hex::decode(&recipient_pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(
                    "Invalid hex encoding for recipient public key".to_string(),
                )),
            )
        }
    };

    // Convert to fixed-size arrays
    let issuer_pubkey: PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("issuer_pubkey must be 33 bytes".to_string())),
            )
        }
    };

    let recipient_pubkey: PubKey = match recipient_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response(
                    "recipient_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(_) = state
        .tx
        .send(TrackerCommand::GetNoteByIssuerAndRecipient {
            issuer_pubkey,
            recipient_pubkey,
            response_tx,
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(Some(note))) => {
            tracing::info!(
                "Successfully retrieved note from {} to {}",
                issuer_pubkey_hex,
                recipient_pubkey_hex
            );
            // Convert to serializable format
            let serializable_note = SerializableIouNote::from(note);
            (
                StatusCode::OK,
                Json(success_response(Some(serializable_note))),
            )
        }
        Ok(Ok(None)) => {
            tracing::info!(
                "No note found from {} to {}",
                issuer_pubkey_hex,
                recipient_pubkey_hex
            );
            (StatusCode::NOT_FOUND, Json(success_response(None)))
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get note: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
            };
            (StatusCode::BAD_REQUEST, Json(error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response("Internal server error".to_string())),
            )
        }
    }
}

// Get paginated tracker events (notes)
#[axum::debug_handler]
async fn get_events_paginated(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, Json<ApiResponse<Vec<TrackerEvent>>>) {
    tracing::debug!("Getting paginated events: {:?}", params);

    // Parse pagination parameters with defaults
    let page = params.get("page").and_then(|p| p.parse().ok()).unwrap_or(0);
    let page_size = params.get("page_size").and_then(|ps| ps.parse().ok()).unwrap_or(20);

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(_) = state
        .tx
        .send(TrackerCommand::GetAllEventsPaginated {
            page,
            page_size,
            response_tx,
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(events)) => {
            tracing::info!(
                "Successfully retrieved {} events for page {} (size: {})",
                events.len(),
                page,
                page_size
            );
            (StatusCode::OK, Json(success_response(events)))
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get paginated events: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
            };
            (StatusCode::BAD_REQUEST, Json(error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_response("Internal server error".to_string())),
            )
        }
    }
}
