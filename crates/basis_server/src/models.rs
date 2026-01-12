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
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
}

// Serializable version of IouNote for API responses with age
#[derive(Debug, Serialize)]
pub struct SerializableIouNoteWithAge {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount_collected: u64,
    pub amount_redeemed: u64,
    pub timestamp: u64,
    pub signature: String,
    pub age_seconds: u64,
}

impl From<IouNote> for SerializableIouNote {
    fn from(note: IouNote) -> Self {
        Self {
            issuer_pubkey: "".to_string(), // Will be set by the API handler
            recipient_pubkey: hex::encode(note.recipient_pubkey),
            amount_collected: note.amount_collected,
            amount_redeemed: note.amount_redeemed,
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
    /// Prepared transaction data that can be submitted to Ergo node
    /// Contains all necessary fields for wallet payment API
    pub transaction_data: Option<TransactionData>,
}

// Transaction data that can be submitted to Ergo node
#[derive(Debug, Serialize)]
pub struct TransactionData {
    /// Target address for the transaction
    pub address: String,
    /// Value in nanoERG
    pub value: u64,
    /// Register values
    pub registers: std::collections::HashMap<String, String>,
    /// Assets to include in transaction
    pub assets: Vec<TokenData>,
    /// Transaction fee
    pub fee: u64,
}

// Token/Asset data for transaction
#[derive(Debug, Serialize)]
pub struct TokenData {
    pub token_id: String,
    pub amount: u64,
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

// Request for tracker signature
#[derive(Debug, Deserialize)]
pub struct TrackerSignatureRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub recipient_address: String,
    pub reserve_box_id: String,
}

// Response for tracker signature
#[derive(Debug, Serialize)]
pub struct TrackerSignatureResponse {
    pub success: bool,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub message_signed: String,
}

// Request for redemption preparation
#[derive(Debug, Deserialize)]
pub struct RedemptionPreparationRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
}

// Response for redemption preparation
#[derive(Debug, Serialize)]
pub struct RedemptionPreparationResponse {
    pub redemption_id: String,
    pub avl_proof: String,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub tracker_state_digest: String,
    pub block_height: u64,
}

// Request for creating a reserve
#[derive(Debug, Deserialize)]
pub struct CreateReserveRequest {
    pub nft_id: String,
    pub owner_pubkey: String,
    pub erg_amount: u64,
}

// Response for reserve creation - formatted for Ergo node's /wallet/payment/send API
#[derive(Debug, Clone, Serialize)]
pub struct ReserveCreationResponse {
    pub requests: Vec<ReservePaymentRequest>,
    pub fee: u64,
    pub change_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReservePaymentRequest {
    pub address: String,
    pub value: u64,
    pub assets: Vec<Asset>,
    pub registers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    pub token_id: String,
    pub amount: u64,
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
