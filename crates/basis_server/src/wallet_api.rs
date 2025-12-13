use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{
    models::{ApiResponse, CreateNoteRequest, SerializableIouNote, TrackerEvent, EventType},
    AppState, TrackerCommand,
};
use basis_store::NoteError;

// --- Models ---

#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub pubkey: String,
    pub total_debt: u64,
    pub collateral: u64,
    pub collateralization_ratio: f64,
    pub token_id: Option<String>,
    pub token_amount: Option<u64>,
    pub note_count: usize,
    pub recent_activity: Vec<WalletActivityItem>,
}

#[derive(Debug, Serialize)]
pub struct WalletActivityItem {
    pub timestamp: u64,
    pub activity_type: String, // "incoming_note", "outgoing_note", "redemption", etc.
    pub other_party: String,   // Pubkey of sender/receiver
    pub amount: u64,
    pub details: String,
}

#[derive(Debug, Serialize)]
pub struct WalletHistory {
    pub incoming_notes: Vec<SerializableIouNote>,
    pub outgoing_notes: Vec<SerializableIouNote>,
}

#[derive(Debug, Deserialize)]
pub struct SimplePaymentRequest {
    pub sender_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub signature: String, // Hex encoded signature of the IOU note
}

// --- Handlers ---

/// Get a unified wallet summary (balance + status + recent)
#[axum::debug_handler]
pub async fn get_wallet_summary(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<WalletSummary>>) {
    // 1. reuse logic from get_key_status for collateral/debt
    // Call the internal logic of get_key_status (we can't call the handler directly easily, so we duplicate the lightweight logic or refactor core logic later. For now, logic duplication is acceptable for "thin layer").
    
    // a. Validate Key
    let pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(b) if b.len() == 33 => b,
        _ => return (StatusCode::BAD_REQUEST, Json(crate::models::error_response("Invalid pubkey".into()))),
    };
    let issuer_pubkey: basis_store::PubKey = match pubkey_bytes.try_into() {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(crate::models::error_response("Invalid pubkey length".into()))),
    };

    // b. Get Debt (Outgoing Notes)
    let (tx_out, rx_out) = tokio::sync::oneshot::channel();
    let _ = state.tx.send(TrackerCommand::GetNotesByIssuer { 
        issuer_pubkey, 
        response_tx: tx_out 
    }).await;
    
    let outgoing_notes = match rx_out.await {
        Ok(Ok(notes)) => notes,
        _ => vec![],
    };
    
    let total_debt: u64 = outgoing_notes.iter().map(|n| n.outstanding_debt()).sum();
    let note_count = outgoing_notes.len();

    // c. Get Collateral
    let tracker = state.reserve_tracker.lock().await;
    let reserves = tracker.get_all_reserves();
    let reserve = reserves.into_iter().find(|r| r.owner_pubkey == pubkey_hex);
    
    let (collateral, ratio, _, token_id, token_amount) = if let Some(r) = reserve {
        (
            r.base_info.collateral_amount,
            r.collateralization_ratio(),
            r.last_updated_timestamp,
            r.base_info.token_id.clone(),
            r.base_info.token_amount,
        )
    } else {
        (0, if total_debt > 0 { 0.0 } else { 999999.0 }, 0, None, None)
    };
    
    // d. Get Recent Activity (from outgoing notes + maybe incoming notes)
    // For a simple summary, we list the last 5 outgoing notes created.
    // In a full implementation, we'd also query incoming notes and merge them.
    let mut recent_activity = Vec::new();
    
    // Convert outgoing notes to activity
    for note in outgoing_notes.iter().rev().take(5) {
        recent_activity.push(WalletActivityItem {
            timestamp: note.timestamp,
            activity_type: "outgoing_payment".to_string(),
            other_party: hex::encode(note.recipient_pubkey),
            amount: note.amount_collected,
            details: "Issued IOU note".to_string(),
        });
    }

    let summary = WalletSummary {
        pubkey: pubkey_hex,
        total_debt,
        collateral,
        collateralization_ratio: ratio,
        token_id,
        token_amount,
        note_count,
        recent_activity,
    };

    (StatusCode::OK, Json(crate::models::success_response(summary)))
}

/// Get simplified wallet history
#[axum::debug_handler]
pub async fn get_wallet_history(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<WalletHistory>>) {
    // 1. Validate Key
    let pubkey_bytes = match hex::decode(&pubkey_hex) {
        Ok(b) if b.len() == 33 => b,
        _ => return (StatusCode::BAD_REQUEST, Json(crate::models::error_response("Invalid pubkey".into()))),
    };
    let pubkey: basis_store::PubKey = match pubkey_bytes.try_into() {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(crate::models::error_response("Invalid pubkey length".into()))),
    };

    // 2. Get Outgoing
    let (tx_out, rx_out) = tokio::sync::oneshot::channel();
    let _ = state.tx.send(TrackerCommand::GetNotesByIssuer { 
        issuer_pubkey: pubkey, 
        response_tx: tx_out 
    }).await;
    let outgoing = rx_out.await.unwrap_or(Ok(vec![])).unwrap_or(vec![]);
    
    // 3. Get Incoming
    let (tx_in, rx_in) = tokio::sync::oneshot::channel();
    let _ = state.tx.send(TrackerCommand::GetNotesByRecipient { 
        recipient_pubkey: pubkey, 
        response_tx: tx_in 
    }).await;
    let incoming = rx_in.await.unwrap_or(Ok(vec![])).unwrap_or(vec![]);

    // 4. Transform
    let history = WalletHistory {
        outgoing_notes: outgoing.into_iter().map(|n| {
            let mut sn = SerializableIouNote::from(n);
            sn.issuer_pubkey = pubkey_hex.clone();
            sn
        }).collect(),
        incoming_notes: incoming.into_iter().map(|(issuer, n)| {
            let mut sn = SerializableIouNote::from(n);
            sn.issuer_pubkey = hex::encode(issuer);
            sn
        }).collect(),
    };

    (StatusCode::OK, Json(crate::models::success_response(history)))
}

/// Simple payment endpoint (Wrapper around CreateNote)
#[axum::debug_handler]
pub async fn send_payment(
    State(state): State<AppState>,
    Json(payload): Json<SimplePaymentRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    // Just delegate to create_note logic logic via `api::create_note` handler is tricky directly.
    // So we recreate the request.
    
    let note_req = CreateNoteRequest {
        issuer_pubkey: payload.sender_pubkey,
        recipient_pubkey: payload.recipient_pubkey,
        amount: payload.amount,
        timestamp: payload.timestamp,
        signature: payload.signature,
    };
    
    // Call the same logic as create_note
    crate::api::create_note(State(state), Json(note_req)).await
}
