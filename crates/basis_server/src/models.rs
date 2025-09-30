use basis_store::IouNote;
use serde::{Deserialize, Serialize};

// Request structure for creating a new IOU note
// Using hex-encoded strings for public keys and signature
#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub signature: String,
    pub issuer_pubkey: String,
}

// Response structure for API responses
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
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
    pub id: u64,
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
pub struct SerializableIouNote {
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub signature: String,
}

impl From<IouNote> for SerializableIouNote {
    fn from(note: IouNote) -> Self {
        Self {
            recipient_pubkey: hex::encode(note.recipient_pubkey),
            amount: note.amount_collected,
            timestamp: note.timestamp,
            signature: hex::encode(note.signature),
        }
    }
}

// Key status response
#[derive(Debug, Serialize)]
pub struct KeyStatusResponse {
    pub total_debt: u64,
    pub collateral: u64,
    pub collateralization_ratio: f64,
    pub note_count: usize,
    pub last_updated: u64,
    pub issuer_pubkey: String,
}

// Redemption request
#[derive(Debug, Deserialize)]
pub struct RedeemRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
}

// Redemption completion request
#[derive(Debug, Deserialize)]
pub struct CompleteRedemptionRequest {
    pub redemption_id: String,
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub redeemed_amount: u64,
}

// Redemption response
#[derive(Debug, Serialize)]
pub struct RedeemResponse {
    pub redemption_id: String,
    pub amount: u64,
    pub timestamp: u64,
    pub proof_available: bool,
    pub transaction_pending: bool,
}

// Proof response
#[derive(Debug, Serialize)]
pub struct ProofResponse {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub proof_data: String,
    pub tracker_state_digest: String,
    pub block_height: u64,
    pub timestamp: u64,
}

// Success response helper
pub fn success_response<T>(data: T) -> ApiResponse<T> {
    ApiResponse {
        success: true,
        data: Some(data),
        error: None,
    }
}

// Error response helper
pub fn error_response<T>(message: String) -> ApiResponse<T> {
    ApiResponse {
        success: false,
        data: None,
        error: Some(message),
    }
}
