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
            std::env::var("RUST_LOG").unwrap_or_else(|_| "basis_server=debug,tower_http=debug".into()),
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
                TrackerCommand::AddNote { issuer_pubkey, note, response_tx } => {
                    let result = tracker.add_note(&issuer_pubkey, &note);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNotesByIssuer { issuer_pubkey, response_tx } => {
                    let result = tracker.get_issuer_notes(&issuer_pubkey);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNoteByIssuerAndRecipient { issuer_pubkey, recipient_pubkey, response_tx } => {
                    let result = tracker.lookup_note(&issuer_pubkey, &recipient_pubkey).map(Some);
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
        .route("/notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}", get(get_note_by_issuer_and_recipient))
        .with_state(app_state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

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
                Json(error_response("recipient_pubkey must be 33 bytes".to_string())),
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
    
    if let Err(_) = state.tx.send(TrackerCommand::AddNote {
        issuer_pubkey,
        note,
        response_tx,
    }).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }
    
    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(())) => {
            tracing::info!("Successfully created note from {} to {}", 
                hex::encode(&issuer_pubkey),
                hex::encode(&recipient_pubkey)
            );
            (
                StatusCode::CREATED,
                Json(success_response(())),
            )
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
            (
                StatusCode::BAD_REQUEST,
                Json(error_response(error_message)),
            )
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
    
    if let Err(_) = state.tx.send(TrackerCommand::GetNotesByIssuer {
        issuer_pubkey,
        response_tx,
    }).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }
    
    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(notes)) => {
            tracing::info!("Successfully retrieved {} notes for issuer {}", 
                notes.len(),
                pubkey_hex
            );
            // Convert to serializable format
            let serializable_notes: Vec<SerializableIouNote> = 
                notes.into_iter().map(SerializableIouNote::from).collect();
            (
                StatusCode::OK,
                Json(success_response(serializable_notes)),
            )
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
            (
                StatusCode::BAD_REQUEST,
                Json(error_response(error_message)),
            )
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
    axum::extract::Path((issuer_pubkey_hex, recipient_pubkey_hex)): axum::extract::Path<(String, String)>,
) -> (StatusCode, Json<ApiResponse<Option<SerializableIouNote>>>) {
    tracing::debug!("Getting note for issuer: {} and recipient: {}", issuer_pubkey_hex, recipient_pubkey_hex);
    
    // Decode hex strings to bytes
    let issuer_pubkey_bytes = match hex::decode(&issuer_pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("Invalid hex encoding for issuer public key".to_string())),
            )
        }
    };
    
    let recipient_pubkey_bytes = match hex::decode(&recipient_pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_response("Invalid hex encoding for recipient public key".to_string())),
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
                Json(error_response("recipient_pubkey must be 33 bytes".to_string())),
            )
        }
    };
    
    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    
    if let Err(_) = state.tx.send(TrackerCommand::GetNoteByIssuerAndRecipient {
        issuer_pubkey,
        recipient_pubkey,
        response_tx,
    }).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_response("Tracker thread unavailable".to_string())),
        );
    }
    
    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(Some(note))) => {
            tracing::info!("Successfully retrieved note from {} to {}", 
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
            tracing::info!("No note found from {} to {}", 
                issuer_pubkey_hex,
                recipient_pubkey_hex
            );
            (
                StatusCode::NOT_FOUND,
                Json(success_response(None)),
            )
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
            (
                StatusCode::BAD_REQUEST,
                Json(error_response(error_message)),
            )
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
