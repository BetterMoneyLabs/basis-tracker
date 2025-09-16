//! Core data structures for Basis tracker

pub mod avl_tree;
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
    /// AVL+ tree root digest of all notes
    pub avl_root_digest: [u8; 32],
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
    /// AVL tree proof bytes
    pub avl_proof: Vec<u8>,
    /// Operations performed to generate the proof
    pub operations: Vec<u8>,
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

/// Tracker state manager with AVL tree
pub struct TrackerStateManager {
    avl_state: avl_tree::AvlTreeState,
    current_state: TrackerState,
}

impl TrackerStateManager {
    /// Create a new tracker state manager
    pub fn new() -> Self {
        let avl_state = avl_tree::AvlTreeState::new();
        
        Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 32],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
        }
    }

    /// Add a new note to the tracker state
    pub fn add_note(&mut self, note: &IouNote) -> Result<(), NoteError> {
        // Extract issuer public key from signature (placeholder implementation)
        // In real implementation, we'd recover the public key from signature
        let issuer_pubkey = [0u8; 33]; // Placeholder
        let key = NoteKey::from_keys(&issuer_pubkey, &note.recipient_pubkey);
        let value_bytes = [
            &note.amount.to_be_bytes()[..],
            &note.timestamp.to_be_bytes()[..],
            &note.nonce.to_be_bytes()[..],
        ].concat();

        self.avl_state.insert(key.to_bytes(), value_bytes)
            .map_err(|e| NoteError::StorageError(e))?;
        
        self.update_state();
        Ok(())
    }

    /// Update an existing note
    pub fn update_note(&mut self, note: &IouNote) -> Result<(), NoteError> {
        // Extract issuer public key from signature (placeholder implementation)
        let issuer_pubkey = [0u8; 33]; // Placeholder
        let key = NoteKey::from_keys(&issuer_pubkey, &note.recipient_pubkey);
        let value_bytes = [
            &note.amount.to_be_bytes()[..],
            &note.timestamp.to_be_bytes()[..],
            &note.nonce.to_be_bytes()[..],
        ].concat();

        self.avl_state.update(key.to_bytes(), value_bytes)
            .map_err(|e| NoteError::StorageError(e))?;
        
        self.update_state();
        Ok(())
    }

    /// Remove a note from the tracker state
    pub fn remove_note(&mut self, issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Result<(), NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        self.avl_state.remove(key.to_bytes())
            .map_err(|e| NoteError::StorageError(e))?;
        
        self.update_state();
        Ok(())
    }

    /// Generate proof for a specific note
    pub fn generate_proof(&mut self, issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Result<NoteProof, NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let _key_bytes = key.to_bytes();

        // For AVL trees, the proof is generated during lookup operations
        // In a real implementation, we'd need to track operations for proof generation
        let proof = self.avl_state.generate_proof();
        
        // Placeholder for operations - in real implementation, this would track
        // the specific operations that led to the current state
        let operations = Vec::new();

        // Lookup the note to include in proof
        let note = self.lookup_note(issuer_pubkey, recipient_pubkey)?;

        Ok(NoteProof {
            note,
            avl_proof: proof,
            operations,
        })
    }

    /// Lookup a note by issuer and recipient
    pub fn lookup_note(&self, issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Result<IouNote, NoteError> {
        let _key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);

        // This would need actual storage implementation
        // For now, return a placeholder error
        Err(NoteError::StorageError("Note storage not implemented".to_string()))
    }

    /// Update the current state with latest AVL tree root
    fn update_state(&mut self) {
        self.current_state.avl_root_digest = self.avl_state.root_digest();
        // Update timestamp would be set to current time in real implementation
    }

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState {
        &self.current_state
    }
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

    /// Convert note key to bytes for AVL tree
    pub fn to_bytes(&self) -> Vec<u8> {
        [&self.issuer_hash[..], &self.recipient_hash[..]].concat()
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