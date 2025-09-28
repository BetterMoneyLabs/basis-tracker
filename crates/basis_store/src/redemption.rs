//! Redemption flow for Basis offchain notes

use thiserror::Error;

use crate::{IouNote, NoteError, PubKey, TrackerStateManager};

#[derive(Error, Debug)]
pub enum RedemptionError {
    #[error("Note not found")]
    NoteNotFound,
    #[error("Invalid note signature")]
    InvalidNoteSignature,
    #[error("Redemption too early: {0} < {1}")]
    RedemptionTooEarly(u64, u64),
    #[error("Insufficient collateral: {0} < {1}")]
    InsufficientCollateral(u64, u64),
    #[error("Reserve not found: {0}")]
    ReserveNotFound(String),
    #[error("Transaction building error: {0}")]
    TransactionError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
}

impl From<NoteError> for RedemptionError {
    fn from(err: NoteError) -> Self {
        match err {
            NoteError::InvalidSignature => RedemptionError::InvalidNoteSignature,
            NoteError::RedemptionTooEarly => RedemptionError::RedemptionTooEarly(0, 0),
            NoteError::StorageError(msg) => RedemptionError::StorageError(msg),
            _ => RedemptionError::StorageError(format!("{:?}", err)),
        }
    }
}

/// Redemption request parameters
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RedemptionRequest {
    /// Issuer's public key (hex encoded)
    pub issuer_pubkey: String,
    /// Recipient's public key (hex encoded)
    pub recipient_pubkey: String,
    /// Amount to redeem
    pub amount: u64,
    /// Timestamp of the note being redeemed
    pub timestamp: u64,
    /// Reserve contract box ID (hex encoded)
    pub reserve_box_id: String,
    /// Recipient's address for redemption output
    pub recipient_address: String,
}

/// Redemption proof and transaction data
#[derive(Debug, Clone)]
pub struct RedemptionData {
    /// Unique redemption ID
    pub redemption_id: String,
    /// The note being redeemed
    pub note: IouNote,
    /// AVL tree proof for the note
    pub avl_proof: Vec<u8>,
    /// Redemption transaction bytes (hex encoded)
    pub transaction_bytes: String,
    /// Required signatures for the transaction
    pub required_signatures: Vec<String>,
    /// Estimated transaction fee
    pub estimated_fee: u64,
    /// Timestamp when redemption can be executed
    pub redemption_time: u64,
}

/// Redemption manager for handling note redemptions
pub struct RedemptionManager {
    pub tracker: TrackerStateManager,
}

impl RedemptionManager {
    /// Create a new redemption manager
    pub fn new(tracker: TrackerStateManager) -> Self {
        Self { tracker }
    }

    /// Initiate redemption process for a note
    pub fn initiate_redemption(
        &mut self,
        request: &RedemptionRequest,
    ) -> Result<RedemptionData, RedemptionError> {
        // Parse public keys
        let issuer_pubkey = parse_pubkey(&request.issuer_pubkey)?;
        let recipient_pubkey = parse_pubkey(&request.recipient_pubkey)?;

        // Lookup the note
        let note = self
            .tracker
            .lookup_note(&issuer_pubkey, &recipient_pubkey)
            .map_err(|_| RedemptionError::NoteNotFound)?;

        // Verify note signature
        note.verify_signature(&issuer_pubkey)
            .map_err(|_| RedemptionError::InvalidNoteSignature)?;

        // Verify note matches redemption request and has sufficient outstanding debt
        if note.amount_collected != request.amount || note.timestamp != request.timestamp {
            return Err(RedemptionError::InvalidNoteSignature);
        }
        
        // Check if there's sufficient outstanding debt to redeem
        if note.outstanding_debt() < request.amount {
            return Err(RedemptionError::InsufficientCollateral(
                note.outstanding_debt(),
                request.amount,
            ));
        }

        // Check redemption time lock (1 week minimum)
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let min_redemption_time = note.timestamp + 7 * 24 * 60 * 60; // 1 week in seconds
        if current_time < min_redemption_time {
            return Err(RedemptionError::RedemptionTooEarly(
                current_time,
                min_redemption_time,
            ));
        }

        // Generate proof for the note
        let proof = self
            .tracker
            .generate_proof(&issuer_pubkey, &recipient_pubkey)?;

        // Build redemption transaction
        let redemption_data = self.build_redemption_transaction(&note, &proof, request)?;

        Ok(redemption_data)
    }

    /// Build redemption transaction
    fn build_redemption_transaction(
        &self,
        note: &IouNote,
        proof: &crate::NoteProof,
        request: &RedemptionRequest,
    ) -> Result<RedemptionData, RedemptionError> {
        // In a real implementation, this would:
        // 1. Fetch the reserve box from the blockchain
        // 2. Create the redemption transaction following the contract logic
        // 3. Include the AVL proof and signatures
        // 4. Calculate appropriate fees

        // For now, create a mock transaction structure
        let redemption_id = format!(
            "redeem_{}_{}_{}",
            &request.issuer_pubkey[..16],
            &request.recipient_pubkey[..16],
            note.timestamp
        );

        // Mock transaction bytes (in real implementation, this would be actual transaction bytes)
        let transaction_bytes = hex::encode(format!(
            "mock_tx_{}_{}_{}",
            request.reserve_box_id, request.amount, note.timestamp
        ));

        // Required signatures: issuer and tracker
        let required_signatures = vec![
            request.issuer_pubkey.clone(),
            "tracker_signature_key".to_string(), // Placeholder for tracker signature
        ];

        // Estimated fee (0.001 ERG)
        let estimated_fee = 1000000;

        // Redemption can happen immediately since we checked the time lock
        let redemption_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(RedemptionData {
            redemption_id,
            note: note.clone(),
            avl_proof: proof.avl_proof.clone(),
            transaction_bytes,
            required_signatures,
            estimated_fee,
            redemption_time,
        })
    }

    /// Complete redemption by updating the note with redeemed amount
    pub fn complete_redemption(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        redeemed_amount: u64,
    ) -> Result<(), RedemptionError> {
        // Get the current note
        let mut note = self.tracker
            .lookup_note(issuer_pubkey, recipient_pubkey)
            .map_err(|_| RedemptionError::NoteNotFound)?;
        
        // Update the redeemed amount
        note.amount_redeemed += redeemed_amount;
        
        // Update the note in tracker
        self.tracker
            .update_note(issuer_pubkey, &note)
            .map_err(RedemptionError::from)
    }

    /// Verify redemption proof against on-chain state
    pub fn verify_redemption_proof(
        &self,
        _proof: &[u8],
        note: &IouNote,
        issuer_pubkey: &PubKey,
    ) -> Result<bool, RedemptionError> {
        // In a real implementation, this would verify the AVL proof against
        // the on-chain commitment stored in the reserve contract

        // For now, just verify the note signature
        note.verify_signature(issuer_pubkey)
            .map(|_| true)
            .map_err(|_| RedemptionError::InvalidNoteSignature)
    }
}

/// Parse hex-encoded public key
fn parse_pubkey(hex_str: &str) -> Result<PubKey, RedemptionError> {
    let bytes = hex::decode(hex_str)
        .map_err(|_| RedemptionError::InvalidPublicKey("Invalid hex encoding".to_string()))?;

    bytes
        .try_into()
        .map_err(|_| RedemptionError::InvalidPublicKey("Must be 33 bytes".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IouNote;

    #[test]
    fn test_redemption_validation() {
        let mut tracker = TrackerStateManager::new();
        let redemption_manager = RedemptionManager::new(tracker);

        // Test public key parsing
        let valid_pubkey = "02".to_string() + &"0".repeat(64); // 33 bytes hex
        let parsed = parse_pubkey(&valid_pubkey);
        assert!(parsed.is_ok());

        // Test invalid hex
        let invalid_hex = "zz".to_string();
        let parsed = parse_pubkey(&invalid_hex);
        assert!(parsed.is_err());

        // Test wrong length
        let wrong_length = "02".to_string() + &"0".repeat(62); // 32 bytes
        let parsed = parse_pubkey(&wrong_length);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_redemption_request_validation() {
        let request = RedemptionRequest {
            issuer_pubkey: "02".to_string() + &"0".repeat(64),
            recipient_pubkey: "02".to_string() + &"1".repeat(64),
            amount: 1000,
            timestamp: 1672531200, // Jan 1, 2023
            reserve_box_id: "box123".to_string(),
            recipient_address: "9".repeat(51), // Ergo address format
        };

        // Should parse valid public keys
        let issuer = parse_pubkey(&request.issuer_pubkey);
        let recipient = parse_pubkey(&request.recipient_pubkey);
        assert!(issuer.is_ok());
        assert!(recipient.is_ok());
    }
}