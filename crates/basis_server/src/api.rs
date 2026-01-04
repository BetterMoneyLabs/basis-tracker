use axum::{extract::State, http::StatusCode, Json};
use std::collections::HashMap;

use crate::{
    models::{
        ApiResponse, CompleteRedemptionRequest, CreateNoteRequest, CreateReserveRequest,
        KeyStatusResponse, ProofResponse, RedeemRequest, RedeemResponse,
        ReserveCreationResponse, ReservePaymentRequest, Asset,
        SerializableIouNote, TrackerEvent,
    },
    AppState, TrackerCommand,
};
use basis_store::{IouNote, NoteError, PubKey, Signature};
use ergo_lib::ergotree_ir::address::AddressEncoder;

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

    // Find reserve for this issuer
    let reserve = all_reserves
        .into_iter()
        .find(|reserve| reserve.owner_pubkey == pubkey_hex);

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

    // Create redemption request
    let redemption_request = basis_store::RedemptionRequest {
        issuer_pubkey: payload.issuer_pubkey.clone(),
        recipient_pubkey: payload.recipient_pubkey.clone(),
        amount: payload.amount,
        timestamp: payload.timestamp,
        reserve_box_id: "".to_string(), // Will be looked up from reserve tracker
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
            tracing::error!("Redemption failed: {}", e);
            // Even if redemption fails, return a response with transaction data if available
            let response = RedeemResponse {
                redemption_id: "failed".to_string(), // Indicate this is a failed redemption
                amount: payload.amount,
                timestamp: payload.timestamp,
                proof_available: false,
                transaction_pending: false,
                transaction_data: None, // No transaction data available on failure
            };

            (
                StatusCode::BAD_REQUEST,
                Json(crate::models::success_response(response)),
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
