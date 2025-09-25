use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use std::collections::HashMap;

use crate::{
    models::{ApiResponse, CreateNoteRequest, SerializableIouNote, TrackerEvent, KeyStatusResponse, RedeemRequest, RedeemResponse, ProofResponse},
    AppState,
};
use basis_store::{IouNote, NoteError, PubKey, Signature};

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
                Json(crate::models::error_response("signature must be hex-encoded".to_string())),
            )
        }
    };

    let signature: Signature = match signature_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("signature must be 64 bytes".to_string())),
            )
        }
    };

    let issuer_pubkey_bytes = match hex::decode(&payload.issuer_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("issuer_pubkey must be hex-encoded".to_string())),
            )
        }
    };

    let issuer_pubkey: PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("issuer_pubkey must be 33 bytes".to_string())),
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
            Json(crate::models::error_response("Tracker thread unavailable".to_string())),
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
            
            (StatusCode::CREATED, Json(crate::models::success_response(())))
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
            (StatusCode::BAD_REQUEST, Json(crate::models::error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response("Internal server error".to_string())),
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
    tracing::debug!("get_notes_by_issuer function called");
    eprintln!("GET /notes/issuer/{} called", pubkey_hex);
    eprintln!("DEBUG: get_notes_by_issuer function executed");
    eprintln!("DEBUG: pubkey_hex = {}", pubkey_hex);

    // Decode hex string to bytes
    let issuer_pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("Invalid hex encoding".to_string())),
            )
        }
    };

    // Convert to fixed-size array
    let issuer_pubkey: PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("issuer_pubkey must be 33 bytes".to_string())),
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
            Json(crate::models::error_response("Tracker thread unavailable".to_string())),
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
                tracing::debug!("Note found: amount={}, timestamp={}", note.amount, note.timestamp);
            }
            
            // Convert to serializable format
            let serializable_notes: Vec<SerializableIouNote> =
                notes.into_iter().map(SerializableIouNote::from).collect();
            (StatusCode::OK, Json(crate::models::success_response(serializable_notes)))
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
            (StatusCode::BAD_REQUEST, Json(crate::models::error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response("Internal server error".to_string())),
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
                Json(crate::models::error_response("issuer_pubkey must be 33 bytes".to_string())),
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
            Json(crate::models::error_response("Tracker thread unavailable".to_string())),
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
                Json(crate::models::success_response(Some(serializable_note))),
            )
        }
        Ok(Ok(None)) => {
            tracing::info!(
                "No note found from {} to {}",
                issuer_pubkey_hex,
                recipient_pubkey_hex
            );
            (StatusCode::NOT_FOUND, Json(crate::models::success_response(None)))
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
            (StatusCode::BAD_REQUEST, Json(crate::models::error_response(error_message)))
        }
        Err(_) => {
            tracing::error!("Tracker thread response channel closed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response("Internal server error".to_string())),
            )
        }
    }
}

// Simple test endpoint
#[axum::debug_handler]
pub async fn test_endpoint() -> (StatusCode, Json<ApiResponse<String>>) {
    tracing::debug!("Test endpoint called");
    (StatusCode::OK, Json(crate::models::success_response("Test successful".to_string())))
}

// Simple GET endpoint without state
#[axum::debug_handler]
pub async fn simple_get(
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    eprintln!("Simple GET endpoint called with: {}", pubkey_hex);
    (StatusCode::OK, Json(crate::models::success_response(format!("Simple response: {}", pubkey_hex))))
}

// Very simple GET endpoint without any extractors
#[axum::debug_handler]
pub async fn very_simple_get() -> (StatusCode, Json<ApiResponse<String>>) {
    eprintln!("Very simple GET endpoint called");
    (StatusCode::OK, Json(crate::models::success_response("Very simple response".to_string())))
}

// GET endpoint with path parameter but no state
#[axum::debug_handler]
pub async fn get_with_param_only(
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    eprintln!("GET with param only called with: {}", pubkey_hex);
    (StatusCode::OK, Json(crate::models::success_response(format!("Param only: {}", pubkey_hex))))
}

// GET endpoint with state but no path parameters
#[axum::debug_handler]
pub async fn get_with_state_only(
    State(_state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    eprintln!("GET with state only called");
    (StatusCode::OK, Json(crate::models::success_response("State only response".to_string())))
}

// GET endpoint with both state and path parameters
#[axum::debug_handler]
pub async fn get_with_state_and_param(
    State(_state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    eprintln!("GET with state and param called with: {}", pubkey_hex);
    (StatusCode::OK, Json(crate::models::success_response(format!("State and param: {}", pubkey_hex))))
}

// Simple test endpoint for notes issuer
#[axum::debug_handler]
pub async fn test_notes_issuer_simple(
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    eprintln!("Test notes issuer simple called with: {}", pubkey_hex);
    (StatusCode::OK, Json(crate::models::success_response(format!("Simple test: {}", pubkey_hex))))
}

// Test endpoint for notes issuer route
#[axum::debug_handler]
pub async fn test_notes_issuer(
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    tracing::debug!("Test notes issuer endpoint called with: {}", pubkey_hex);
    eprintln!("Test notes issuer endpoint called with: {}", pubkey_hex);
    (StatusCode::OK, Json(crate::models::success_response(format!("Received pubkey: {}", pubkey_hex))))
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
    let page_size = params.get("page_size").and_then(|ps| ps.parse().ok()).unwrap_or(20);

    // Get events from event store
    let events = match state.event_store.get_events_paginated(page, page_size).await {
        Ok(events) => events,
        Err(e) => {
            tracing::error!("Failed to retrieve events: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response("Failed to retrieve events".to_string())),
            );
        }
    };
    
    tracing::info!(
        "Successfully retrieved {} events for page {} (size: {})",
        events.len(),
        page,
        page_size
    );
    
    (StatusCode::OK, Json(crate::models::success_response(events)))
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
                Json(crate::models::error_response("Failed to retrieve events".to_string())),
            );
        }
    };
    
    tracing::info!("Successfully retrieved {} recent events", events.len());
    
    (StatusCode::OK, Json(crate::models::success_response(events)))
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
                Json(crate::models::error_response("Invalid hex encoding".to_string())),
            )
        }
    };

    // Convert to fixed-size array
    let pubkey: basis_store::PubKey = match pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("Public key must be 33 bytes".to_string())),
            )
        }
    };

    // In a real implementation, this would:
    // 1. Get total debt from note storage
    // 2. Get collateral from reserve tracker
    // 3. Calculate collateralization ratio
    // 4. Return comprehensive status

    // Mock implementation for now
    let status = KeyStatusResponse {
        total_debt: 1500000000, // 1.5 ERG
        collateral: 3000000000, // 3.0 ERG
        collateralization_ratio: 2.0,
        note_count: 5,
        last_updated: 1672531200, // Jan 1, 2023
        issuer_pubkey: pubkey_hex.clone(),
    };

    tracing::info!("Returning key status for {}", pubkey_hex);
    
    (StatusCode::OK, Json(crate::models::success_response(status)))
}

// Initiate redemption process
#[axum::debug_handler]
pub async fn initiate_redemption(
    State(_state): State<AppState>,
    Json(payload): Json<RedeemRequest>,
) -> (StatusCode, Json<ApiResponse<RedeemResponse>>) {
    tracing::debug!("Initiating redemption: {:?}", payload);

    // Validate input parameters
    let issuer_pubkey_bytes = match hex::decode(&payload.issuer_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("Invalid issuer_pubkey hex encoding".to_string())),
            )
        }
    };

    let recipient_pubkey_bytes = match hex::decode(&payload.recipient_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("Invalid recipient_pubkey hex encoding".to_string())),
            )
        }
    };

    let _issuer_pubkey: basis_store::PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("issuer_pubkey must be 33 bytes".to_string())),
            )
        }
    };

    let _recipient_pubkey: basis_store::PubKey = match recipient_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response("recipient_pubkey must be 33 bytes".to_string())),
            )
        }
    };

    // In a real implementation, this would:
    // 1. Verify the note exists and is valid
    // 2. Generate redemption proof
    // 3. Create redemption transaction
    // 4. Return redemption information

    // Mock implementation for now
    let response = RedeemResponse {
        redemption_id: format!("redeem_{}_{}", &payload.issuer_pubkey[..16], &payload.recipient_pubkey[..16]),
        amount: payload.amount,
        timestamp: payload.timestamp,
        proof_available: true,
        transaction_pending: false,
    };

    tracing::info!("Redemption initiated for {} -> {}", payload.issuer_pubkey, payload.recipient_pubkey);
    
    (StatusCode::OK, Json(crate::models::success_response(response)))
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
            Json(crate::models::error_response("issuer_pubkey and recipient_pubkey parameters are required".to_string())),
        );
    }

    // Validate hex encoding
    if hex::decode(issuer_pubkey).is_err() || hex::decode(recipient_pubkey).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response("Invalid hex encoding for public keys".to_string())),
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

    tracing::info!("Proof generated for {} -> {}", issuer_pubkey, recipient_pubkey);
    
    (StatusCode::OK, Json(crate::models::success_response(proof)))
}