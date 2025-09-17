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

// Success response helper
fn success_response<T>(data: T) -> ApiResponse<T> {
    ApiResponse {
        success: true,
        data: Some(data),
        error: None,
    }
}

// Error response helper
fn error_response(message: String) -> ApiResponse<()> {
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
            }
        }
    });

    let app_state = AppState { tx };

    // Build our application with routes
    let app = Router::new()
        .route("/", get(root))
        .route("/notes", post(create_note))
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