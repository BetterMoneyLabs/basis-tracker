//! Core data structures for Basis tracker

pub mod avl_tree;

pub mod contract_compiler;
pub mod cross_verification;
pub mod ergo_scanner;
pub mod persistence;
pub mod redemption;
pub mod tracker_scanner;
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
pub mod tracker_scanner_test;
#[cfg(test)]
pub mod property_tests;
#[cfg(test)]
pub mod real_scanner_integration_tests;
#[cfg(test)]
pub mod reserve_tracking_test;
#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]


use secp256k1;
use basis_core;
use basis_core::impls::SchnorrVerifier;
use basis_core::traits::SignatureVerifier;

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
    /// Tracker NFT ID from R6 register (hex-encoded serialized SColl(SByte) format following byte_array_register_serialization.md spec)
    pub tracker_nft_id: String,
}

/// Tracker box information for state commitment boxes
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrackerBoxInfo {
    /// Box ID (hex encoded)
    pub box_id: String,
    /// Tracker public key (hex encoded, from R4)
    pub tracker_pubkey: String,
    /// State commitment hash (hex encoded, from R5)
    pub state_commitment: String,
    /// Last verified height (from R6)
    pub last_verified_height: u64,
    /// Box value in nanoERG
    pub value: u64,
    /// Creation height
    pub creation_height: u64,
    /// Tracker NFT ID (hex encoded)
    pub tracker_nft_id: String,
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
    PastTimestamp,
    RedemptionTooEarly,
    InsufficientCollateral,
    StorageError(String),
    UnsupportedOperation,
}

impl From<secp256k1::Error> for NoteError {
    fn from(_: secp256k1::Error) -> Self {
        NoteError::InvalidSignature
    }
}

/// Tracker state manager with persistent AVL tree
pub struct TrackerStateManager {
    avl_state: basis_trees::BasisAvlTree,
    current_state: TrackerState,
    storage: persistence::NoteStorage,
}

impl TrackerStateManager {
    /// Create a new tracker state manager with default storage location
    pub fn new() -> Self {
        tracing::debug!("Creating TrackerStateManager...");

        // Use the standard storage location for production
        tracing::debug!("Opening note storage...");
        let storage_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/basis_server/data/notes");
        let storage = match persistence::NoteStorage::open(&storage_path) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully at: {:?}", storage_path);
                storage
            }
            Err(e) => {
                tracing::error!("Failed to initialize note storage: {:?}", e);
                // Fallback to in-memory storage if file storage fails
                // In production, this should handle errors properly
                panic!("Failed to initialize note storage: {:?}", e);
            }
        };

        // Create in-memory AVL tree
        let avl_state = match basis_trees::BasisAvlTree::new() {
            Ok(tree) => {
                tracing::debug!("In-memory AVL tree created successfully");
                tree
            }
            Err(e) => {
                tracing::error!("Failed to initialize AVL tree: {:?}", e);
                panic!("Failed to initialize AVL tree: {:?}", e);
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

    /// Create a new tracker state manager with temporary storage (used in tests only)
    pub fn new_with_temp_storage() -> Self {
        tracing::debug!("Creating TrackerStateManager (test version with temporary storage)...");

        // Use a temporary directory for storage to avoid test conflicts
        tracing::debug!("Opening note storage...");
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let storage_path = std::env::temp_dir().join(format!("basis_test_{}_{}_{}", unique_id, std::process::id(), rand::random::<u64>()));

        // Try to clean up any existing storage at this path first
        let _ = std::fs::remove_dir_all(&storage_path);

        let storage = match persistence::NoteStorage::open(&storage_path) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully at: {:?}", storage_path);
                storage
            }
            Err(e) => {
                tracing::error!("Failed to initialize note storage: {:?}. Retrying with new path...", e);

                // Create a new unique path if the first one failed
                let unique_id_retry = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
                    + rand::random::<u128>();
                let storage_path_retry = std::env::temp_dir().join(format!("basis_test_retry_{}_{}_{}", unique_id_retry, std::process::id(), rand::random::<u64>()));

                // Try to clean up the retry path as well
                let _ = std::fs::remove_dir_all(&storage_path_retry);

                match persistence::NoteStorage::open(&storage_path_retry) {
                    Ok(storage) => {
                        tracing::debug!("Note storage opened successfully at retry path: {:?}", storage_path_retry);
                        storage
                    }
                    Err(e2) => {
                        tracing::error!("Failed to initialize note storage on retry: {:?}", e2);
                        // Fallback to in-memory storage if file storage fails
                        // In production, this should handle errors properly
                        panic!("Failed to initialize note storage: {:?}", e);
                    }
                }
            }
        };

        // Create in-memory AVL tree
        let avl_state = match basis_trees::BasisAvlTree::new() {
            Ok(tree) => {
                tracing::debug!("In-memory AVL tree created successfully");
                tree
            }
            Err(e) => {
                tracing::error!("Failed to initialize AVL tree: {:?}", e);
                panic!("Failed to initialize AVL tree: {:?}", e);
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
        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_secs();

        if note.timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }

        // Check if there is an existing note with the same issuer-recipient pair
        // and ensure the new timestamp is greater than the existing one (ever increasing)
        if let Ok(existing_note) = self.lookup_note(issuer_pubkey, &note.recipient_pubkey) {
            if note.timestamp <= existing_note.timestamp {
                return Err(NoteError::PastTimestamp);
            }
        }

        // Verify the note signature before storing it
        note.verify_signature(issuer_pubkey)
            .map_err(|e| {
                tracing::error!("Invalid note signature when adding note: {:?}", e);
                NoteError::InvalidSignature
            })?;

        // Prepare AVL tree key and value in advance
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Create value bytes matching persistence format
        let mut value_bytes = Vec::new();
        value_bytes.extend_from_slice(issuer_pubkey);
        value_bytes.extend_from_slice(&note.amount_collected.to_be_bytes());
        value_bytes.extend_from_slice(&note.amount_redeemed.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        // Update AVL tree state first to ensure consistency
        // Use update operation since in Basis tracker, only one note per issuer-recipient pair exists
        // and new operations replace existing ones
        let avl_result = self.avl_state.update(key_bytes.clone(), value_bytes.clone());

        // Only proceed with database storage if AVL tree update succeeded
        match avl_result {
            Ok(()) => {
                // Now store note in persistent storage
                self.storage.store_note(issuer_pubkey, note)?;
                self.update_state();
                Ok(())
            }
            Err(e) => Err(NoteError::StorageError(e.to_string())),
        }
    }

    /// Update an existing note
    pub fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_secs();

        if note.timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }

        // Check if there is an existing note with the same issuer-recipient pair
        // and ensure the new timestamp is greater than the existing one (ever increasing)
        if let Ok(existing_note) = self.lookup_note(issuer_pubkey, &note.recipient_pubkey) {
            if note.timestamp <= existing_note.timestamp {
                return Err(NoteError::PastTimestamp);
            }
        }

        // Prepare key and value in advance
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Create value bytes matching persistence format (consistent with add_note)
        let mut value_bytes = Vec::new();
        value_bytes.extend_from_slice(issuer_pubkey);
        value_bytes.extend_from_slice(&note.amount_collected.to_be_bytes());
        value_bytes.extend_from_slice(&note.amount_redeemed.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        // Update AVL tree state first to ensure consistency
        let avl_result = self.avl_state.update(key_bytes.clone(), value_bytes);

        // Only proceed with database storage if AVL tree update succeeded
        match avl_result {
            Ok(()) => {
                // Now store note in persistent storage
                self.storage.store_note(issuer_pubkey, note)?;
                self.update_state();
                Ok(())
            }
            Err(e) => Err(NoteError::StorageError(e.to_string())),
        }
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

    /// Get all notes in the tracker with issuer information
    pub fn get_all_notes_with_issuer(&self) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.storage.get_all_notes_with_issuer()
    }

    /// Update the current state with latest AVL tree root
    fn update_state(&mut self) {
        self.current_state.avl_root_digest = self.avl_state.root_digest();
        // Update timestamp would be set to current time in real implementation
        self.current_state.last_update_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState {
        &self.current_state
    }
}

impl TrackerStateManager {
    /// Find the reserve box ID for an issuer using key matching
    pub fn find_reserve_box_id_for_issuer(&self, issuer_pubkey_hex: &str, reserve_tracker: &ReserveTracker) -> Result<String, NoteError> {
        // Get all reserves from the reserve tracker
        let all_reserves = reserve_tracker.get_all_reserves();

        // Since we now strip the 0x07 prefix when reading from registers,
        // we can do a direct match (with normalization for any remaining edge cases)
        for reserve in all_reserves {
            if issuer_pubkey_hex == reserve.owner_pubkey ||
               normalize_public_key(issuer_pubkey_hex) == normalize_public_key(&reserve.owner_pubkey) {
                return Ok(reserve.box_id);
            }
        }

        // If no matching reserve is found, return an error
        Err(NoteError::StorageError(format!("No reserve found for issuer: {}", issuer_pubkey_hex)))
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
        let signature = schnorr::schnorr_sign(&message, &secret_key.secret_bytes(), &issuer_pubkey)?;

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
        // Generate the signing message
        let message = self.signing_message();

        // Use the canonical Schnorr verification from basis_core
        let verifier = SchnorrVerifier;
        match verifier.verify_signature(&self.signature, &message, issuer_pubkey) {
            Ok(()) => Ok(()),
            Err(basis_core::traits::CryptoError::InvalidSignature) => Err(NoteError::InvalidSignature),
            Err(basis_core::traits::CryptoError::InvalidPublicKey) => Err(NoteError::InvalidSignature),
            Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => Err(NoteError::InvalidSignature),
            Err(basis_core::traits::CryptoError::InternalError(_)) => Err(NoteError::InvalidSignature),
        }
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
    use blake2::{Blake2b, Digest};
    use generic_array::typenum::U32;
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    let result = hasher.finalize();
    result[..32]
        .try_into()
        .expect("Blake2b should produce at least 32 bytes")
}

/// Normalize public key representations to handle different Ergo register formats.
/// This function exists for backward compatibility and handles any remaining edge cases
/// where public keys may still have prefixes that weren't stripped at source.
pub fn normalize_public_key(pubkey_hex: &str) -> String {
    // Since we now strip the 0x07 prefix when reading from registers,
    // this function mainly exists for backward compatibility
    // and handles any remaining edge cases
    let pubkey_bytes = match hex::decode(pubkey_hex) {
        Ok(bytes) => bytes,
        Err(_) => return pubkey_hex.to_string(), // Return original if invalid hex
    };

    if pubkey_bytes.len() < 1 {
        return pubkey_hex.to_string();
    }

    // If it starts with 07 (GroupElement), it's likely a prefixed version
    // where the actual public key starts from the 2nd byte
    // This handles any remaining cases where prefix wasn't stripped at source
    if pubkey_bytes[0] == 0x07 && pubkey_bytes.len() >= 34 {
        // Extract the actual public key (33 bytes after the 0x07 prefix)
        let actual_pubkey = &pubkey_bytes[1..34]; // 33 bytes after the prefix
        hex::encode(actual_pubkey)
    } else {
        // For standard formats, return as is
        pubkey_hex.to_string()
    }
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

// Re-export reqwest for use in dependent crates
pub use reqwest;
