use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use crate::{
    models::{
        ApiResponse, Asset, CheckAcceptanceRequest, CheckAcceptanceResponse, CreateNoteRequest,
        CreateReserveRequest, KeyStatusResponse, NoteConfirmationSummary, NoteStateRequest,
        NoteStateResponse, PendingTxResponse, ReserveCreationResponse, ReservePaymentRequest,
        SerializableIouNote, TrackerEvent, TrackerStateResponse, UploadPolicyRequest,
        UploadPolicyResponse,
    },
    AppState, TrackerCommand,
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

    let tracker_response =
        crate::tracker_request(&state.tx, |response_tx| crate::TrackerCommand::AddNote {
            issuer_pubkey,
            note,
            response_tx,
        })
        .await;

    match tracker_response {
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
                NoteError::DebtRegression => "Cumulative debt cannot decrease".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::MigrationRequired(msg) => format!("Migration required: {}", msg),
                NoteError::GenerationMismatch(msg) => format!("Generation mismatch: {}", msg),
                NoteError::GenerationBindingRequired(msg) => {
                    format!("Generation binding required: {}", msg)
                }
                NoteError::CapacityExceeded { limit } => {
                    format!("Tracker note capacity exceeded ({})", limit)
                }
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::StorageOutcomeUnknown(_) => {
                    "Storage outcome unknown; restart and reconcile".to_string()
                }
                NoteError::PublicationInProgress => {
                    "Tracker publication in progress; retry".to_string()
                }
                NoteError::PublicationLeaseMismatch => {
                    "Tracker publication lease mismatch".to_string()
                }
                NoteError::InvalidTransactionId => "Invalid transaction id".to_string(),
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

    tracing::debug!("Sending GetNotesByIssuer command to tracker thread");
    let tracker_response = crate::tracker_request(&state.tx, |response_tx| {
        crate::TrackerCommand::GetNotesByIssuer {
            issuer_pubkey,
            response_tx,
        }
    })
    .await;

    tracing::debug!("GetNotesByIssuer command sent successfully");
    match tracker_response {
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

            // Convert to serializable format with issuer pubkey and confirmation state
            let mut serializable_notes = Vec::new();
            for note in notes {
                let confirmation = fetch_confirmation(
                    &state.tx,
                    issuer_pubkey,
                    note.recipient_pubkey,
                    note.amount_redeemed,
                )
                .await;
                let mut serializable_note = SerializableIouNote::from(note);
                serializable_note.issuer_pubkey = pubkey_hex.clone();
                serializable_note.confirmation = confirmation;
                serializable_notes.push(serializable_note);
            }
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
                NoteError::DebtRegression => "Cumulative debt cannot decrease".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::MigrationRequired(msg) => format!("Migration required: {}", msg),
                NoteError::GenerationMismatch(msg) => format!("Generation mismatch: {}", msg),
                NoteError::GenerationBindingRequired(msg) => {
                    format!("Generation binding required: {}", msg)
                }
                NoteError::CapacityExceeded { limit } => {
                    format!("Tracker note capacity exceeded ({})", limit)
                }
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::StorageOutcomeUnknown(_) => {
                    "Storage outcome unknown; restart and reconcile".to_string()
                }
                NoteError::PublicationInProgress => {
                    "Tracker publication in progress; retry".to_string()
                }
                NoteError::PublicationLeaseMismatch => {
                    "Tracker publication lease mismatch".to_string()
                }
                NoteError::InvalidTransactionId => "Invalid transaction id".to_string(),
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

    let tracker_response = crate::tracker_request(&state.tx, |response_tx| {
        crate::TrackerCommand::GetNotesByRecipientWithIssuer {
            recipient_pubkey,
            response_tx,
        }
    })
    .await;

    match tracker_response {
        Ok(Ok(notes_with_issuer)) => {
            tracing::info!(
                "Successfully retrieved {} notes for recipient {}",
                notes_with_issuer.len(),
                pubkey_hex
            );

            // Convert to serializable format with correct issuer pubkey and confirmation state
            let mut serializable_notes = Vec::new();
            for (issuer_pubkey, note) in notes_with_issuer {
                let confirmation = fetch_confirmation(
                    &state.tx,
                    issuer_pubkey,
                    note.recipient_pubkey,
                    note.amount_redeemed,
                )
                .await;
                let mut serializable_note = SerializableIouNote::from(note);
                serializable_note.issuer_pubkey = hex::encode(issuer_pubkey);
                serializable_note.confirmation = confirmation;
                serializable_notes.push(serializable_note);
            }
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
                NoteError::DebtRegression => "Cumulative debt cannot decrease".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::MigrationRequired(msg) => format!("Migration required: {}", msg),
                NoteError::GenerationMismatch(msg) => format!("Generation mismatch: {}", msg),
                NoteError::GenerationBindingRequired(msg) => {
                    format!("Generation binding required: {}", msg)
                }
                NoteError::CapacityExceeded { limit } => {
                    format!("Tracker note capacity exceeded ({})", limit)
                }
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::StorageOutcomeUnknown(_) => {
                    "Storage outcome unknown; restart and reconcile".to_string()
                }
                NoteError::PublicationInProgress => {
                    "Tracker publication in progress; retry".to_string()
                }
                NoteError::PublicationLeaseMismatch => {
                    "Tracker publication lease mismatch".to_string()
                }
                NoteError::InvalidTransactionId => "Invalid transaction id".to_string(),
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

    let tracker_response = crate::tracker_request(&state.tx, |response_tx| {
        crate::TrackerCommand::GetNoteByIssuerAndRecipient {
            issuer_pubkey,
            recipient_pubkey,
            response_tx,
        }
    })
    .await;

    match tracker_response {
        Ok(Ok(Some(note))) => {
            tracing::info!(
                "Successfully retrieved note from {} to {}",
                issuer_pubkey_hex,
                recipient_pubkey_hex
            );
            // Convert to serializable format with issuer pubkey and confirmation state
            let confirmation = fetch_confirmation(
                &state.tx,
                issuer_pubkey,
                note.recipient_pubkey,
                note.amount_redeemed,
            )
            .await;
            let mut serializable_note = SerializableIouNote::from(note);
            serializable_note.issuer_pubkey = issuer_pubkey_hex.clone();
            serializable_note.confirmation = confirmation;
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
                NoteError::DebtRegression => "Cumulative debt cannot decrease".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::MigrationRequired(msg) => format!("Migration required: {}", msg),
                NoteError::GenerationMismatch(msg) => format!("Generation mismatch: {}", msg),
                NoteError::GenerationBindingRequired(msg) => {
                    format!("Generation binding required: {}", msg)
                }
                NoteError::CapacityExceeded { limit } => {
                    format!("Tracker note capacity exceeded ({})", limit)
                }
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::StorageOutcomeUnknown(_) => {
                    "Storage outcome unknown; restart and reconcile".to_string()
                }
                NoteError::PublicationInProgress => {
                    "Tracker publication in progress; retry".to_string()
                }
                NoteError::PublicationLeaseMismatch => {
                    "Tracker publication lease mismatch".to_string()
                }
                NoteError::InvalidTransactionId => "Invalid transaction id".to_string(),
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
) -> (
    StatusCode,
    Json<ApiResponse<Vec<crate::models::SerializableIouNoteWithAge>>>,
) {
    tracing::debug!("Getting all notes");

    let tracker_response = crate::tracker_request(&state.tx, |response_tx| {
        crate::TrackerCommand::GetNotes { response_tx }
    })
    .await;

    match tracker_response {
        Ok(Ok(notes_with_issuer)) => {
            tracing::info!("Successfully retrieved {} notes", notes_with_issuer.len());

            // Convert to serializable format with age calculation
            let current_time_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            let mut serializable_notes: Vec<crate::models::SerializableIouNoteWithAge> = Vec::new();
            for (issuer_pubkey, note) in notes_with_issuer {
                let age_seconds = current_time_ms.saturating_sub(note.timestamp) / 1000;
                let confirmation = fetch_confirmation(
                    &state.tx,
                    issuer_pubkey,
                    note.recipient_pubkey,
                    note.amount_redeemed,
                )
                .await;
                serializable_notes.push(crate::models::SerializableIouNoteWithAge {
                    issuer_pubkey: hex::encode(issuer_pubkey),
                    recipient_pubkey: hex::encode(note.recipient_pubkey),
                    amount_collected: note.amount_collected,
                    amount_redeemed: note.amount_redeemed,
                    timestamp: note.timestamp,
                    signature: hex::encode(note.signature),
                    age_seconds,
                    confirmation,
                });
            }

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
                NoteError::DebtRegression => "Cumulative debt cannot decrease".to_string(),
                NoteError::RedemptionTooEarly => "Redemption too early".to_string(),
                NoteError::InsufficientCollateral => "Insufficient collateral".to_string(),
                NoteError::MigrationRequired(msg) => format!("Migration required: {}", msg),
                NoteError::GenerationMismatch(msg) => format!("Generation mismatch: {}", msg),
                NoteError::GenerationBindingRequired(msg) => {
                    format!("Generation binding required: {}", msg)
                }
                NoteError::CapacityExceeded { limit } => {
                    format!("Tracker note capacity exceeded ({})", limit)
                }
                NoteError::StorageError(msg) => format!("Storage error: {}", msg),
                NoteError::StorageOutcomeUnknown(_) => {
                    "Storage outcome unknown; restart and reconcile".to_string()
                }
                NoteError::PublicationInProgress => {
                    "Tracker publication in progress; retry".to_string()
                }
                NoteError::PublicationLeaseMismatch => {
                    "Tracker publication lease mismatch".to_string()
                }
                NoteError::InvalidTransactionId => "Invalid transaction id".to_string(),
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

/// Load a serialized, conservative debt snapshot from the tracker thread.
///
/// Returning `None` is deliberate fail-closed input for collateral predicates;
/// predicates that do not inspect collateral remain unaffected.
async fn load_projected_issuer_gross_debt(
    state: &AppState,
    issuer_pubkey: PubKey,
    candidate_recipient: Option<PubKey>,
    candidate_total_debt: u64,
) -> Option<u64> {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    if let Err(e) = state
        .tx
        .try_send(TrackerCommand::GetProjectedIssuerGrossDebt {
            issuer_pubkey,
            candidate_recipient,
            candidate_total_debt,
            response_tx,
        })
    {
        tracing::error!("Failed to request projected issuer debt: {:?}", e);
        return None;
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), response_rx).await {
        Ok(Ok(Ok(total))) => Some(total),
        Ok(Ok(Err(e))) => {
            tracing::error!("Failed to calculate projected issuer debt: {:?}", e);
            None
        }
        Ok(Err(e)) => {
            tracing::error!("Projected issuer debt response channel closed: {:?}", e);
            None
        }
        Err(_) => {
            tracing::error!("Timed out calculating projected issuer debt");
            None
        }
    }
}

/// Check if a note would be accepted by the server's acceptance policy
///
/// First checks for a per-recipient policy in the database. If found, uses that policy.
/// Otherwise falls back to the server's global acceptance predicate.
#[axum::debug_handler]
pub async fn check_acceptance(
    State(state): State<AppState>,
    Json(payload): Json<CheckAcceptanceRequest>,
) -> (StatusCode, Json<ApiResponse<CheckAcceptanceResponse>>) {
    tracing::debug!("Checking acceptance for issuer: {}", payload.issuer_pubkey);

    // Parse issuer public key
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

    // Parse recipient public key (if provided, otherwise use server default)
    let recipient_pubkey = if let Some(ref recipient_hex) = payload.recipient_pubkey {
        let recipient_bytes = match hex::decode(recipient_hex) {
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
        match recipient_bytes.try_into() {
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
    } else {
        [0u8; 33] // Default: no specific recipient
    };

    // Try to find per-recipient policy first
    let result = match state.policy_storage.get_policy(&recipient_pubkey) {
        Ok(Some(stored_policy)) => {
            tracing::debug!(
                "Found per-recipient policy for {}",
                payload
                    .recipient_pubkey
                    .as_ref()
                    .unwrap_or(&"default".to_string())
            );

            // Parse the stored policy
            match parse_acceptance_policy_json(&stored_policy.policy_json) {
                Ok(policy_config) => {
                    // Build predicate tree from stored policy
                    match crate::acceptance::builder::build_predicate_tree(policy_config) {
                        Ok(Some(predicate)) => {
                            // Clone reserve tracker from mutex
                            let reserve_tracker = state.reserve_tracker.lock().await.clone();
                            let projected_issuer_gross_debt =
                                if predicate.requires_liability_snapshot() {
                                    load_projected_issuer_gross_debt(
                                        &state,
                                        issuer_pubkey,
                                        payload.recipient_pubkey.as_ref().map(|_| recipient_pubkey),
                                        payload.total_debt,
                                    )
                                    .await
                                } else {
                                    None
                                };

                            // Build context
                            let ctx = crate::acceptance::PredicateContext {
                                issuer_pubkey,
                                recipient_pubkey,
                                total_debt: payload.total_debt,
                                projected_issuer_gross_debt,
                                reserve_tracker: Some(reserve_tracker),
                            };

                            let acceptable = predicate.acceptable(&ctx);
                            let reason = if acceptable {
                                None
                            } else {
                                Some(format!(
                                    "Note rejected by per-recipient policy '{}'",
                                    predicate.name()
                                ))
                            };

                            CheckAcceptanceResponse { acceptable, reason }
                        }
                        Ok(None) => {
                            // Empty policy - use default
                            CheckAcceptanceResponse {
                                acceptable: false,
                                reason: Some(
                                    "Empty per-recipient policy - rejecting by default".to_string(),
                                ),
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to build predicate from stored policy: {}", e);
                            // Fall through to global policy
                            CheckAcceptanceResponse {
                                acceptable: false,
                                reason: Some(
                                    "Invalid stored policy - rejecting by default".to_string(),
                                ),
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse stored policy JSON: {}", e);
                    // Fall through to global policy
                    CheckAcceptanceResponse {
                        acceptable: false,
                        reason: Some("Corrupted stored policy - rejecting by default".to_string()),
                    }
                }
            }
        }
        Ok(None) => {
            // No per-recipient policy found, use global policy
            tracing::debug!("No per-recipient policy found, using global policy");

            if let Some(predicate) = &state.acceptance_predicate {
                // Clone reserve tracker from mutex
                let reserve_tracker = state.reserve_tracker.lock().await.clone();
                let projected_issuer_gross_debt = if predicate.requires_liability_snapshot() {
                    load_projected_issuer_gross_debt(
                        &state,
                        issuer_pubkey,
                        payload.recipient_pubkey.as_ref().map(|_| recipient_pubkey),
                        payload.total_debt,
                    )
                    .await
                } else {
                    None
                };

                // Build context
                let ctx = crate::acceptance::PredicateContext {
                    issuer_pubkey,
                    recipient_pubkey,
                    total_debt: payload.total_debt,
                    projected_issuer_gross_debt,
                    reserve_tracker: Some(reserve_tracker),
                };

                let acceptable = predicate.acceptable(&ctx);
                let reason = if acceptable {
                    None
                } else {
                    Some(format!(
                        "Note rejected by global policy '{}'",
                        predicate.name()
                    ))
                };

                CheckAcceptanceResponse { acceptable, reason }
            } else {
                // No predicate configured - use default from config
                let acceptable = state.config.acceptance.default.acceptable();
                let reason = if acceptable {
                    None
                } else {
                    Some("No acceptance policy configured - rejecting by default".to_string())
                };

                CheckAcceptanceResponse { acceptable, reason }
            }
        }
        Err(e) => {
            tracing::error!("Failed to read policy from storage: {:?}", e);
            // Fall back to global policy on error
            if let Some(predicate) = &state.acceptance_predicate {
                let reserve_tracker = state.reserve_tracker.lock().await.clone();
                let projected_issuer_gross_debt = if predicate.requires_liability_snapshot() {
                    load_projected_issuer_gross_debt(
                        &state,
                        issuer_pubkey,
                        payload.recipient_pubkey.as_ref().map(|_| recipient_pubkey),
                        payload.total_debt,
                    )
                    .await
                } else {
                    None
                };
                let ctx = crate::acceptance::PredicateContext {
                    issuer_pubkey,
                    recipient_pubkey,
                    total_debt: payload.total_debt,
                    projected_issuer_gross_debt,
                    reserve_tracker: Some(reserve_tracker),
                };
                let acceptable = predicate.acceptable(&ctx);
                let reason = if acceptable {
                    None
                } else {
                    Some(format!(
                        "Note rejected by global policy '{}'",
                        predicate.name()
                    ))
                };
                CheckAcceptanceResponse { acceptable, reason }
            } else {
                CheckAcceptanceResponse {
                    acceptable: false,
                    reason: Some("Policy storage error - rejecting by default".to_string()),
                }
            }
        }
    };

    tracing::info!(
        "Acceptance check for {}: acceptable={}, total_debt={}",
        payload.issuer_pubkey,
        result.acceptable,
        payload.total_debt
    );

    (
        StatusCode::OK,
        Json(crate::models::success_response(result)),
    )
}

/// Parse an acceptance policy JSON string into an `AcceptanceConfig`.
///
/// This works around a serde_json limitation when the `arbitrary_precision` feature is
/// enabled (pulled in by ergo-lib): internally tagged enums with `f64` fields fail to
/// deserialize directly from a stream, so we first parse to `Value` and then to the
/// target type.
fn parse_acceptance_policy_json(
    policy_json: &str,
) -> Result<basis_core::acceptance::AcceptanceConfig, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(policy_json)?;
    serde_json::from_value(value)
}

/// Upload a signed acceptance policy for a recipient
#[axum::debug_handler]
pub async fn upload_policy(
    State(state): State<AppState>,
    Json(payload): Json<UploadPolicyRequest>,
) -> (StatusCode, Json<ApiResponse<UploadPolicyResponse>>) {
    tracing::debug!(
        "Uploading acceptance policy for: {}",
        payload.recipient_pubkey
    );

    // Parse recipient public key
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

    // Verify signature over policy_json
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

    if signature_bytes.len() != 65 {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(format!(
                "signature must be 65 bytes (130 hex chars), got {} bytes",
                signature_bytes.len()
            ))),
        );
    }

    let mut signature: Signature = [0u8; 65];
    signature.copy_from_slice(&signature_bytes);

    // Parse policy JSON to validate structure
    let policy_config: basis_core::acceptance::AcceptanceConfig =
        match parse_acceptance_policy_json(&payload.policy_json) {
            Ok(config) => config,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(crate::models::error_response(format!(
                        "Invalid policy JSON: {}",
                        e
                    ))),
                )
            }
        };

    // Verify Schnorr signature over policy_json using recipient_pubkey
    // The policy is signed by the recipient to prove ownership
    let policy_message = payload.policy_json.as_bytes();
    match basis_offchain::schnorr::schnorr_verify(&signature, policy_message, &recipient_pubkey) {
        Ok(()) => {
            tracing::info!(
                "Signature verified for policy upload from {}",
                payload.recipient_pubkey
            );
        }
        Err(e) => {
            tracing::warn!(
                "Signature verification failed for policy upload from {}: {:?}",
                payload.recipient_pubkey,
                e
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(crate::models::error_response(
                    "Invalid signature: policy signature verification failed".to_string(),
                )),
            );
        }
    }

    // Store policy in database
    let policy_hash = format!("{:x}", {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        payload.policy_json.hash(&mut hasher);
        hasher.finish()
    });

    let uploaded_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Store the policy in the database
    match state.policy_storage.store_policy(
        &recipient_pubkey,
        &payload.policy_json,
        &payload.signature,
    ) {
        Ok(()) => {
            tracing::info!(
                "Policy stored for recipient {}: hash={}",
                payload.recipient_pubkey,
                policy_hash
            );
        }
        Err(e) => {
            tracing::error!("Failed to store policy: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to store policy".to_string(),
                )),
            );
        }
    }

    // Build a new predicate tree from the uploaded policy for validation
    match crate::acceptance::builder::build_predicate_tree(policy_config) {
        Ok(Some(predicate)) => {
            tracing::info!(
                "Built predicate tree from uploaded policy: '{}'",
                predicate.name()
            );
        }
        Ok(None) => {
            tracing::info!("Empty policy uploaded for {}", payload.recipient_pubkey);
        }
        Err(e) => {
            tracing::warn!("Failed to build predicate tree from uploaded policy: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(format!(
                    "Invalid policy structure: {}",
                    e
                ))),
            );
        }
    }

    let response = UploadPolicyResponse {
        uploaded_at,
        policy_hash,
    };

    (
        StatusCode::OK,
        Json(crate::models::success_response(response)),
    )
}

/// Get acceptance policy for a specific recipient
#[axum::debug_handler]
pub async fn get_policy_by_recipient(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::models::GetPolicyResponse>>,
) {
    tracing::debug!("Getting acceptance policy for: {}", pubkey_hex);

    // Parse recipient public key
    let recipient_pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "pubkey must be hex-encoded".to_string(),
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
                    "pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    // Retrieve policy from storage
    match state.policy_storage.get_policy(&recipient_pubkey) {
        Ok(Some(stored_policy)) => {
            let response = crate::models::GetPolicyResponse {
                recipient_pubkey: pubkey_hex,
                policy_json: stored_policy.policy_json,
                signature: stored_policy.signature,
                uploaded_at: stored_policy.timestamp,
            };
            (
                StatusCode::OK,
                Json(crate::models::success_response(response)),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(crate::models::error_response(
                "No policy found for this recipient".to_string(),
            )),
        ),
        Err(e) => {
            tracing::error!("Failed to retrieve policy: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve policy".to_string(),
                )),
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EventPageQuery {
    page: Option<usize>,
    page_size: Option<usize>,
}

// Get paginated tracker events from event store
#[axum::debug_handler]
pub async fn get_events_paginated(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<EventPageQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<TrackerEvent>>>) {
    tracing::debug!("Getting paginated events: {:?}", params);

    let page = params.page.unwrap_or(0);
    let page_size = params.page_size.unwrap_or(20);

    // Get events from event store
    let events = match state
        .event_store
        .get_events_paginated(page, page_size)
        .await
    {
        Ok(events) => events,
        Err(
            e @ (crate::store::EventStoreError::InvalidPageSize
            | crate::store::EventStoreError::PaginationOverflow),
        ) => {
            tracing::debug!("Rejected event pagination request: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(e.to_string())),
            );
        }
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
    let events = match state.event_store.get_recent_events(50).await {
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

    let tracker_response = crate::tracker_request(&state.tx, |response_tx| {
        crate::TrackerCommand::GetNotesByIssuer {
            issuer_pubkey,
            response_tx,
        }
    })
    .await;

    let notes = match tracker_response {
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
    let total_debt = match notes.iter().try_fold(0u64, |total, note| {
        total.checked_add(note.outstanding_debt())
    }) {
        Some(total) => total,
        None => {
            tracing::warn!("Issuer outstanding-debt aggregate overflow");
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(crate::models::error_response(
                    "Outstanding debt aggregate exceeds the supported u64 range".to_string(),
                )),
            );
        }
    };
    let note_count = notes.len();

    // Get collateral from reserve tracker
    let tracker = state.reserve_tracker.lock().await;
    let all_reserves = tracker.get_all_reserves();

    // Normalize the public key to handle different representations (e.g., 07 prefix for GroupElement)
    let normalized_pubkey = basis_store::normalize_public_key(&pubkey_hex);

    // Find all reserves for this issuer - check multiple key representations for comprehensive correlation
    let matching_reserves: Vec<_> = all_reserves
        .into_iter()
        .filter(|reserve| {
            let normalized_reserve_key = basis_store::normalize_public_key(&reserve.owner_pubkey);
            let original_reserve_key = &reserve.owner_pubkey;

            // Check multiple matching possibilities to ensure comprehensive key correlation:
            // 1. Direct match between normalized keys (main case)
            // 2. Match between original pubkey and normalized reserve key
            // 3. Match between original pubkey and original reserve key (backup)
            // 4. Special case: original pubkey matches the part of reserve key after '07' prefix
            normalized_pubkey == normalized_reserve_key
                || pubkey_hex == normalized_reserve_key
                || pubkey_hex == *original_reserve_key
                || (original_reserve_key.starts_with("07")
                    && original_reserve_key.len() >= 66
                    && &original_reserve_key[2..] == pubkey_hex.as_str())
        })
        .collect();

    let has_pending_refund = matching_reserves
        .iter()
        .any(|reserve| reserve.is_refund_pending());

    let (collateral, collateralization_ratio, last_updated) =
        if let Some(reserve) = matching_reserves.first() {
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
        has_pending_refund,
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

/// Retired server-sign redemption endpoint.
#[axum::debug_handler]
pub async fn initiate_redemption() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Legacy direct-completion endpoint.
///
/// A caller-provided transaction id and accounting tuple are not evidence that
/// the expected reserve successor is confirmed on the active chain.  Keep the
/// route as an explicit tombstone so older clients fail closed instead of
/// silently mutating tracker state.
#[axum::debug_handler]
pub async fn complete_redemption() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 tracker-proof endpoint.
#[axum::debug_handler]
pub async fn get_tracker_proof() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 reserve-proof endpoint.
#[axum::debug_handler]
pub async fn get_reserve_proof() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 tracker-signature endpoint.
#[axum::debug_handler]
pub async fn request_tracker_signature() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 redemption-preparation endpoint.
#[axum::debug_handler]
pub async fn prepare_redemption() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 aggregate redemption-proof endpoint.
#[axum::debug_handler]
pub async fn get_redemption_proof() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

// Get the latest tracker box ID from the tracker storage
#[axum::debug_handler]
pub async fn get_latest_tracker_box_id(
    State(state): State<AppState>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::models::TrackerBoxIdResponse>>,
) {
    tracing::debug!("Getting latest tracker box ID");

    // Use the live shared tracker state maintained by the background updater.
    // The updater queries the node every cycle and records the confirmed
    // on-chain tracker box, so this is always current.  Falling back to the
    // confirmed snapshot handles the brief window before the first updater run.
    let shared = state.shared_tracker_state.lock().await;
    let tracker_box_id = shared
        .get_tracker_box_id()
        .or_else(|| shared.get_confirmed().box_id);
    drop(shared);

    match tracker_box_id {
        Some(tracker_box_id) => {
            let current_height = state
                .ergo_scanner
                .lock()
                .await
                .get_current_height()
                .await
                .unwrap_or(0);
            let response = crate::models::TrackerBoxIdResponse {
                tracker_box_id,
                timestamp: current_height,
                height: current_height,
            };

            tracing::info!(
                "Successfully retrieved latest tracker box ID: {}",
                &response.tracker_box_id[..16] // Log first 16 chars for privacy
            );

            (
                StatusCode::OK,
                Json(crate::models::success_response(response)),
            )
        }
        None => {
            tracing::info!("No tracker box found in live state");
            (
                StatusCode::NOT_FOUND,
                Json(crate::models::error_response(
                    "No tracker boxes found".to_string(),
                )),
            )
        }
    }
}

// Create a reserve creation payload for Ergo node's /wallet/payment/send API
#[axum::debug_handler]
pub async fn create_reserve_payload(
    State(state): State<AppState>,
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

    // Use the exact configuration installed in this running server. Reloading a
    // second file/env view here could validate one P2S and build against another.
    let config = state.config.clone();

    if let Err(e) = config.reject_unsupported_reserve_builder() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(crate::models::error_response(e)),
        );
    }

    let reserve_contract_address = config.ergo.basis_reserve_contract_p2s.clone();

    // Build properly serialized register values following Ergo constant format
    // R4: GroupElement (owner pubkey) - prefix 07 + 33-byte compressed pubkey
    let r4_value = format!("07{}", payload.owner_pubkey);

    // R5: SAvlTree (empty AVL tree) - prefix 64 + 33-byte digest + flags + key_len + value_len
    // Empty tree digest for PlasmaParameters(32, None) is 4ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900
    // Flags: 0x03 (insert + update allowed) for the insertOrUpdate reserve contract.
    let empty_tree_hex =
        "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000";
    let r5_value = format!("{}", empty_tree_hex);

    // R6: Coll[Byte] (tracker NFT ID) - prefix 0e + 2-byte length + 32-byte NFT ID
    let tracker_nft_id = config
        .ergo
        .tracker_nft_id
        .as_ref()
        .unwrap_or(&payload.nft_id);
    let tracker_nft_bytes = match hex::decode(tracker_nft_id) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "tracker_nft_id must be valid hex".to_string(),
                )),
            );
        }
    };
    // Verify tracker NFT ID is 32 bytes
    if tracker_nft_bytes.len() != 32 {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::error_response(format!(
                "tracker_nft_id must be 32 bytes, got {}",
                tracker_nft_bytes.len()
            ))),
        );
    }
    let r6_value = format!("0e{:02x}{}", tracker_nft_bytes.len(), tracker_nft_id);

    // R7: Long (refund initiation height, 0 for a new reserve with no pending refund)
    let r7_value = "05000000000000000000".to_string();

    // Create registers map
    let mut registers = std::collections::HashMap::new();
    registers.insert("R4".to_string(), r4_value);
    registers.insert("R5".to_string(), r5_value);
    registers.insert("R6".to_string(), r6_value);
    registers.insert("R7".to_string(), r7_value);

    let payment_request = ReservePaymentRequest {
        address: reserve_contract_address,
        value: payload.erg_amount,
        assets: vec![Asset {
            token_id: payload.nft_id.clone(), // Reserve NFT (singleton)
            amount: 1,
        }],
        registers,
    };

    // Get change address from configuration
    let change_address = state.config.get_change_address().unwrap_or_else(|e| {
        tracing::warn!("Failed to get change address from config: {}", e);
        // Fallback: derive from tracker public key directly
        if let Some(ref pubkey) = config.ergo.tracker_public_key {
            pubkey.clone()
        } else {
            payload.owner_pubkey.clone() // Use owner address as fallback (not ideal but safe)
        }
    });

    // Create the response following Ergo node's /wallet/payment/send format
    let response = ReserveCreationResponse {
        requests: vec![payment_request],
        fee: config.transaction.fee, // Get fee from configuration
        change_address,
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

/// Retired node-wallet proxy.
///
/// Reserve creation payloads are returned by `/reserves/create` for review and
/// signing by the reserve owner's wallet.  The tracker must never convert an
/// unauthenticated HTTP request into authority over its configured node wallet.
#[axum::debug_handler]
pub async fn submit_reserve_transaction(
    Json(_payload): Json<ReserveCreationResponse>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        StatusCode::GONE,
        Json(crate::models::error_response(
            "Tracker-side reserve submission is retired; sign and submit the generated payload with the reserve owner's wallet"
                .to_string(),
        )),
    )
}

// Get the Basis reserve contract P2S address from server configuration
#[axum::debug_handler]
pub async fn get_basis_reserve_contract_p2s(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    tracing::debug!("Getting Basis reserve contract P2S address from configuration");

    let config = state.config.clone();

    if let Err(e) = config.reject_unsupported_reserve_builder() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(crate::models::error_response(e)),
        );
    }

    let reserve_contract_address = config.basis_reserve_contract_p2s();

    tracing::info!(
        "Successfully retrieved Basis reserve contract P2S address: {}",
        reserve_contract_address
    );

    (
        StatusCode::OK,
        Json(crate::models::success_response(
            reserve_contract_address.to_string(),
        )),
    )
}

#[cfg(test)]
mod security_boundary_tests {
    use super::*;

    #[tokio::test]
    async fn reserve_wallet_proxy_is_a_gone_tombstone() {
        let payload = ReserveCreationResponse {
            requests: Vec::new(),
            fee: 1_000_000,
            change_address: "not-forwarded".to_string(),
        };

        let (status, _) = submit_reserve_transaction(Json(payload)).await;
        assert_eq!(status, StatusCode::GONE);
    }
}

/// Get the current tracker state: local digest, confirmed on-chain digest, and
/// any in-flight pending update transaction.
#[axum::debug_handler]
pub async fn get_tracker_state(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<TrackerStateResponse>>) {
    tracing::debug!("Getting tracker state");

    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    if state
        .tx
        .send(TrackerCommand::GetValidatedState { response_tx })
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(crate::models::error_response(
                "Tracker state actor unavailable".to_string(),
            )),
        );
    }
    let local_state = match response_rx.await {
        Ok(Ok(local_state)) => local_state,
        Ok(Err(error)) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(crate::models::error_response(format!(
                    "Tracker state is unavailable: {error:?}"
                ))),
            )
        }
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(crate::models::error_response(
                    "Tracker state actor stopped".to_string(),
                )),
            )
        }
    };

    let shared = state.shared_tracker_state.lock().await;
    let confirmed = shared.get_confirmed();
    let pending = shared.get_pending();
    let tracker_box_id = shared.get_tracker_box_id();

    let response = TrackerStateResponse {
        local_digest: hex::encode(local_state.avl_root_digest),
        confirmed_digest: confirmed.digest.map(hex::encode),
        confirmed_box_id: confirmed.box_id,
        confirmed_height: confirmed.height,
        pending_digest: pending.digest.map(hex::encode),
        pending_tx_id: pending.tx_id,
        pending_submitted_height: pending.submitted_height,
        tracker_box_id,
    };

    tracing::info!(
        "Tracker state: local_digest={}, confirmed_digest={:?}, pending_tx_id={:?}",
        response.local_digest,
        response.confirmed_digest,
        response.pending_tx_id
    );

    (
        StatusCode::OK,
        Json(crate::models::success_response(response)),
    )
}

/// Get the currently in-flight tracker box update transaction, if any.
#[axum::debug_handler]
pub async fn get_pending_tx(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<PendingTxResponse>>) {
    tracing::debug!("Getting pending tracker update tx");

    let shared = state.shared_tracker_state.lock().await;
    let pending = shared.get_pending();

    let response = PendingTxResponse {
        pending_tx_id: pending.tx_id,
        pending_digest: pending.digest.map(hex::encode),
        submitted_height: pending.submitted_height,
    };

    (
        StatusCode::OK,
        Json(crate::models::success_response(response)),
    )
}

/// Get the confirmation state for a single note.
#[axum::debug_handler]
pub async fn get_note_state(
    State(state): State<AppState>,
    Json(payload): Json<NoteStateRequest>,
) -> (StatusCode, Json<ApiResponse<NoteStateResponse>>) {
    tracing::debug!(
        "Getting note state for issuer {} recipient {}",
        payload.issuer_pubkey,
        payload.recipient_pubkey
    );

    let issuer_pubkey = match parse_pubkey(&payload.issuer_pubkey) {
        Ok(pk) => pk,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(msg)),
            )
        }
    };

    let recipient_pubkey = match parse_pubkey(&payload.recipient_pubkey) {
        Ok(pk) => pk,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(msg)),
            )
        }
    };

    let note_response = crate::tracker_request(&state.tx, |response_tx| {
        TrackerCommand::GetNoteByIssuerAndRecipient {
            issuer_pubkey,
            recipient_pubkey,
            response_tx,
        }
    })
    .await;

    let note = match note_response {
        Ok(Ok(Some(n))) => n,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(crate::models::error_response("Note not found".to_string())),
            )
        }
        Ok(Err(NoteError::StorageError(msg))) if msg == "Note not found" => {
            return (
                StatusCode::NOT_FOUND,
                Json(crate::models::error_response("Note not found".to_string())),
            )
        }
        Ok(Err(e)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(format!(
                    "Failed to get note: {:?}",
                    e
                ))),
            )
        }
        Err(_) => return internal_error("Tracker thread response channel closed"),
    };

    let confirmation_response =
        crate::tracker_request(&state.tx, |response_tx| TrackerCommand::GetConfirmation {
            issuer_pubkey,
            recipient_pubkey,
            response_tx,
        })
        .await;

    let confirmation = match confirmation_response {
        Ok(Ok(Some(c))) => Some(c),
        Ok(Ok(None)) => None,
        Ok(Err(e)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(format!(
                    "Failed to get confirmation: {:?}",
                    e
                ))),
            )
        }
        Err(_) => return internal_error("Tracker thread response channel closed"),
    };

    let status = confirmation
        .as_ref()
        .map(|c| status_to_string(c.status))
        .unwrap_or_else(|| "local_only".to_string());

    let response = NoteStateResponse {
        issuer_pubkey: payload.issuer_pubkey,
        recipient_pubkey: payload.recipient_pubkey,
        local: note.amount_collected,
        confirmed: confirmation.as_ref().and_then(|c| c.confirmed_total_debt),
        pending: confirmation.as_ref().and_then(|c| c.pending_total_debt),
        already_redeemed: note.amount_redeemed,
        redeemable: confirmation
            .as_ref()
            .map(|c| c.is_redeemable(note.amount_redeemed))
            .unwrap_or(false),
        redeemable_amount: confirmation
            .as_ref()
            .map(|c| c.redeemable_amount(note.amount_redeemed))
            .unwrap_or(0),
        status,
    };

    (
        StatusCode::OK,
        Json(crate::models::success_response(response)),
    )
}

fn status_to_string(status: basis_store::NoteConfirmationStatus) -> String {
    match status {
        basis_store::NoteConfirmationStatus::LocalOnly => "local_only".to_string(),
        basis_store::NoteConfirmationStatus::Pending => "pending".to_string(),
        basis_store::NoteConfirmationStatus::Confirmed => "confirmed".to_string(),
    }
}

fn parse_pubkey(hex_str: &str) -> Result<PubKey, String> {
    let bytes = match hex::decode(hex_str) {
        Ok(b) => b,
        Err(_) => return Err("Public key must be hex-encoded".to_string()),
    };

    match bytes.try_into() {
        Ok(arr) => Ok(arr),
        Err(_) => Err("Public key must be 33 bytes".to_string()),
    }
}

fn internal_error<T>(msg: &str) -> (StatusCode, Json<ApiResponse<T>>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(crate::models::error_response(msg.to_string())),
    )
}

/// Fetch the confirmation summary for a note from the tracker thread.
async fn fetch_confirmation(
    tx: &tokio::sync::mpsc::Sender<TrackerCommand>,
    issuer_pubkey: PubKey,
    recipient_pubkey: PubKey,
    amount_redeemed: u64,
) -> Option<NoteConfirmationSummary> {
    match crate::tracker_request(tx, |response_tx| TrackerCommand::GetConfirmation {
        issuer_pubkey,
        recipient_pubkey,
        response_tx,
    })
    .await
    {
        Ok(Ok(Some(c))) => Some(NoteConfirmationSummary::from_confirmation(
            &c,
            amount_redeemed,
        )),
        _ => None,
    }
}
