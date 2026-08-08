use basis_store::IouNote;
use serde::{Deserialize, Serialize};

/// Confirmation summary attached to note responses.
/// Mirrors the store-side `NoteConfirmation` but includes the
/// redeemable flag/amount computed for the client.
#[derive(Debug, Serialize)]
pub struct NoteConfirmationSummary {
    pub status: String,
    pub confirmed_total_debt: Option<u64>,
    pub pending_total_debt: Option<u64>,
    pub confirmed_box_id: Option<String>,
    pub confirmed_height: Option<u64>,
    pub pending_tx_id: Option<String>,
    pub redeemable: bool,
    pub redeemable_amount: u64,
}

impl NoteConfirmationSummary {
    /// Build a summary from a store confirmation and the already-redeemed amount.
    pub fn from_confirmation(
        confirmation: &basis_store::NoteConfirmation,
        already_redeemed: u64,
    ) -> Self {
        Self {
            status: status_to_string(confirmation.status),
            confirmed_total_debt: confirmation.confirmed_total_debt,
            pending_total_debt: confirmation.pending_total_debt,
            confirmed_box_id: confirmation.confirmed_box_id.clone(),
            confirmed_height: confirmation.confirmed_height,
            pending_tx_id: confirmation.pending_tx_id.clone(),
            redeemable: confirmation.is_redeemable(already_redeemed),
            redeemable_amount: confirmation.redeemable_amount(already_redeemed),
        }
    }
}

fn status_to_string(status: basis_store::NoteConfirmationStatus) -> String {
    match status {
        basis_store::NoteConfirmationStatus::LocalOnly => "local_only".to_string(),
        basis_store::NoteConfirmationStatus::Pending => "pending".to_string(),
        basis_store::NoteConfirmationStatus::Confirmed => "confirmed".to_string(),
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<NoteConfirmationSummary>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<NoteConfirmationSummary>,
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
            confirmation: None,
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
    pub has_pending_refund: bool,
}

// Redemption request
#[derive(Debug, Deserialize)]
pub struct RedeemRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    /// Reserve box ID (optional - will be looked up if not provided)
    #[serde(default)]
    pub reserve_box_id: String,
    /// Recipient address for redemption output (optional - derived from recipient_pubkey if not provided)
    #[serde(default)]
    pub recipient_address: String,
    /// Issuer's Schnorr signature (65 bytes, hex encoded = 130 chars)
    pub issuer_signature: String,
    /// Whether this is an emergency redemption
    #[serde(default)]
    pub emergency: bool,
}

// Redemption completion request
#[derive(Debug, Deserialize)]
pub struct CompleteRedemptionRequest {
    pub redemption_id: String,
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub redeemed_amount: u64,
    /// Optional explicit cumulative reserve-tree value (from the on-chain build).
    /// When omitted, the note's cumulative redeemed amount is used.
    #[serde(default)]
    pub new_already_redeemed: Option<u64>,
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
    /// Raw Ergo transaction JSON (hex encoded) that can be signed and submitted
    pub transaction_bytes: Option<String>,
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
// Following specs/server/redemption_state_spec.md - POST /tracker/signature
#[derive(Debug, Deserialize)]
pub struct TrackerSignatureRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub total_debt: u64,
    /// Payment timestamp in milliseconds since Unix epoch
    pub timestamp: u64,
    #[serde(default)]
    pub emergency: bool,
}

// Response for tracker signature
// Following specs/server/redemption_state_spec.md
#[derive(Debug, Serialize)]
pub struct TrackerSignatureResponse {
    pub success: bool,
    pub tracker_signature: String,
    pub tracker_pubkey: String,
    pub message_signed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_emergency: Option<bool>,
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

// Redemption proof response - following specs/server/redemption_state_spec.md
// GET /proof/redemption endpoint response
#[derive(Debug, Serialize)]
pub struct RedemptionProofResponse {
    pub success: bool,
    pub data: RedemptionProofData,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionProofData {
    pub tracker_lookup_proof: String,
    pub reserve_lookup_proof: Option<String>,
    pub reserve_insert_proof: String,
    pub tracker_state_digest: String,
    pub reserve_state_digest: String,
    pub total_debt: u64,
    pub already_redeemed: u64,
    pub proof_valid: bool,
    pub is_first_redemption: bool,
}

// Tracker lookup proof response - for context var #8
// GET /tracker/proof endpoint response
#[derive(Debug, Serialize)]
pub struct TrackerProofResponse {
    pub success: bool,
    pub data: TrackerProofData,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TrackerProofData {
    /// Hex-encoded AVL tree key: hash(ownerKey || receiverKey)
    pub key: String,
    /// Hex-encoded value: totalDebt as 8-byte big-endian
    pub value: String,
    /// Hex-encoded AVL proof bytes
    pub proof: String,
    /// Total debt as integer
    pub total_debt: u64,
    /// Current tracker state digest (R5 register value)
    pub tracker_state_digest: String,
}

// Reserve lookup proof response - for context var #7
// GET /reserve/proof endpoint response
#[derive(Debug, Serialize)]
pub struct ReserveProofData {
    /// Hex-encoded AVL tree key: hash(ownerKey || receiverKey)
    pub key: String,
    /// Hex-encoded value: already_redeemed as 8-byte big-endian
    pub value: String,
    /// Hex-encoded AVL proof bytes (None for first redemption) - for context var #7 (lookup)
    pub proof: Option<String>,
    /// Already redeemed amount as integer
    pub already_redeemed: u64,
    /// Whether this is the first redemption (no lookup proof needed)
    pub is_first_redemption: bool,
    /// Hex-encoded AVL insert proof for context var #5 (insert operation)
    /// This proof is used to INSERT the new already_redeemed amount into the reserve tree
    pub insert_proof: String,
    /// Hex-encoded updated reserve state digest after the insert operation (R5 register value)
    pub new_reserve_state_digest: String,
}

// Redemption preparation response - updated with new fields
#[derive(Debug, Serialize)]
pub struct RedemptionPreparationResponseV2 {
    pub success: bool,
    pub data: RedemptionPreparationData,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RedemptionPreparationData {
    pub redemption_id: String,
    pub tracker_lookup_proof: String,
    pub reserve_lookup_proof: Option<String>,
    pub reserve_insert_proof: String,
    pub tracker_signature: String,
    pub reserve_signature: String,
    pub tracker_pubkey: String,
    pub tracker_state_digest: String,
    pub reserve_state_digest: String,
    pub total_debt: u64,
    pub already_redeemed: u64,
    pub is_first_redemption: bool,
    pub transaction_bytes: String,
}

// Request for creating a reserve
#[derive(Debug, Deserialize)]
pub struct CreateReserveRequest {
    pub nft_id: String,
    pub owner_pubkey: String,
    pub erg_amount: u64,
}

// Response for reserve creation - formatted for Ergo node's /wallet/payment/send API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveCreationResponse {
    pub requests: Vec<ReservePaymentRequest>,
    pub fee: u64,
    pub change_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservePaymentRequest {
    pub address: String,
    pub value: u64,
    pub assets: Vec<Asset>,
    pub registers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Serialize)]
pub struct TrackerBoxIdResponse {
    pub tracker_box_id: String,
    pub timestamp: u64,
    pub height: u64,
}

// Tracker state response - shows local, confirmed and pending digests.
#[derive(Debug, Clone, Serialize)]
pub struct TrackerStateResponse {
    pub local_digest: String,
    pub confirmed_digest: Option<String>,
    pub confirmed_box_id: Option<String>,
    pub confirmed_height: Option<u64>,
    pub pending_digest: Option<String>,
    pub pending_tx_id: Option<String>,
    pub pending_submitted_height: Option<u64>,
    pub tracker_box_id: Option<String>,
}

// Pending tracker update transaction response.
#[derive(Debug, Clone, Serialize)]
pub struct PendingTxResponse {
    pub pending_tx_id: Option<String>,
    pub pending_digest: Option<String>,
    pub submitted_height: Option<u64>,
}

// Request for a single note's confirmation state.
#[derive(Debug, Deserialize)]
pub struct NoteStateRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
}

// Response for a single note's confirmation state.
#[derive(Debug, Clone, Serialize)]
pub struct NoteStateResponse {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub local: u64,
    pub confirmed: Option<u64>,
    pub pending: Option<u64>,
    pub already_redeemed: u64,
    pub redeemable: bool,
    pub redeemable_amount: u64,
    pub status: String,
}

// Request for checking note acceptance
#[derive(Debug, Deserialize)]
pub struct CheckAcceptanceRequest {
    /// Hex-encoded issuer public key (33 bytes)
    pub issuer_pubkey: String,
    /// Total cumulative debt amount
    pub total_debt: u64,
    /// Optional hex-encoded recipient public key (33 bytes) for per-recipient policy lookup
    #[serde(default)]
    pub recipient_pubkey: Option<String>,
}

// Response for checking note acceptance
#[derive(Debug, Serialize)]
pub struct CheckAcceptanceResponse {
    /// Whether the note is acceptable
    pub acceptable: bool,
    /// Optional reason for rejection
    pub reason: Option<String>,
}

// Request for uploading acceptance policy
#[derive(Debug, Deserialize)]
pub struct UploadPolicyRequest {
    /// Hex-encoded recipient public key (33 bytes)
    pub recipient_pubkey: String,
    /// Policy JSON string (serialized AcceptanceConfig)
    pub policy_json: String,
    /// Schnorr signature over policy_json (65 bytes, hex encoded)
    pub signature: String,
}

// Response for uploading acceptance policy
#[derive(Debug, Serialize)]
pub struct UploadPolicyResponse {
    /// Unix timestamp when policy was uploaded
    pub uploaded_at: u64,
    /// Hash of the policy for verification
    pub policy_hash: String,
}

// Response for getting a recipient's acceptance policy
#[derive(Debug, Serialize)]
pub struct GetPolicyResponse {
    /// Recipient public key (hex encoded)
    pub recipient_pubkey: String,
    /// Policy JSON string
    pub policy_json: String,
    /// Hex-encoded Schnorr signature (65 bytes = 130 hex chars)
    pub signature: String,
    /// Unix timestamp when policy was uploaded
    pub uploaded_at: u64,
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
