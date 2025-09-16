//! Core data structures for Basis tracker

pub mod tests;

/// Public key type (Secp256k1)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1)
pub type Signature = [u8; 64];

/// IOU Note representing debt from A to B
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IouNote {
    /// Recipient's public key
    pub recipient_pubkey: PubKey,
    /// Total amount of debt
    pub amount: u64,
    /// Timestamp of latest payment/update
    pub timestamp: u64,
    /// Signature from issuer (A)
    pub signature: Signature,
    /// Nonce to prevent replay attacks
    pub nonce: u64,
}

/// Tracker state commitment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerState {
    /// Merkle root hash of all notes
    pub merkle_root: [u8; 32],
    /// Block height of last on-chain commitment
    pub last_commit_height: u64,
    /// Timestamp of last state update
    pub last_update_timestamp: u64,
}

/// Reserve information for a public key
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveInfo {
    /// On-chain collateral amount
    pub collateral_amount: u64,
    /// Last known block height
    pub last_updated_height: u64,
    /// Reserve contract address
    pub contract_address: String,
}

/// Proof for a specific note against tracker state
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteProof {
    /// The IOU note being proven
    pub note: IouNote,
    /// Merkle proof path
    pub merkle_proof: Vec<[u8; 32]>,
    /// Index in the Merkle tree
    pub index: u64,
}

/// Key for note lookup (hash of issuer + recipient)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteKey {
    /// Hash of issuer public key
    pub issuer_hash: [u8; 32],
    /// Hash of recipient public key
    pub recipient_hash: [u8; 32],
}

/// Status information for a public key
#[derive(Debug, Clone, PartialEq)]
pub struct KeyStatus {
    /// Total issued debt
    pub total_debt: u64,
    /// On-chain collateral
    pub collateral: u64,
    /// Collateralization ratio (collateral / debt)
    pub collateralization_ratio: f64,
    /// Number of outstanding notes
    pub note_count: usize,
    /// Last update timestamp
    pub last_updated: u64,
}

/// Error types for note operations
#[derive(Debug)]
pub enum NoteError {
    InvalidSignature,
    AmountOverflow,
    FutureTimestamp,
    RedemptionTooEarly,
    InsufficientCollateral,
    DuplicateNonce,
    StorageError(String),
}

impl IouNote {
    /// Create a new IOU note
    pub fn new(
        recipient_pubkey: PubKey,
        amount: u64,
        timestamp: u64,
        signature: Signature,
        nonce: u64,
    ) -> Self {
        Self {
            recipient_pubkey,
            amount,
            timestamp,
            signature,
            nonce,
        }
    }

    /// Generate the message that should be signed
    pub fn signing_message(&self) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&self.recipient_pubkey);
        message.extend_from_slice(&self.amount.to_be_bytes());
        message.extend_from_slice(&self.timestamp.to_be_bytes());
        message.extend_from_slice(&self.nonce.to_be_bytes());
        message
    }

    /// Verify the signature against an issuer public key
    pub fn verify_signature(&self, _issuer_pubkey: &PubKey) -> Result<(), NoteError> {
        // TODO: Implement actual Secp256k1 signature verification
        // For now, this is a placeholder
        if self.signature == [0u8; 64] {
            return Err(NoteError::InvalidSignature);
        }
        Ok(())
    }
}

impl NoteKey {
    /// Create a note key from issuer and recipient public keys
    pub fn from_keys(issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Self {
        // Simple XOR-based hash for now (replace with proper crypto later)
        let issuer_hash = simple_hash(issuer_pubkey);
        let recipient_hash = simple_hash(recipient_pubkey);
        
        Self {
            issuer_hash,
            recipient_hash,
        }
    }
}

/// Simple hash function for prototyping (replace with proper crypto)
pub fn simple_hash(data: &[u8]) -> [u8; 32] {
    let mut result = [0u8; 32];
    for (i, &byte) in data.iter().enumerate() {
        result[i % 32] ^= byte;
    }
    result
}