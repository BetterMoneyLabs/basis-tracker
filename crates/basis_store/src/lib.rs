//! Core data structures for Basis tracker

pub mod avl_tree;
pub mod ergo_scanner;
pub mod persistence;
pub mod reserve_tracker;
pub mod tests;
pub mod schnorr_tests;
pub mod schnorr_verification_vectors;
pub mod cross_verification;
pub mod schnorr;

use secp256k1;

/// Public key type (Secp256k1)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1) - following chaincash-rs format: 33 bytes a + 32 bytes z
pub type Signature = [u8; 65];

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
}

/// Tracker state commitment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerState {
    /// AVL+ tree root digest of all notes (32 bytes label + 1 byte height)
    pub avl_root_digest: [u8; 33],
    /// Block height of last on-chain commitment
    pub last_commit_height: u64,
    /// Timestamp of last state update
    pub last_update_timestamp: u64,
}

/// Reserve information for a public key
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    StorageError(String),
}

impl From<secp256k1::Error> for NoteError {
    fn from(_: secp256k1::Error) -> Self {
        NoteError::InvalidSignature
    }
}

/// Tracker state manager with AVL tree
pub struct TrackerStateManager {
    avl_state: avl_tree::AvlTreeState,
    current_state: TrackerState,
    storage: persistence::NoteStorage,
}

impl TrackerStateManager {
    /// Create a new tracker state manager
    pub fn new() -> Self {
        eprintln!("Creating TrackerStateManager...");
        let avl_state = avl_tree::AvlTreeState::new();

        // Use a temporary directory for storage (in real implementation, this would be configurable)
        eprintln!("Opening note storage...");
        let storage = match persistence::NoteStorage::open("crates/basis_server/data/notes") {
            Ok(storage) => {
                eprintln!("Note storage opened successfully");
                storage
            },
            Err(e) => {
                eprintln!("Failed to initialize note storage: {:?}", e);
                // Fallback to in-memory storage if file storage fails
                // In production, this should handle errors properly
                panic!("Failed to initialize note storage: {:?}", e);
            }
        };

        eprintln!("TrackerStateManager created successfully");
        Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
        }
    }

    /// Add a new note to the tracker state
    pub fn add_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Store note in persistent storage
        self.storage.store_note(issuer_pubkey, note)?;

        // Update AVL tree state
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        
        // Create value bytes matching persistence format
        let mut value_bytes = Vec::new();
        value_bytes.extend_from_slice(issuer_pubkey);
        value_bytes.extend_from_slice(&note.amount.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        // Try to insert, if key already exists then update
        if let Err(_e) = self.avl_state.insert(key.to_bytes(), value_bytes.clone()) {
            // For any error, try to update instead (assuming key already exists)
            self.avl_state
                .update(key.to_bytes(), value_bytes)
                .map_err(|e| NoteError::StorageError(e))?;
        }

        self.update_state();
        Ok(())
    }

    /// Update an existing note
    pub fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Update note in persistent storage
        self.storage.store_note(issuer_pubkey, note)?;

        // Update AVL tree state
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let value_bytes = [
            &note.amount.to_be_bytes()[..],
            &note.timestamp.to_be_bytes()[..],
        ]
        .concat();

        self.avl_state
            .update(key.to_bytes(), value_bytes)
            .map_err(|e| NoteError::StorageError(e))?;

        self.update_state();
        Ok(())
    }

    /// Remove a note from the tracker state
    pub fn remove_note(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<(), NoteError> {
        // Remove note from persistent storage
        self.storage.remove_note(issuer_pubkey, recipient_pubkey)?;

        // Update AVL tree state
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        self.avl_state
            .remove(key.to_bytes())
            .map_err(|e| NoteError::StorageError(e))?;

        self.update_state();
        Ok(())
    }

    /// Generate proof for a specific note
    pub fn generate_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<NoteProof, NoteError> {
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
    pub fn lookup_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<IouNote, NoteError> {
        self.storage
            .get_note(issuer_pubkey, recipient_pubkey)?
            .ok_or_else(|| NoteError::StorageError("Note not found".to_string()))
    }

    /// Get all notes for a specific issuer
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError> {
        self.storage.get_issuer_notes(issuer_pubkey)
    }

    /// Get all notes for a specific recipient
    pub fn get_recipient_notes(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        self.storage.get_recipient_notes(recipient_pubkey)
    }

    /// Get all notes in the tracker
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError> {
        self.storage.get_all_notes()
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
    ) -> Self {
        Self {
            recipient_pubkey,
            amount,
            timestamp,
            signature,
        }
    }

    /// Create and sign a new IOU note using the chaincash-rs Schnorr signature approach
    pub fn create_and_sign(
        recipient_pubkey: PubKey,
        amount: u64,
        timestamp: u64,
        issuer_secret_key: &[u8; 32],
    ) -> Result<Self, NoteError> {
        use secp256k1::{Secp256k1, SecretKey};
        
        let secp = Secp256k1::new();
        
        // Parse the secret key
        let secret_key = SecretKey::from_slice(issuer_secret_key)
            .map_err(|_| NoteError::InvalidSignature)?;
        
        // Generate the corresponding public key
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = public_key.serialize();
        
        // Generate the signing message (same format as chaincash-rs)
        let message = schnorr::signing_message(&recipient_pubkey, amount, timestamp);
        
        // Use the chaincash-rs approach for Schnorr signing
        let signature = schnorr::schnorr_sign(&message, &secret_key, &issuer_pubkey);
        
        Ok(Self {
            recipient_pubkey,
            amount,
            timestamp,
            signature,
        })
    }

    /// Generate the message that should be signed
    pub fn signing_message(&self) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&self.recipient_pubkey);
        message.extend_from_slice(&self.amount.to_be_bytes());
        message.extend_from_slice(&self.timestamp.to_be_bytes());
        message
    }

    /// Verify the signature against an issuer public key using Schnorr signature verification
    /// This follows the chaincash-rs approach for Schnorr signature verification
    pub fn verify_signature(&self, issuer_pubkey: &PubKey) -> Result<(), NoteError> {
        // Generate the signing message
        let message = self.signing_message();
        
        // Use the chaincash-rs approach for Schnorr verification
        schnorr::schnorr_verify(&self.signature, &message, issuer_pubkey)
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



// Re-export reserve tracker types
pub use reserve_tracker::{ExtendedReserveInfo, ReserveTracker, ReserveTrackerError};

// Re-export ergo scanner types
pub use ergo_scanner::{ErgoScanner, ErgoScannerError, NodeConfig, ReserveEvent, ErgoBox};
