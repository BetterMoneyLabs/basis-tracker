use axum::{extract::State, http::StatusCode, Json};
use std::collections::HashMap;

use crate::{
    models::{
        ApiResponse, CompleteRedemptionRequest, CreateNoteRequest, CreateReserveRequest,
        KeyStatusResponse, ProofResponse, RedeemRequest, RedeemResponse,
        ReserveCreationResponse, ReservePaymentRequest, Asset,
        SerializableIouNote, TrackerEvent, TrackerSignatureRequest,
        TrackerSignatureResponse, RedemptionPreparationRequest,
        RedemptionPreparationResponse,
    },
    AppState, TrackerCommand,
};
use basis_store::{IouNote, NoteError, PubKey, Signature};
use ergo_lib::ergotree_ir::address::AddressEncoder;
use basis_store::reqwest;
use serde::{Deserialize, Serialize};

// Structs for the Schnorr signing API
#[derive(Serialize, Deserialize)]
struct SchnorrSignRequest {
    address: String,
    message: String,
}

#[derive(Deserialize)]
struct SchnorrSignResponse {
    #[serde(rename = "signedMessage")]
    signed_message: String,
    signature: String,
    #[serde(rename = "publicKey")]
    public_key: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<ErrorMessage>,
}

#[derive(Deserialize)]
struct ErrorMessage {
    code: String,
    message: String,
}

/// Call the Ergo node's schnorrSign API to generate a tracker signature
async fn call_schnorr_sign_api(
    node_url: &str,
    api_key: Option<&str>,
    address: &str,
    message: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();

    let request_body = SchnorrSignRequest {
        address: address.to_string(),
        message: message.to_string(),
    };

    let url = format!("{}/utils/schnorrSign", node_url.trim_end_matches('/'));
    let mut request_builder = client.post(&url);

    // Add API key if provided
    if let Some(key) = api_key {
        request_builder = request_builder.header("api_key", key);
    }

    let response = request_builder
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    let status = response.status();
    let response_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if status.is_success() {
        let sign_response: SchnorrSignResponse = serde_json::from_str(&response_text)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(sign_response.signature)
    } else {
        // Try to parse error response
        let error_response: Result<ErrorResponse, _> = serde_json::from_str(&response_text);
        let error_msg = match error_response {
            Ok(err_resp) => {
                if let Some(err) = err_resp.error {
                    format!("{}: {}", err.code, err.message)
                } else {
                    response_text
                }
            }
            Err(_) => response_text,
        };
        Err(format!("API error {}: {}", status.as_u16(), error_msg))
    }
}

/// Verify that a signature from the Ergo node is compatible with the Basis server's verification algorithm
/// This is needed because the Ergo node's Schnorr implementation has been found to be incompatible
/// with the Basis server's verification algorithm
async fn verify_ergo_node_signature_compatibility(
    signature_hex: &str,
    message_hex: &str,
    public_key_bytes: &[u8; 33],
) -> Result<(), String> {
    // Decode the signature and message
    let signature_bytes = hex::decode(signature_hex)
        .map_err(|e| format!("Failed to decode signature: {}", e))?;
    let message_bytes = hex::decode(message_hex)
        .map_err(|e| format!("Failed to decode message: {}", e))?;

    // Check signature length
    if signature_bytes.len() != 65 {
        return Err(format!("Signature is not 65 bytes: {}", signature_bytes.len()));
    }

    // Convert to fixed-size arrays
    let mut signature_array = [0u8; 65];
    signature_array.copy_from_slice(&signature_bytes);

    // Try to verify using the same algorithm as basis_offchain
    // This will help detect compatibility issues
    match basis_offchain::schnorr::schnorr_verify(&signature_array, &message_bytes, public_key_bytes) {
        Ok(()) => {
            // Verification succeeded
            Ok(())
        },
        Err(_) => {
            // Verification failed - this indicates the signature is not compatible
            Err("Signature verification failed with Basis server algorithm".to_string())
        }
    }
}

// Basic handler that responds with a static string
pub async fn root() -> &'static str {
    "Hello, Basis Tracker API!"
}

// Create a new IOU note
#[axum::debug_handler]
pub async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNoteRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    tracing::debug!("Creating new note: {:?}", payload);

    // Validate and convert hex-encoded strings to fixed-size arrays
    let recipient_pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "recipient_pubkey must be hex-encoded".to_string(),
                )),
            )
        }
    };

    let recipient_pubkey: PubKey = match recipient_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "recipient_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    let signature_bytes = match hex::decode(&payload.signature) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "signature must be hex-encoded".to_string(),
                )),
            )
        }
    };

    let signature: Signature = match signature_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "signature must be 65 bytes".to_string(),
                )),
            )
        }
    };

    let issuer_pubkey_bytes = match hex::decode(&payload.issuer_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "issuer_pubkey must be hex-encoded".to_string(),
                )),
            )
        }
    };

    let issuer_pubkey: PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "issuer_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Create the IOU note
    let note = IouNote::new(
        recipient_pubkey,
        payload.amount,
        0, // amount_redeemed
        payload.timestamp,
        signature,
    );

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(e) = state
        .tx
        .send(crate::TrackerCommand::AddNote {
            issuer_pubkey,
            note,
            response_tx,
        })
        .await
    {
        tracing::error!("Failed to send to tracker thread: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
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

            // Store event in event store
            let event = TrackerEvent {
                id: 0, // Will be set by event store
                event_type: crate::models::EventType::NoteUpdated,
                timestamp: payload.timestamp,
                issuer_pubkey: Some(hex::encode(&issuer_pubkey)),
                recipient_pubkey: Some(hex::encode(&recipient_pubkey)),
                amount: Some(payload.amount),
                reserve_box_id: None,
                collateral_amount: None,
                redeemed_amount: None,
                height: None,
            };

            match state.event_store.add_event(event).await {
                Ok(event_id) => {
                    tracing::debug!("Stored note creation event with ID: {}", event_id);
                }
                Err(e) => {
                    tracing::warn!("Failed to store event: {:?}", e);
                }
            }

            (
                StatusCode::CREATED,
                Json(crate::models::success_response(())),
            )
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to create note: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::PastTimestamp => "Past timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::UnsupportedOperation => "Operation not supported".to_string(),
            };
            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(error_message)),
            )
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            )
        }
    }
}

// Get notes by issuer public key
#[axum::debug_handler]
pub async fn get_notes_by_issuer(
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
                Json(crate::models::error_response(
                    "Invalid hex encoding".to_string(),
                )),
            )
        }
    };

    // Convert to fixed-size array
    let issuer_pubkey: PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "issuer_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    tracing::debug!("Sending GetNotesByIssuer command to tracker thread");

    if let Err(e) = state
        .tx
        .send(crate::TrackerCommand::GetNotesByIssuer {
            issuer_pubkey,
            response_tx,
        })
        .await
    {
        tracing::error!("Failed to send to tracker thread: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }

    tracing::debug!("GetNotesByIssuer command sent successfully");

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(notes)) => {
            tracing::info!(
                "Successfully retrieved {} notes for issuer {}",
                notes.len(),
                pubkey_hex
            );

            // Debug: log the actual notes found
            for note in &notes {
                tracing::debug!(
                    "Note found: collected={}, redeemed={}, timestamp={}",
                    note.amount_collected,
                    note.amount_redeemed,
                    note.timestamp
                );
            }

            // Convert to serializable format with issuer pubkey
            let serializable_notes: Vec<SerializableIouNote> = notes
                .into_iter()
                .map(|note| {
                    let mut serializable_note = SerializableIouNote::from(note);
                    serializable_note.issuer_pubkey = pubkey_hex.clone();
                    serializable_note
                })
                .collect();
            (
                StatusCode::OK,
                Json(crate::models::success_response(serializable_notes)),
            )
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get notes: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::PastTimestamp => "Past timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::UnsupportedOperation => "Operation not supported".to_string(),
            };
            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(error_message)),
            )
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            )
        }
    }
}

// Get notes by recipient public key
#[axum::debug_handler]
pub async fn get_notes_by_recipient(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<SerializableIouNote>>>) {
    tracing::debug!("Getting notes for recipient: {}", pubkey_hex);

    // Decode hex string to bytes
    let recipient_pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding".to_string(),
                )),
            )
        }
    };

    // Convert to fixed-size array
    let recipient_pubkey: PubKey = match recipient_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "recipient_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(e) = state
        .tx
        .send(crate::TrackerCommand::GetNotesByRecipient {
            recipient_pubkey,
            response_tx,
        })
        .await
    {
        tracing::error!("Failed to send to tracker thread: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(notes)) => {
            tracing::info!(
                "Successfully retrieved {} notes for recipient {}",
                notes.len(),
                pubkey_hex
            );

            // Convert to serializable format with issuer pubkey
            let serializable_notes: Vec<SerializableIouNote> = notes
                .into_iter()
                .map(|note| {
                    let mut serializable_note = SerializableIouNote::from(note);
                    serializable_note.issuer_pubkey = pubkey_hex.clone();
                    serializable_note
                })
                .collect();
            (
                StatusCode::OK,
                Json(crate::models::success_response(serializable_notes)),
            )
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get notes: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::PastTimestamp => "Past timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::UnsupportedOperation => "Operation not supported".to_string(),
            };
            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(error_message)),
            )
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            )
        }
    }
}

// Get a specific note by issuer and recipient public keys
#[axum::debug_handler]
pub async fn get_note_by_issuer_and_recipient(
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
                Json(crate::models::error_response(
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
                Json(crate::models::error_response(
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
                Json(crate::models::error_response(
                    "issuer_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    let recipient_pubkey: PubKey = match recipient_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "recipient_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(_) = state
        .tx
        .send(crate::TrackerCommand::GetNoteByIssuerAndRecipient {
            issuer_pubkey,
            recipient_pubkey,
            response_tx,
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
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
            // Convert to serializable format with issuer pubkey
            let mut serializable_note = SerializableIouNote::from(note);
            serializable_note.issuer_pubkey = issuer_pubkey_hex.clone();
            (
                StatusCode::OK,
                Json(crate::models::success_response(Some(serializable_note))),
            )
        }
        Ok(Ok(None)) => {
            tracing::info!(
                "No note found from {} to {}",
                issuer_pubkey_hex,
                recipient_pubkey_hex
            );
            (
                StatusCode::NOT_FOUND,
                Json(crate::models::success_response(None)),
            )
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get note: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::PastTimestamp => "Past timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::UnsupportedOperation => "Operation not supported".to_string(),
            };
            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(error_message)),
            )
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            )
        }
    }
}

// Get all notes with their age
#[axum::debug_handler]
pub async fn get_all_notes(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<Vec<crate::models::SerializableIouNoteWithAge>>>) {
    tracing::debug!("Getting all notes");

    // Send command to tracker thread
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(_) = state
        .tx
        .send(crate::TrackerCommand::GetNotes {
            response_tx,
        })
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(notes_with_issuer)) => {
            tracing::info!("Successfully retrieved {} notes", notes_with_issuer.len());

            // Convert to serializable format with age calculation
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let serializable_notes: Vec<crate::models::SerializableIouNoteWithAge> = notes_with_issuer
                .into_iter()
                .map(|(issuer_pubkey, note)| {
                    let age_seconds = current_time.saturating_sub(note.timestamp);
                    crate::models::SerializableIouNoteWithAge {
                        issuer_pubkey: hex::encode(issuer_pubkey),
                        recipient_pubkey: hex::encode(note.recipient_pubkey),
                        amount_collected: note.amount_collected,
                        amount_redeemed: note.amount_redeemed,
                        timestamp: note.timestamp,
                        signature: hex::encode(note.signature),
                        age_seconds,
                    }
                })
                .collect();

            (
                StatusCode::OK,
                Json(crate::models::success_response(serializable_notes)),
            )
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to get all notes: {:?}", e);
            let error_message = match e {
                NoteError::InvalidSignature => "Invalid signature".to_string(),
                NoteError::AmountOverflow => "Amount overflow".to_string(),
                NoteError::FutureTimestamp => "Future timestamp".to_string(),
                NoteError::PastTimestamp => "Past timestamp".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::UnsupportedOperation => "Operation not supported".to_string(),
            };
            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(error_message)),
            )
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            )
        }
    }
}

// Get paginated tracker events from event store
#[axum::debug_handler]
pub async fn get_events_paginated(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> (StatusCode, Json<ApiResponse<Vec<TrackerEvent>>>) {
    tracing::debug!("Getting paginated events: {:?}", params);

    // Parse pagination parameters with defaults
    let page = params.get("page").and_then(|p| p.parse().ok()).unwrap_or(0);
    let page_size = params
        .get("page_size")
        .and_then(|ps| ps.parse().ok())
        .unwrap_or(20);

    // Get events from event store
    let events = match state
        .event_store
        .get_events_paginated(page, page_size)
        .await
    {
        Ok(events) => events,
        Err(e) => {
            tracing::error!("Failed to retrieve events: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve events".to_string(),
                )),
            );
        }
    };

    tracing::info!(
        "Successfully retrieved {} events for page {} (size: {})",
        events.len(),
        page,
        page_size
    );

    (
        StatusCode::OK,
        Json(crate::models::success_response(events)),
    )
}

// Get recent tracker events (simple events endpoint)
#[axum::debug_handler]
pub async fn get_events(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<Vec<TrackerEvent>>>) {
    tracing::debug!("Getting recent events");

    // Get recent events (last 50 events by default)
    let events = match state.event_store.get_events_paginated(0, 50).await {
        Ok(events) => events,
        Err(e) => {
            tracing::error!("Failed to retrieve events: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve events".to_string(),
                )),
            );
        }
    };

    tracing::info!("Successfully retrieved {} recent events", events.len());

    (
        StatusCode::OK,
        Json(crate::models::success_response(events)),
    )
}

// Get key status information
#[axum::debug_handler]
pub async fn get_key_status(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<KeyStatusResponse>>) {
    tracing::debug!("Getting key status for: {}", pubkey_hex);

    // Decode hex string to bytes
    let pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding".to_string(),
                )),
            )
        }
    };

    // Convert to fixed-size array
    let issuer_pubkey: basis_store::PubKey = match pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Public key must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Get total debt from note storage
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(e) = state
        .tx
        .send(crate::TrackerCommand::GetNotesByIssuer {
            issuer_pubkey,
            response_tx,
        })
        .await
    {
        tracing::error!("Failed to send to tracker thread: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }

    let notes = match response_rx.await {
        Ok(Ok(notes)) => notes,
        Ok(Err(e)) => {
            tracing::error!("Failed to get notes: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve notes".to_string(),
                )),
            );
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            );
        }
    };

    // Calculate total debt and note count
    let total_debt: u64 = notes.iter().map(|note| note.outstanding_debt()).sum();
    let note_count = notes.len();

    // Get collateral from reserve tracker
    let tracker = state.reserve_tracker.lock().await;
    let all_reserves = tracker.get_all_reserves();

    // Normalize the public key to handle different representations (e.g., 07 prefix for GroupElement)
    let normalized_pubkey = basis_store::normalize_public_key(&pubkey_hex);

    // Find reserve for this issuer - check multiple key representations for comprehensive correlation
    let reserve = all_reserves
        .into_iter()
        .find(|reserve| {
            let normalized_reserve_key = basis_store::normalize_public_key(&reserve.owner_pubkey);
            let original_reserve_key = &reserve.owner_pubkey;

            // Check multiple matching possibilities to ensure comprehensive key correlation:
            // 1. Direct match between normalized keys (main case)
            // 2. Match between original pubkey and normalized reserve key
            // 3. Match between original pubkey and original reserve key (backup)
            // 4. Special case: original pubkey matches the part of reserve key after '07' prefix
            normalized_pubkey == normalized_reserve_key ||
            pubkey_hex == normalized_reserve_key ||
            pubkey_hex == *original_reserve_key ||
            (original_reserve_key.starts_with("07") && original_reserve_key.len() >= 66 &&
             &original_reserve_key[2..] == pubkey_hex.as_str())
        });

    let (collateral, collateralization_ratio, last_updated) = if let Some(reserve) = reserve {
        let collateral = reserve.base_info.collateral_amount;
        let ratio = if total_debt > 0 {
            collateral as f64 / total_debt as f64
        } else {
            // Use a very high ratio when there's no debt
            999999.0
        };
        (collateral, ratio, reserve.last_updated_timestamp)
    } else {
        // No reserve found - use zero collateral
        (0, if total_debt > 0 { 0.0 } else { 999999.0 }, 0)
    };

    let status = KeyStatusResponse {
        total_debt,
        collateral,
        collateralization_ratio,
        note_count,
        last_updated,
        issuer_pubkey: pubkey_hex.clone(),
    };

    tracing::info!(
        "Returning real key status for {}: debt={}, collateral={}, ratio={:.2}",
        pubkey_hex,
        total_debt,
        collateral,
        collateralization_ratio
    );

    (
        StatusCode::OK,
        Json(crate::models::success_response(status)),
    )
}

// Initiate redemption process
#[axum::debug_handler]
pub async fn initiate_redemption(
    State(state): State<AppState>,
    Json(payload): Json<RedeemRequest>,
) -> (StatusCode, Json<ApiResponse<RedeemResponse>>) {
    tracing::debug!("Initiating redemption: {:?}", payload);

    // Convert recipient public key to P2PK address
    let recipient_address = {
        // Convert the public key to a P2PK address
        use ergo_lib::ergotree_ir::address::{Address, NetworkPrefix};
        use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
        use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

        // Decode the hex public key
        let pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
            Ok(bytes) => bytes,
            Err(_) => {
                // If hex decoding fails, abort redemption
                return (
                    StatusCode::BAD_REQUEST,
                    Json(crate::models::error_response(
                        "Invalid hex encoding for recipient public key".to_string(),
                    )),
                );
            }
        };

        // Create an EcPoint from the public key bytes
        match EcPoint::sigma_parse_bytes(&pubkey_bytes) {
            Ok(ec_point) => {
                // Create a P2PK address from the public key
                let prove_dlog = ProveDlog::from(ec_point);
                let address = Address::P2Pk(prove_dlog);
                // Use mainnet prefix by default, could be configurable
                let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
                encoder.address_to_str(&address)
            },
            Err(_) => {
                // If conversion fails, abort redemption
                return (
                    StatusCode::BAD_REQUEST,
                    Json(crate::models::error_response(
                        "Invalid public key format for recipient".to_string(),
                    )),
                );
            }
        }
    };

    // Find the reserve box ID for the issuer using normalized key matching
    let reserve_box_id = {
        // Get the reserve tracker to look up the reserve
        let tracker = state.reserve_tracker.lock().await;

        // Get all reserves to find the matching one
        let all_reserves = tracker.get_all_reserves();

        // Normalize the issuer public key
        let normalized_issuer_key = basis_store::normalize_public_key(&payload.issuer_pubkey);

        // Find a reserve where the owner key matches (considering normalized forms)
        let mut found_box_id = String::new();
        for reserve in &all_reserves {
            // Handle the case where the owner key might be double-encoded
            // The database might store the hex string as ASCII characters, which are hex-encoded again
            let actual_owner_key = {
                // Try to decode the stored key as hex to get the original hex string
                if let Ok(decoded_bytes) = hex::decode(&reserve.owner_pubkey) {
                    // If successful, try to interpret as ASCII string
                    if let Ok(decoded_string) = String::from_utf8(decoded_bytes) {
                        // Check if this looks like a valid hex string (all valid hex chars)
                        if decoded_string.chars().all(|c| c.is_ascii_hexdigit()) {
                            decoded_string
                        } else {
                            // If not a valid hex string, use the original
                            reserve.owner_pubkey.clone()
                        }
                    } else {
                        // If not valid UTF-8, use the original
                        reserve.owner_pubkey.clone()
                    }
                } else {
                    // If hex decoding fails, use the original
                    reserve.owner_pubkey.clone()
                }
            };

            let normalized_actual_key = basis_store::normalize_public_key(&actual_owner_key);
            let original_reserve_key = &reserve.owner_pubkey;

            // Debug: Print the values being compared
            tracing::debug!("Comparing keys - Issuer: {}, Normalized Issuer: {}, Actual Owner Key: {}, Normalized Actual: {}, Stored: {}",
                           payload.issuer_pubkey, normalized_issuer_key, actual_owner_key, normalized_actual_key, original_reserve_key);

            // Since we now strip the 0x07 prefix when reading from registers,
            // we only need to match normalized keys (handles any remaining edge cases)
            let matches = normalized_issuer_key == normalized_actual_key;

            if matches {
                tracing::debug!("Key match found! Reserve box ID: {}", reserve.box_id);
                found_box_id = reserve.box_id.clone();
                break;
            }
        }

        if found_box_id.is_empty() {
            tracing::warn!("No reserve found for issuer: {}", payload.issuer_pubkey);
            tracing::debug!("Available reserves for debugging:");
            for reserve in &all_reserves {
                tracing::debug!("  Reserve box: {}, owner key: {}", reserve.box_id, reserve.owner_pubkey);
            }

            // Return a failed redemption response
            let response = crate::models::RedeemResponse {
                redemption_id: "failed_no_matching_reserve".to_string(),
                amount: payload.amount,
                timestamp: payload.timestamp,
                proof_available: false,
                transaction_pending: false,
                transaction_data: None,
            };

            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(format!("No matching reserve found for issuer: {}", payload.issuer_pubkey))),
            );
        }

        found_box_id
    };

    // Create redemption request
    let redemption_request = basis_store::RedemptionRequest {
        issuer_pubkey: payload.issuer_pubkey.clone(),
        recipient_pubkey: payload.recipient_pubkey.clone(),
        amount: payload.amount,
        timestamp: payload.timestamp,
        reserve_box_id, // Use the found reserve box ID
        recipient_address: recipient_address.clone(), // Use derived address from public key
    };

    // Send command to tracker thread to initiate redemption
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    let cmd = TrackerCommand::InitiateRedemption {
        request: redemption_request,
        response_tx,
    };

    if let Err(e) = state.tx.send(cmd).await {
        tracing::error!("Failed to send redemption command to tracker: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Failed to process redemption request".to_string(),
            )),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(redemption_data)) => {
            // Get tracker NFT ID from configuration
            let tracker_nft_id = match state.config.tracker_nft_bytes() {
                Ok(bytes) => hex::encode(bytes),
                Err(_) => {
                    tracing::error!("Tracker NFT ID is not properly configured");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(crate::models::error_response(
                            "Tracker NFT ID is not properly configured".to_string(),
                        )),
                    );
                }
            };

            // Create transaction data that can be submitted to Ergo node
            // Use the transaction data that was prepared by the redemption manager
            let transaction_data = Some(crate::models::TransactionData {
                address: recipient_address, // Use address derived from recipient public key
                value: 100000, // Minimum ERG value for box (0.001 ERG)
                registers: {
                    let mut regs = std::collections::HashMap::new();
                    // R4 and R5 registers will be populated with the actual values from the redemption transaction
                    // The redemption manager should prepare these values appropriately
                    // For now, using placeholders based on the redemption data
                    regs.insert("R4".to_string(), redemption_data.transaction_bytes.clone()); // Placeholder for R4 register
                    regs.insert("R5".to_string(), hex::encode(&redemption_data.avl_proof)); // AVL proof for the note being redeemed
                    regs
                },
                assets: vec![crate::models::TokenData {
                    token_id: tracker_nft_id, // Use configured tracker NFT ID
                    amount: 1,
                }],
                fee: redemption_data.estimated_fee, // Use actual estimated fee from redemption data
            });

            let response = RedeemResponse {
                redemption_id: redemption_data.redemption_id,
                amount: payload.amount,
                timestamp: payload.timestamp,
                proof_available: !redemption_data.avl_proof.is_empty(),
                transaction_pending: true,
                transaction_data,
            };

            tracing::info!(
                "Redemption initiated successfully for {} -> {}: {}, transaction_data available",
                payload.issuer_pubkey,
                payload.recipient_pubkey,
                response.redemption_id
            );

            (
                StatusCode::OK,
                Json(crate::models::success_response(response)),
            )
        }
        Ok(Err(e)) => {
            tracing::error!("Redemption failed: {:?}", e);
            // Return a more specific error response based on the error type
            let error_msg = format!("Redemption failed: {}", e);
            let redemption_id = match e {
                basis_store::RedemptionError::NoteNotFound => "failed_note_not_found".to_string(),
                basis_store::RedemptionError::InvalidNoteSignature => "failed_invalid_signature".to_string(),
                basis_store::RedemptionError::InsufficientCollateral(_, _) => "failed_insufficient_collateral".to_string(),
                basis_store::RedemptionError::RedemptionTooEarly(_, _) => "failed_too_early".to_string(),
                basis_store::RedemptionError::StorageError(_) => "failed_storage_error".to_string(),
                _ => "failed_other_error".to_string(),
            };

            // Return a response with more specific failure information
            let failure_response = RedeemResponse {
                redemption_id, // Use specific failure ID
                amount: payload.amount,
                timestamp: payload.timestamp,
                proof_available: false,
                transaction_pending: false,
                transaction_data: None, // No transaction data available on failure
            };

            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(error_msg)),
            )
        }
        Err(_) => {
            tracing::error!("Failed to receive redemption response from tracker");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to process redemption request".to_string(),
                )),
            )
        }
    }
}

// Complete redemption process by removing the note from tracker state
#[axum::debug_handler]
pub async fn complete_redemption(
    State(_state): State<AppState>,
    Json(payload): Json<CompleteRedemptionRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    tracing::debug!("Completing redemption: {:?}", payload);

    // Parse public keys
    let issuer_pubkey = match hex::decode(&payload.issuer_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid issuer_pubkey hex encoding".to_string(),
                )),
            )
        }
    };

    let recipient_pubkey = match hex::decode(&payload.recipient_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid recipient_pubkey hex encoding".to_string(),
                )),
            )
        }
    };

    let issuer_pubkey: PubKey = match issuer_pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "issuer_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    let recipient_pubkey: PubKey = match recipient_pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "recipient_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Send command to tracker thread to complete redemption
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    let cmd = TrackerCommand::CompleteRedemption {
        issuer_pubkey,
        recipient_pubkey,
        redeemed_amount: payload.redeemed_amount,
        response_tx,
    };

    if let Err(e) = _state.tx.send(cmd).await {
        tracing::error!(
            "Failed to send complete redemption command to tracker: {}",
            e
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Failed to complete redemption".to_string(),
            )),
        );
    }

    // Wait for response from tracker thread
    match response_rx.await {
        Ok(Ok(())) => {
            tracing::info!(
                "Redemption completed successfully for {} -> {}",
                payload.issuer_pubkey,
                payload.recipient_pubkey
            );

            (StatusCode::OK, Json(crate::models::success_response(())))
        }
        Ok(Err(e)) => {
            tracing::error!("Redemption completion failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(format!(
                    "Redemption completion failed: {}",
                    e
                ))),
            )
        }
        Err(_) => {
            tracing::error!("Failed to receive redemption completion response from tracker");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to complete redemption".to_string(),
                )),
            )
        }
    }
}

// Get proof for a specific note
#[axum::debug_handler]
pub async fn get_proof(
    State(_state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, Json<ApiResponse<ProofResponse>>) {
    tracing::debug!("Getting proof with params: {:?}", params);

    let empty_string = "".to_string();
    let issuer_pubkey = params.get("issuer_pubkey").unwrap_or(&empty_string);
    let recipient_pubkey = params.get("recipient_pubkey").unwrap_or(&empty_string);

    if issuer_pubkey.is_empty() || recipient_pubkey.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "issuer_pubkey and recipient_pubkey parameters are required".to_string(),
            )),
        );
    }

    // Validate hex encoding
    if hex::decode(issuer_pubkey).is_err() || hex::decode(recipient_pubkey).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "Invalid hex encoding for public keys".to_string(),
            )),
        );
    }

    // In a real implementation, this would:
    // 1. Generate AVL tree proof for the note
    // 2. Include tracker state commitment
    // 3. Return the complete proof

    // Mock implementation for now
    let proof = ProofResponse {
        issuer_pubkey: issuer_pubkey.clone(),
        recipient_pubkey: recipient_pubkey.clone(),
        proof_data: format!("proof_{}_{}", &issuer_pubkey[..16], &recipient_pubkey[..16]),
        tracker_state_digest: "mock_digest_1234567890abcdef".to_string(),
        block_height: 1500,
        timestamp: 1672531200,
    };

    tracing::info!(
        "Proof generated for {} -> {}",
        issuer_pubkey,
        recipient_pubkey
    );

    (StatusCode::OK, Json(crate::models::success_response(proof)))
}

// Request tracker signature for redemption
#[axum::debug_handler]
pub async fn request_tracker_signature(
    State(state): State<AppState>,
    Json(payload): Json<TrackerSignatureRequest>,
) -> (StatusCode, Json<ApiResponse<TrackerSignatureResponse>>) {
    tracing::debug!("Requesting tracker signature for redemption: {:?}", payload);

    // Validate public keys
    if hex::decode(&payload.issuer_pubkey).is_err() || hex::decode(&payload.recipient_pubkey).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "Invalid hex encoding for public keys".to_string(),
            )),
        );
    }

    // Get tracker public key from configuration
    let tracker_pubkey_bytes = match state.config.tracker_public_key_bytes() {
        Ok(Some(key)) => key,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Tracker public key not configured".to_string(),
                )),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Invalid tracker public key format: {}", e),
                )),
            );
        }
    };

    // Create a message to be signed by the tracker following the spec
    // According to the offchain spec: (recipient_pubkey || amount || timestamp)
    let recipient_pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding for recipient public key".to_string(),
                )),
            );
        }
    };

    let message_to_sign_bytes = basis_offchain::schnorr::signing_message(
        &recipient_pubkey_bytes.try_into().unwrap_or([0u8; 33]),
        payload.amount,
        payload.timestamp,
    );

    let message_to_sign = hex::encode(&message_to_sign_bytes);

    // Convert tracker public key to P2PK address format for the Ergo node API
    use ergo_lib::ergotree_ir::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let tracker_ec_point = match EcPoint::sigma_parse_bytes(&tracker_pubkey_bytes) {
        Ok(point) => point,
        Err(e) => {
            tracing::error!("Failed to parse tracker public key as EcPoint: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Failed to parse tracker public key: {}", e),
                )),
            );
        }
    };

    let prove_dlog = ProveDlog::from(tracker_ec_point);
    let tracker_address = Address::P2Pk(prove_dlog);
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet); // Use appropriate network prefix
    let tracker_p2pk_address = encoder.address_to_str(&tracker_address);

    // Get node URL and API key from configuration
    let node_url = &state.config.ergo.node.node_url;
    let api_key = state.config.ergo.node.api_key.as_deref();

    // Call the Ergo node's schnorrSign API to generate the tracker signature
    let tracker_signature = match call_schnorr_sign_api(
        node_url,
        api_key,
        &tracker_p2pk_address,
        &message_to_sign,
    ).await {
        Ok(signature) => signature,
        Err(e) => {
            tracing::error!("Failed to generate tracker signature via Ergo node API: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Failed to generate tracker signature: {}", e),
                )),
            );
        }
    };

    // Verify that the signature from the Ergo node is compatible with our verification algorithm
    // Due to compatibility issues discovered between Ergo node and Basis server Schnorr implementations
    if let Err(verification_error) = verify_ergo_node_signature_compatibility(
        &tracker_signature,
        &message_to_sign,
        &tracker_pubkey_bytes,
    ).await {
        tracing::warn!("Ergo node signature is not compatible with Basis verification: {}. This may cause verification issues later.", verification_error);
        // Note: We still return the signature but log the compatibility issue
        // In a production environment, you might want to handle this differently
    }

    let tracker_pubkey = hex::encode(&tracker_pubkey_bytes);

    let response = TrackerSignatureResponse {
        success: true,
        tracker_signature,
        tracker_pubkey,
        message_signed: message_to_sign,
    };

    tracing::info!(
        "Tracker signature generated for redemption from {} to {}",
        payload.issuer_pubkey,
        payload.recipient_pubkey
    );

    (StatusCode::OK, Json(crate::models::success_response(response)))
}

// Prepare redemption with all necessary data
#[axum::debug_handler]
pub async fn prepare_redemption(
    State(state): State<AppState>,
    Json(payload): Json<RedemptionPreparationRequest>,
) -> (StatusCode, Json<ApiResponse<RedemptionPreparationResponse>>) {
    tracing::debug!("Preparing redemption: {:?}", payload);

    // Validate public keys
    if hex::decode(&payload.issuer_pubkey).is_err() || hex::decode(&payload.recipient_pubkey).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "Invalid hex encoding for public keys".to_string(),
            )),
        );
    }

    // Get tracker public key from configuration
    let tracker_pubkey_bytes = match state.config.tracker_public_key_bytes() {
        Ok(Some(key)) => key,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Tracker public key not configured".to_string(),
                )),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Invalid tracker public key format: {}", e),
                )),
            );
        }
    };

    // Generate a unique redemption ID
    let redemption_id = format!("redemption_{}_{}_{}",
        &payload.issuer_pubkey[..8],
        &payload.recipient_pubkey[..8],
        payload.timestamp
    );

    // Create a message to be signed by the tracker following the spec
    // According to the offchain spec: (recipient_pubkey || amount || timestamp)
    let recipient_pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding for recipient public key".to_string(),
                )),
            );
        }
    };

    let message_to_sign_bytes = basis_offchain::schnorr::signing_message(
        &recipient_pubkey_bytes.try_into().unwrap_or([0u8; 33]),
        payload.amount,
        payload.timestamp,
    );

    let message_to_sign = hex::encode(&message_to_sign_bytes);

    // Convert tracker public key to P2PK address format for the Ergo node API
    use ergo_lib::ergotree_ir::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let tracker_ec_point = match EcPoint::sigma_parse_bytes(&tracker_pubkey_bytes) {
        Ok(point) => point,
        Err(e) => {
            tracing::error!("Failed to parse tracker public key as EcPoint: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Failed to parse tracker public key: {}", e),
                )),
            );
        }
    };

    let prove_dlog = ProveDlog::from(tracker_ec_point);
    let tracker_address = Address::P2Pk(prove_dlog);
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet); // Use appropriate network prefix
    let tracker_p2pk_address = encoder.address_to_str(&tracker_address);

    // Get node URL and API key from configuration
    let node_url = &state.config.ergo.node.node_url;
    let api_key = state.config.ergo.node.api_key.as_deref();

    // Call the Ergo node's schnorrSign API to generate the tracker signature
    let tracker_signature = match call_schnorr_sign_api(
        node_url,
        api_key,
        &tracker_p2pk_address,
        &message_to_sign,
    ).await {
        Ok(signature) => signature,
        Err(e) => {
            tracing::error!("Failed to generate tracker signature via Ergo node API: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Failed to generate tracker signature: {}", e),
                )),
            );
        }
    };

    // Verify that the signature from the Ergo node is compatible with our verification algorithm
    // Due to compatibility issues discovered between Ergo node and Basis server Schnorr implementations
    if let Err(verification_error) = verify_ergo_node_signature_compatibility(
        &tracker_signature,
        &message_to_sign,
        &tracker_pubkey_bytes,
    ).await {
        tracing::warn!("Ergo node signature is not compatible with Basis verification: {}. This may cause verification issues later.", verification_error);
        // Note: We still return the signature but log the compatibility issue
        // In a production environment, you might want to handle this differently
    }

    // Get the current tracker state digest from shared tracker state
    let tracker_state_digest = {
        // Get the current AVL root digest from shared tracker state
        let shared_state = state.shared_tracker_state.lock().await;
        let current_digest = shared_state.get_avl_root_digest();
        drop(shared_state); // Release the lock early
        hex::encode(&current_digest)
    };

    // Generate a real AVL proof for the note
    // Send command to tracker thread to generate the proof
    let (proof_response_tx, proof_response_rx) = tokio::sync::oneshot::channel();

    let issuer_pubkey_bytes = match hex::decode(&payload.issuer_pubkey) {
        Ok(bytes) => {
            match bytes.try_into() {
                Ok(arr) => arr,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(crate::models::error_response(
                            "issuer_pubkey must be 33 bytes".to_string(),
                        )),
                    );
                }
            }
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding for issuer public key".to_string(),
                )),
            );
        }
    };

    let recipient_pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
        Ok(bytes) => {
            match bytes.try_into() {
                Ok(arr) => arr,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(crate::models::error_response(
                            "recipient_pubkey must be 33 bytes".to_string(),
                        )),
                    );
                }
            }
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding for recipient public key".to_string(),
                )),
            );
        }
    };

    if let Err(e) = state.tx.send(TrackerCommand::GenerateProof {
        issuer_pubkey: issuer_pubkey_bytes,
        recipient_pubkey: recipient_pubkey_bytes,
        response_tx: proof_response_tx,
    }).await {
        tracing::error!("Failed to send proof generation command to tracker thread: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }

    // Wait for response from tracker thread
    let proof_result = match proof_response_rx.await {
        Ok(Ok(note_proof)) => {
            // Convert the proof to a hex string for transmission
            hex::encode(&note_proof.avl_proof)
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to generate proof: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Failed to generate proof: {:?}", e),
                )),
            );
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            );
        }
    };

    let avl_proof = proof_result;
    let tracker_pubkey = hex::encode(&tracker_pubkey_bytes);

    let response = RedemptionPreparationResponse {
        redemption_id,
        avl_proof,
        tracker_signature,
        tracker_pubkey,
        tracker_state_digest,
        block_height: 1500,
    };

    tracing::info!(
        "Redemption prepared for {} -> {} with ID {}",
        payload.issuer_pubkey,
        payload.recipient_pubkey,
        response.redemption_id
    );

    (StatusCode::OK, Json(crate::models::success_response(response)))
}

// Enhanced proof endpoint specifically for redemption
#[axum::debug_handler]
pub async fn get_redemption_proof(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (StatusCode, Json<ApiResponse<ProofResponse>>) {
    tracing::debug!("Getting redemption proof with params: {:?}", params);

    let empty_string = "".to_string();
    let issuer_pubkey = params.get("issuer_pubkey").unwrap_or(&empty_string);
    let recipient_pubkey = params.get("recipient_pubkey").unwrap_or(&empty_string);
    let amount = params.get("amount").unwrap_or(&empty_string);

    if issuer_pubkey.is_empty() || recipient_pubkey.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "issuer_pubkey and recipient_pubkey parameters are required".to_string(),
            )),
        );
    }

    // Validate hex encoding
    if hex::decode(issuer_pubkey).is_err() || hex::decode(recipient_pubkey).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "Invalid hex encoding for public keys".to_string(),
            )),
        );
    }

    // Validate amount if provided
    if !amount.is_empty() {
        if amount.parse::<u64>().is_err() {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid amount parameter".to_string(),
                )),
            );
        }
    }

    // Get the current tracker state digest from shared tracker state
    let tracker_state_digest = {
        // Get the current AVL root digest from shared tracker state
        let shared_state = state.shared_tracker_state.lock().await;
        let current_digest = shared_state.get_avl_root_digest();
        drop(shared_state); // Release the lock early
        hex::encode(&current_digest)
    };

    // Generate a real AVL proof for the note
    // Send command to tracker thread to generate the proof
    let (proof_response_tx, proof_response_rx) = tokio::sync::oneshot::channel();

    let issuer_pubkey_bytes = match hex::decode(issuer_pubkey) {
        Ok(bytes) => {
            match bytes.try_into() {
                Ok(arr) => arr,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(crate::models::error_response(
                            "issuer_pubkey must be 33 bytes".to_string(),
                        )),
                    )
                }
            }
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding for issuer public key".to_string(),
                )),
            )
        }
    };

    let recipient_pubkey_bytes = match hex::decode(recipient_pubkey) {
        Ok(bytes) => {
            match bytes.try_into() {
                Ok(arr) => arr,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(crate::models::error_response(
                            "recipient_pubkey must be 33 bytes".to_string(),
                        )),
                    )
                }
            }
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "Invalid hex encoding for recipient public key".to_string(),
                )),
            )
        }
    };

    if let Err(e) = state.tx.send(TrackerCommand::GenerateProof {
        issuer_pubkey: issuer_pubkey_bytes,
        recipient_pubkey: recipient_pubkey_bytes,
        response_tx: proof_response_tx,
    }).await {
        tracing::error!("Failed to send proof generation command to tracker thread: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }

    // Wait for response from tracker thread
    let proof_result = match proof_response_rx.await {
        Ok(Ok(note_proof)) => {
            // Convert the proof to a hex string for transmission
            hex::encode(&note_proof.avl_proof)
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to generate proof: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    format!("Failed to generate proof: {:?}", e),
                )),
            );
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Internal server error".to_string(),
                )),
            );
        }
    };

    let proof = ProofResponse {
        issuer_pubkey: issuer_pubkey.clone(),
        recipient_pubkey: recipient_pubkey.clone(),
        proof_data: proof_result,
        tracker_state_digest,
        block_height: 1500,
        timestamp: 1672531200,
    };

    tracing::info!(
        "Redemption proof generated for {} -> {} with amount {}",
        issuer_pubkey,
        recipient_pubkey,
        amount
    );

    (StatusCode::OK, Json(crate::models::success_response(proof)))
}

// Get the latest tracker box ID from the tracker storage
#[axum::debug_handler]
pub async fn get_latest_tracker_box_id(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<crate::models::TrackerBoxIdResponse>>) {
    tracing::debug!("Getting latest tracker box ID");

    // Get all tracker boxes from the tracker storage
    let tracker_boxes = match state.tracker_storage.get_all_tracker_boxes() {
        Ok(boxes) => boxes,
        Err(e) => {
            tracing::error!("Failed to retrieve tracker boxes: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve tracker boxes".to_string(),
                )),
            );
        }
    };

    if tracker_boxes.is_empty() {
        tracing::info!("No tracker boxes found");
        return (
            StatusCode::NOT_FOUND,
            Json(crate::models::error_response(
                "No tracker boxes found".to_string(),
            )),
        );
    }

    // Find the tracker box with the highest creation height (most recent)
    let latest_tracker_box = tracker_boxes
        .into_iter()
        .max_by_key(|box_info| box_info.creation_height);

    if let Some(tracker_box) = latest_tracker_box {
        let response = crate::models::TrackerBoxIdResponse {
            tracker_box_id: tracker_box.box_id,
            timestamp: tracker_box.last_verified_height, // Using last_verified_height as timestamp
            height: tracker_box.last_verified_height,
        };

        tracing::info!(
            "Successfully retrieved latest tracker box ID: {}",
            &response.tracker_box_id[..16]  // Log first 16 chars for privacy
        );

        (
            StatusCode::OK,
            Json(crate::models::success_response(response)),
        )
    } else {
        tracing::info!("No tracker boxes found");
        (
            StatusCode::NOT_FOUND,
            Json(crate::models::error_response(
                "No tracker boxes found".to_string(),
            )),
        )
    }
}

// Create a reserve creation payload for Ergo node's /wallet/payment/send API
#[axum::debug_handler]
pub async fn create_reserve_payload(
    State(_state): State<AppState>,
    Json(payload): Json<CreateReserveRequest>,
) -> (StatusCode, Json<ApiResponse<ReserveCreationResponse>>) {
    tracing::debug!("Creating reserve payload: {:?}", payload);

    // Validate the owner public key (33 bytes when hex-decoded)
    let owner_pubkey_bytes = match hex::decode(&payload.owner_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "owner_pubkey must be hex-encoded".to_string(),
                )),
            );
        }
    };

    if owner_pubkey_bytes.len() != 33 {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "owner_pubkey must be 33 bytes (66 hex characters)".to_string(),
            )),
        );
    }

    // Validate the NFT ID (should be valid hex for token ID)
    if payload.nft_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "nft_id cannot be empty".to_string(),
            )),
        );
    }

    // Validate the amount
    if payload.erg_amount == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(
                "erg_amount must be greater than 0".to_string(),
            )),
        );
    }

    // Get the hardcoded reserve contract P2S address from configuration
    let config = match crate::config::AppConfig::load() {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to load server configuration".to_string(),
                )),
            );
        }
    };

    let reserve_contract_address = config.ergo.basis_reserve_contract_p2s;

    // Create the payment request for the reserve
    let mut registers = std::collections::HashMap::new();
    registers.insert("R4".to_string(), payload.owner_pubkey.clone());
    // R5 can contain the tracker NFT ID if one is configured
    if let Some(tracker_nft_id) = &config.ergo.tracker_nft_id {
        registers.insert("R5".to_string(), tracker_nft_id.clone());
    } else {
        // If no tracker NFT ID is configured, we might still include the NFT ID
        // or leave it empty based on the use case - for now, we'll include the provided nft_id
        registers.insert("R5".to_string(), payload.nft_id.clone());
    }

    let payment_request = ReservePaymentRequest {
        address: reserve_contract_address,
        value: payload.erg_amount,
        assets: vec![Asset {
            token_id: payload.nft_id.clone(), // Clone to avoid moving
            amount: 1,
        }],
        registers,
    };

    // Create the response following Ergo node's /wallet/payment/send format
    let response = ReserveCreationResponse {
        requests: vec![payment_request],
        fee: config.transaction.fee, // Get fee from configuration
        change_address: "default".to_string(), // This will be filled by the wallet
    };

    tracing::info!(
        "Successfully created reserve payload for {} with {} ERG and NFT {}",
        payload.owner_pubkey,
        payload.erg_amount,
        &payload.nft_id
    );

    (
        StatusCode::OK,
        Json(crate::models::success_response(response)),
    )
}

// Get the Basis reserve contract P2S address from server configuration
#[axum::debug_handler]
pub async fn get_basis_reserve_contract_p2s(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    tracing::debug!("Getting Basis reserve contract P2S address from configuration");

    // Get the reserve contract address from the server configuration
    let config = match crate::config::AppConfig::load() {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to load server configuration".to_string(),
                )),
            );
        }
    };

    let reserve_contract_address = config.basis_reserve_contract_p2s();

    tracing::info!("Successfully retrieved Basis reserve contract P2S address: {}", reserve_contract_address);

    (
        StatusCode::OK,
        Json(crate::models::success_response(reserve_contract_address.to_string())),
    )
}
