//! Core data structures for Basis tracker

pub mod avl_tree;
pub mod contract_compiler;
pub mod cross_verification;
pub mod ergo_scanner;
pub mod persistence;
pub mod redemption;
#[cfg(test)]
pub mod redemption_blockchain_tests;
#[cfg(test)]
pub mod redemption_simple_tests;
pub mod reserve_tracker;
pub mod schnorr;
pub mod schnorr_tests;
#[cfg(test)]
pub mod simple_integration_tests;
pub mod tests;

// Test modules
#[cfg(test)]
pub mod cross_verification_tests;
#[cfg(test)]
pub mod property_tests;
#[cfg(test)]
pub mod real_scanner_integration_tests;
#[cfg(test)]
pub mod test_helpers;


use blake2::{Blake2b512, Digest};
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
    /// Total amount ever collected (cumulative debt)
    pub amount_collected: u64,
    /// Total amount ever redeemed
    pub amount_redeemed: u64,
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
        tracing::debug!("Creating TrackerStateManager...");
        let avl_state = avl_tree::AvlTreeState::new();

        // Use a temporary directory for storage (in real implementation, this would be configurable)
        tracing::debug!("Opening note storage...");
        let storage_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/basis_server/data/notes");
        let storage = match persistence::NoteStorage::open(&storage_path) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully");
                storage
            }
            Err(e) => {
                tracing::error!("Failed to initialize note storage: {:?}", e);
                // Fallback to in-memory storage if file storage fails
                // In production, this should handle errors properly
                panic!("Failed to initialize note storage: {:?}", e);
            }
        };

        tracing::debug!("TrackerStateManager created successfully");
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
        value_bytes.extend_from_slice(&note.amount_collected.to_be_bytes());
        value_bytes.extend_from_slice(&note.amount_redeemed.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        // Try to insert, if key already exists then update
        if let Err(_e) = self.avl_state.insert(key.to_bytes(), value_bytes.clone()) {
            // For any error, try to update instead (assuming key already exists)
            self.avl_state
                .update(key.to_bytes(), value_bytes)
                .map_err(NoteError::StorageError)?;
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
            &note.amount_collected.to_be_bytes()[..],
            &note.amount_redeemed.to_be_bytes()[..],
            &note.timestamp.to_be_bytes()[..],
        ]
        .concat();

        self.avl_state
            .update(key.to_bytes(), value_bytes)
            .map_err(NoteError::StorageError)?;

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
            .map_err(NoteError::StorageError)?;

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
        amount_collected: u64,
        amount_redeemed: u64,
        timestamp: u64,
        signature: Signature,
    ) -> Self {
        Self {
            recipient_pubkey,
            amount_collected,
            amount_redeemed,
            timestamp,
            signature,
        }
    }

    /// Get the current outstanding debt (collected - redeemed)
    pub fn outstanding_debt(&self) -> u64 {
        self.amount_collected.saturating_sub(self.amount_redeemed)
    }

    /// Check if the note is fully redeemed
    pub fn is_fully_redeemed(&self) -> bool {
        self.amount_collected == self.amount_redeemed
    }

    /// Create and sign a new IOU note using the chaincash-rs Schnorr signature approach
    pub fn create_and_sign(
        recipient_pubkey: PubKey,
        amount_collected: u64,
        timestamp: u64,
        issuer_secret_key: &[u8; 32],
    ) -> Result<Self, NoteError> {
        use secp256k1::{Secp256k1, SecretKey};

        let secp = Secp256k1::new();

        // Parse the secret key
        let secret_key =
            SecretKey::from_slice(issuer_secret_key).map_err(|_| NoteError::InvalidSignature)?;

        // Generate the corresponding public key
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = public_key.serialize();

        // Generate the signing message (same format as chaincash-rs)
        let message = schnorr::signing_message(&recipient_pubkey, amount_collected, timestamp);

        // Use the chaincash-rs approach for Schnorr signing
        let signature = schnorr::schnorr_sign(&message, &secret_key, &issuer_pubkey)?;

        Ok(Self {
            recipient_pubkey,
            amount_collected,
            amount_redeemed: 0, // Start with no redemptions
            timestamp,
            signature,
        })
    }

    /// Generate the message that should be signed
    pub fn signing_message(&self) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&self.recipient_pubkey);
        message.extend_from_slice(&self.amount_collected.to_be_bytes());
        message.extend_from_slice(&self.timestamp.to_be_bytes());
        message
    }

    /// Verify the signature against an issuer public key using Schnorr signature verification
    /// This follows the chaincash-rs approach for Schnorr signature verification
    pub fn verify_signature(&self, issuer_pubkey: &PubKey) -> Result<(), NoteError> {
        // Validate the issuer public key first
        schnorr::validate_public_key(issuer_pubkey)?;

        // Validate the signature format
        schnorr::validate_signature_format(&self.signature)?;

        // Generate the signing message
        let message = self.signing_message();

        // Use the chaincash-rs approach for Schnorr verification
        schnorr::schnorr_verify(&self.signature, &message, issuer_pubkey)
    }
}

impl NoteKey {
    /// Create a note key from issuer and recipient public keys
    pub fn from_keys(issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Self {
        let issuer_hash = blake2b256_hash(issuer_pubkey);
        let recipient_hash = blake2b256_hash(recipient_pubkey);

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

/// Blake2b256 hash function for cryptographic hashing
pub fn blake2b256_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b512::new();
    hasher.update(data);
    let result = hasher.finalize();
    result[..32]
        .try_into()
        .expect("Blake2b512 should produce at least 32 bytes")
}

// Re-export reserve tracker types
pub use reserve_tracker::{ExtendedReserveInfo, ReserveTracker, ReserveTrackerError};

// Re-export ergo scanner types
pub use ergo_scanner::{
    create_default_scanner, start_scanner, ErgoBox, NodeConfig, ReserveEvent, ScanType,
    ScannerError, ServerState,
};

// Re-export redemption types
pub use redemption::{RedemptionData, RedemptionError, RedemptionManager, RedemptionRequest};
