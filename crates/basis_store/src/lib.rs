//! Core data structures for Basis tracker

pub mod avl_tree;
pub mod basis_v2_state;

pub mod contract_compiler;
#[cfg(test)]
pub mod cross_validation_tests;
pub mod cross_verification;
pub mod ergo_scanner;
pub mod persistence;
pub mod redemption;
#[cfg(test)]
pub mod redemption_blockchain_tests;
#[cfg(test)]
pub mod redemption_simple_tests;
pub mod reserve_tracker;
pub mod scala_test_vectors;
pub mod schnorr;
pub mod schnorr_test_vectors;
pub mod schnorr_tests;
#[cfg(test)]
pub mod simple_integration_tests;
pub mod tests;
pub mod tracker_scanner;
pub mod transaction_builder;

// Test modules
#[cfg(test)]
pub mod basis_spec_tests;
#[cfg(test)]
pub mod cross_verification_tests;
#[cfg(test)]
pub mod property_tests;
#[cfg(test)]
pub mod real_scanner_integration_tests;
#[cfg(test)]
pub mod reserve_tracking_test;
#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod tracker_scanner_test;

use basis_core;
use basis_core::impls::SchnorrVerifier;
use basis_core::traits::SignatureVerifier;
use secp256k1;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

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

/// Confirmation status of a note relative to the on-chain tracker box commitment.
///
/// A note is only redeemable when its `totalDebt` is committed in the confirmed
/// on-chain tracker box R5 (i.e. `Confirmed`). Notes that are only in the local
/// tracker tree (`LocalOnly`) or in a submitted-but-unconfirmed update transaction
/// (`Pending`) cannot be redeemed yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteConfirmationStatus {
    /// Present only in the local tracker tree, not yet submitted on-chain.
    LocalOnly,
    /// Included in a tracker box update transaction that has been submitted but
    /// not yet confirmed on-chain.
    Pending,
    /// Committed in the latest confirmed on-chain tracker box R5.
    Confirmed,
}

/// Cached confirmation record for a single note, keyed by
/// `blake2b256(issuer_pubkey || recipient_pubkey)`.
///
/// The record tracks the note's value at each commitment level so that clients
/// can determine exactly how much is redeemable right now (the confirmed value)
/// versus how much is only known locally or is still in flight.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoteConfirmation {
    /// Current confirmation status of the note.
    pub status: NoteConfirmationStatus,
    /// The `totalDebt` value committed in the latest confirmed on-chain tracker
    /// box R5. `None` if the note has never been confirmed on-chain. This is the
    /// maximum amount the contract will accept for redemption.
    pub confirmed_total_debt: Option<u64>,
    /// The `totalDebt` value included in the currently in-flight tracker box
    /// update transaction. `None` if the note is not part of a pending update.
    pub pending_total_debt: Option<u64>,
    /// Box ID of the confirmed tracker box that committed `confirmed_total_debt`.
    pub confirmed_box_id: Option<String>,
    /// Height at which the confirmed tracker box was observed.
    pub confirmed_height: Option<u64>,
    /// Transaction ID of the in-flight tracker box update that covers this note.
    pub pending_tx_id: Option<String>,
}

impl NoteConfirmation {
    /// Create a fresh record for a note that exists only in the local tree.
    pub fn local_only() -> Self {
        Self {
            status: NoteConfirmationStatus::LocalOnly,
            confirmed_total_debt: None,
            pending_total_debt: None,
            confirmed_box_id: None,
            confirmed_height: None,
            pending_tx_id: None,
        }
    }

    /// Returns true when the note has a confirmed value that exceeds the
    /// `already_redeemed` amount, i.e. there is something left to redeem.
    pub fn is_redeemable(&self, already_redeemed: u64) -> bool {
        self.confirmed_total_debt
            .map(|debt| debt > already_redeemed)
            .unwrap_or(false)
    }

    /// Returns the amount that can be redeemed right now:
    /// `max(0, confirmed_total_debt - already_redeemed)`.
    pub fn redeemable_amount(&self, already_redeemed: u64) -> u64 {
        self.confirmed_total_debt
            .map(|debt| debt.saturating_sub(already_redeemed))
            .unwrap_or(0)
    }
}

impl Default for NoteConfirmation {
    fn default() -> Self {
        Self::local_only()
    }
}

/// Note key (32 bytes) used to index confirmation records.
pub type NoteKeyBytes = [u8; 32];

/// Durable identity of one tracker-root publication that may have crossed the
/// node admission boundary but is not yet confirmed on the active chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTrackerPublication {
    digest: [u8; 33],
    tx_id: String,
    submitted_height: u64,
}

impl PendingTrackerPublication {
    pub fn digest(&self) -> [u8; 33] {
        self.digest
    }

    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    pub fn submitted_height(&self) -> u64 {
        self.submitted_height
    }
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
    /// Refund initiation height from R7 register (0 if no refund pending)
    #[serde(default)]
    pub refund_initiation_height: u64,
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

/// Tracker lookup proof for context var #8 in redemption transactions
/// Proves that totalDebt exists in the tracker's AVL tree at key hash(ownerKey||receiverKey)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerLookupProof {
    /// The AVL tree key: blake2b256(ownerKey || receiverKey) (32 bytes)
    pub key: Vec<u8>,
    /// The value: totalDebt as 8-byte big-endian
    pub value: Vec<u8>,
    /// AVL proof bytes for the lookup
    pub proof: Vec<u8>,
}

/// Reserve lookup proof for context var #7 in redemption transactions
/// Proves that already_redeemed exists in the reserve's AVL tree at key hash(ownerKey||receiverKey)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveLookupProof {
    /// The AVL tree key: blake2b256(ownerKey || receiverKey) (32 bytes)
    pub key: Vec<u8>,
    /// The value: already_redeemed (8 bytes BE)
    pub value: Vec<u8>,
    /// AVL proof bytes for the lookup (None for first redemption)
    pub proof: Option<Vec<u8>>,
}

/// Key for note lookup: blake2b256(issuer_pubkey || recipient_pubkey)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteKey {
    /// blake2b256(issuer_pubkey || recipient_pubkey)
    pub key_hash: [u8; 32],
}

impl NoteKey {
    /// Create a note key from issuer and recipient public keys
    pub fn from_keys(issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Self {
        let mut data = Vec::with_capacity(66);
        data.extend_from_slice(issuer_pubkey);
        data.extend_from_slice(recipient_pubkey);
        let key_hash = blake2b256_hash(&data);

        Self { key_hash }
    }

    /// Convert note key to bytes for AVL tree
    pub fn to_bytes(&self) -> Vec<u8> {
        self.key_hash.to_vec()
    }

    /// Create a note key from bytes (32-byte hash)
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self { key_hash: *bytes }
    }
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
    DebtRegression,
    RedemptionTooEarly,
    InsufficientCollateral,
    /// Existing note data uses a persistence schema that this binary will not
    /// rewrite implicitly. An explicit export/migration or approved reset is required.
    MigrationRequired(String),
    /// The configured tracker NFT does not match the generation bound to this
    /// data directory, or the first observed on-chain root does not match the
    /// explicitly approved fresh-generation root.
    GenerationMismatch(String),
    /// A new data directory cannot be initialized until the operator explicitly
    /// approves creation of a fresh tracker generation.
    GenerationBindingRequired(String),
    /// The bounded live-note set is full. No state was mutated and the manager
    /// remains healthy.
    CapacityExceeded {
        limit: usize,
    },
    StorageError(String),
    /// The storage engine reported a durability failure after beginning a WAL
    /// commit. The operation may become visible after restart, so the current
    /// manager must be quarantined rather than treating this as a rollback.
    StorageOutcomeUnknown(String),
    /// The sole tracker actor is fenced across an external commitment effect.
    PublicationInProgress,
    /// A publication completion/abort did not present the actor's active lease.
    PublicationLeaseMismatch,
    /// A node transaction identity is not exactly 32 bytes of hexadecimal.
    InvalidTransactionId,
    UnsupportedOperation,
}

/// Explicit startup policy for a tracker generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshGenerationApproval {
    /// Open only an already-bound generation.
    Deny,
    /// Permit creation of a new, empty generation manifest for this NFT.
    Approve,
}

/// Persistent tracker-generation identity supplied by the server at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackerGenerationConfig {
    pub tracker_nft_id: [u8; 32],
    pub fresh_generation: FreshGenerationApproval,
}

/// One-way health signal shared with every component capable of publishing a
/// tracker root. Once quarantined it cannot be reset in-process.
#[derive(Debug, Clone)]
pub struct PublicationHealth(Arc<AtomicBool>);

impl PublicationHealth {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    pub fn is_healthy(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn quarantine(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Default for PublicationHealth {
    fn default() -> Self {
        Self::new()
    }
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
    /// Reserve AVL tree tracking hash(ownerKey || receiverKey) -> already_redeemed (8 bytes BE)
    reserve_avl_state: basis_trees::BasisAvlTree,
    /// Per-note confirmation records, keyed by note key (32 bytes).
    confirmations: std::collections::HashMap<NoteKeyBytes, NoteConfirmation>,
    poisoned: AtomicBool,
    publication_health: PublicationHealth,
}

impl TrackerStateManager {
    fn poison(&self) {
        self.poisoned.store(true, Ordering::SeqCst);
        self.publication_health.quarantine();
    }

    fn ensure_healthy(&self) -> Result<(), NoteError> {
        if self.poisoned.load(Ordering::SeqCst) {
            Err(NoteError::StorageOutcomeUnknown(
                "Tracker state manager is quarantined after an indeterminate durable write; restart and reconcile before reuse"
                    .to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn quarantine_on_storage_failure<T>(
        &self,
        result: Result<T, NoteError>,
    ) -> Result<T, NoteError> {
        if matches!(
            result,
            Err(NoteError::InvalidSignature)
                | Err(NoteError::StorageError(_))
                | Err(NoteError::StorageOutcomeUnknown(_))
                | Err(NoteError::MigrationRequired(_))
                | Err(NoteError::GenerationMismatch(_))
                | Err(NoteError::GenerationBindingRequired(_))
        ) {
            self.poison();
        }
        result
    }

    /// Returns whether this manager and every publisher sharing its one-way
    /// health signal may still expose a tracker commitment.
    pub fn is_healthy(&self) -> bool {
        !self.poisoned.load(Ordering::SeqCst) && self.publication_health.is_healthy()
    }

    /// Validate the configured NFT and first observed on-chain root against the
    /// persisted generation manifest. A mismatch permanently quarantines this
    /// process so a wrong data directory can never publish over that NFT.
    pub fn validate_observed_generation(
        &self,
        tracker_nft_id: &[u8; 32],
        observed_root: [u8; 33],
    ) -> Result<(), NoteError> {
        self.ensure_healthy()?;
        // Publication is itself a state transition. Revalidate the complete
        // durable snapshot against the live tree before authorizing the updater
        // to spend the tracker box, even when no new note was admitted in this
        // process cycle.
        self.validate_complete_snapshot_against_live()?;
        self.quarantine_on_storage_failure(
            self.storage
                .validate_or_anchor_generation(tracker_nft_id, observed_root),
        )
    }

    /// Create a new tracker state manager with the configured storage location.
    pub fn new(data_dir: impl AsRef<Path>, generation: TrackerGenerationConfig) -> Self {
        Self::try_new(data_dir, generation)
            .unwrap_or_else(|e| panic!("Failed to initialize tracker state manager: {:?}", e))
    }

    /// Try to create the sole writer for a tracker state directory.
    ///
    /// A second in-process or cross-process writer is rejected by the storage
    /// lock, and any legacy/malformed persistence state is returned as a typed
    /// error instead of being silently reordered or repaired.
    pub fn try_new(
        data_dir: impl AsRef<Path>,
        generation: TrackerGenerationConfig,
    ) -> Result<Self, NoteError> {
        Self::try_new_with_publication_health(data_dir, generation, PublicationHealth::new())
    }

    /// Open a generation while sharing its terminal health state with the
    /// component that can publish tracker commitments.
    pub fn try_new_with_publication_health(
        data_dir: impl AsRef<Path>,
        generation: TrackerGenerationConfig,
        publication_health: PublicationHealth,
    ) -> Result<Self, NoteError> {
        tracing::debug!("Creating TrackerStateManager...");

        tracing::debug!("Opening note storage...");
        let storage_path = data_dir.as_ref().join("notes");
        let storage = persistence::NoteStorage::open(&storage_path, generation)?;
        tracing::debug!("Note storage opened successfully at: {:?}", storage_path);

        let avl_state = basis_trees::BasisAvlTree::new().map_err(|e| {
            NoteError::StorageError(format!("Failed to initialize AVL tree: {:?}", e))
        })?;

        let reserve_avl_state = basis_trees::BasisAvlTree::new().map_err(|e| {
            NoteError::StorageError(format!("Failed to initialize reserve AVL tree: {:?}", e))
        })?;

        // Rebuild AVL tree from all stored notes to ensure consistency after restart
        let mut manager = Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
            reserve_avl_state,
            confirmations: std::collections::HashMap::new(),
            poisoned: AtomicBool::new(false),
            publication_health,
        };

        manager.rebuild_avl_tree()?;

        // Rebuild confirmation records from storage and mark every stored note as
        // LocalOnly until the updater confirms otherwise.
        manager.rebuild_confirmations()?;

        tracing::debug!("TrackerStateManager created successfully");
        Ok(manager)
    }

    /// Rebuild the AVL tree from the authoritative first-insertion order.
    ///
    /// AVL tree roots are insertion-order sensitive. A final note snapshot or
    /// business timestamp cannot reproduce the original key insertion order, so
    /// a non-empty legacy store without that order is rejected rather than
    /// synthesizing a potentially different root. Repeated value updates do not
    /// append history: only the final snapshot and each key's first position are
    /// required to rebuild the same bounded tree state.
    pub fn rebuild_avl_tree(&mut self) -> Result<(), NoteError> {
        self.ensure_healthy()?;
        tracing::info!("Rebuilding AVL tree from persistent note insertion order...");

        let rebuilt_tree = match self.build_validated_avl_tree() {
            Ok(tree) => tree,
            Err(error) => {
                self.poison();
                return Err(error);
            }
        };

        self.avl_state = rebuilt_tree;
        self.update_state();
        let root_digest = self.current_state.avl_root_digest;
        tracing::info!(
            "AVL tree rebuilt successfully with root digest: {}",
            hex::encode(&root_digest)
        );

        Ok(())
    }

    fn build_validated_avl_tree(&self) -> Result<basis_trees::BasisAvlTree, NoteError> {
        // Build in isolation. The live tree and its published digest remain
        // untouched if any storage, signature, ordering, or AVL validation fails.
        let mut rebuilt_tree = basis_trees::BasisAvlTree::new().map_err(|e| {
            NoteError::StorageError(format!("Failed to initialize rebuilt AVL tree: {:?}", e))
        })?;

        let persisted_state = self.storage.read_state_strict()?;
        let expected_root = persisted_state.avl_root_digest;
        let ordered_notes = persisted_state.notes;

        tracing::info!(
            "Replaying {} ordered live note keys...",
            ordered_notes.len()
        );

        for (issuer_pubkey, note) in ordered_notes {
            note.verify_signature(&issuer_pubkey)
                .map_err(|_| NoteError::InvalidSignature)?;
            if note.amount_redeemed > note.amount_collected {
                return Err(NoteError::StorageError(
                    "Stored redeemed amount exceeds cumulative debt".to_string(),
                ));
            }

            let key = NoteKey::from_keys(&issuer_pubkey, &note.recipient_pubkey);
            let key_bytes = key.to_bytes();
            let value_bytes = note.amount_collected.to_be_bytes().to_vec();

            rebuilt_tree
                .update(key_bytes.clone(), value_bytes)
                .map_err(|e| {
                    NoteError::StorageError(format!(
                        "AVL tree update failed during rebuild: {:?}",
                        e
                    ))
                })?;
        }

        if rebuilt_tree.root_digest() != expected_root {
            return Err(NoteError::StorageError(
                "Persisted note snapshot does not reproduce its committed AVL root".to_string(),
            ));
        }

        Ok(rebuilt_tree)
    }

    fn validate_complete_snapshot_against_live(
        &self,
    ) -> Result<persistence::StoredNoteState, NoteError> {
        self.ensure_healthy()?;
        let state = match self.storage.read_state_strict() {
            Ok(state) => state,
            Err(error) => {
                self.poison();
                return Err(error);
            }
        };
        let mut rebuilt = basis_trees::BasisAvlTree::new().map_err(|error| {
            self.poison();
            NoteError::StorageError(format!("Failed to initialize validation tree: {error}"))
        })?;

        for (issuer_pubkey, note) in &state.notes {
            if note.verify_signature(issuer_pubkey).is_err() {
                self.poison();
                return Err(NoteError::InvalidSignature);
            }
            if note.amount_redeemed > note.amount_collected {
                self.poison();
                return Err(NoteError::StorageError(
                    "Stored redeemed amount exceeds cumulative debt".to_string(),
                ));
            }
            let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey).to_bytes();
            if let Err(error) = rebuilt.update(key, note.amount_collected.to_be_bytes().to_vec()) {
                self.poison();
                return Err(NoteError::StorageError(format!(
                    "AVL validation update failed: {error}"
                )));
            }
        }

        let rebuilt_root = rebuilt.root_digest();
        if rebuilt_root != state.avl_root_digest
            || rebuilt_root != self.avl_state.root_digest()
            || rebuilt_root != self.current_state.avl_root_digest
        {
            self.poison();
            return Err(NoteError::StorageError(
                "Persisted snapshot, physical AVL keys, and live root do not agree".to_string(),
            ));
        }
        Ok(state)
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
        let storage_path = std::env::temp_dir().join(format!(
            "basis_test_{}_{}_{}",
            unique_id,
            std::process::id(),
            rand::random::<u64>()
        ));

        // Try to clean up any existing storage at this path first
        let _ = std::fs::remove_dir_all(&storage_path);

        let generation = TrackerGenerationConfig {
            tracker_nft_id: [0x55; 32],
            fresh_generation: FreshGenerationApproval::Approve,
        };
        let storage = match persistence::NoteStorage::open(&storage_path, generation) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully at: {:?}", storage_path);
                storage
            }
            Err(e) => {
                tracing::error!(
                    "Failed to initialize note storage: {:?}. Retrying with new path...",
                    e
                );

                // Create a new unique path if the first one failed
                let unique_id_retry = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
                    + rand::random::<u128>();
                let storage_path_retry = std::env::temp_dir().join(format!(
                    "basis_test_retry_{}_{}_{}",
                    unique_id_retry,
                    std::process::id(),
                    rand::random::<u64>()
                ));

                // Try to clean up the retry path as well
                let _ = std::fs::remove_dir_all(&storage_path_retry);

                match persistence::NoteStorage::open(&storage_path_retry, generation) {
                    Ok(storage) => {
                        tracing::debug!(
                            "Note storage opened successfully at retry path: {:?}",
                            storage_path_retry
                        );
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

        // Create reserve AVL tree for tracking already_redeemed
        let reserve_avl_state = match basis_trees::BasisAvlTree::new() {
            Ok(tree) => {
                tracing::debug!("Reserve AVL tree created successfully");
                tree
            }
            Err(e) => {
                tracing::error!("Failed to initialize reserve AVL tree: {:?}", e);
                panic!("Failed to initialize reserve AVL tree: {:?}", e);
            }
        };

        tracing::debug!("TrackerStateManager created successfully");
        let mut manager = Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
            reserve_avl_state,
            confirmations: std::collections::HashMap::new(),
            poisoned: AtomicBool::new(false),
            publication_health: PublicationHealth::new(),
        };

        // Rebuild AVL tree and confirmations so test instances mirror production.
        if let Err(e) = manager.rebuild_avl_tree() {
            panic!(
                "Failed to rebuild AVL tree in test instance from authoritative snapshot: {:?}",
                e
            );
        }
        if let Err(e) = manager.rebuild_confirmations() {
            panic!("Failed to rebuild confirmations in test instance: {:?}", e);
        }

        manager
    }

    /// Add a new note to the tracker state
    /// Updates the AVL tree with hash(issuer||receiver) -> totalDebt mapping
    pub fn add_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        self.ensure_healthy()?;
        let persisted_state = self.validate_complete_snapshot_against_live()?;

        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;

        if note.timestamp > current_time {
            return Err(NoteError::FutureTimestamp);
        }

        // A note is a cumulative debt record. Both its timestamp and totalDebt
        // must be monotone for a given issuer-recipient edge.
        let target_key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey).to_bytes();
        let existing_note =
            persisted_state
                .notes
                .iter()
                .find_map(|(stored_issuer, stored_note)| {
                    (NoteKey::from_keys(stored_issuer, &stored_note.recipient_pubkey).to_bytes()
                        == target_key)
                        .then_some(stored_note.clone())
                });
        if let Some(existing_note) = &existing_note {
            if note.timestamp <= existing_note.timestamp {
                return Err(NoteError::PastTimestamp);
            }
            if note.amount_collected < existing_note.amount_collected {
                return Err(NoteError::DebtRegression);
            }
        }

        // Verify the note signature before storing it
        note.verify_signature(issuer_pubkey).map_err(|e| {
            tracing::error!("Invalid note signature when adding note: {:?}", e);
            NoteError::InvalidSignature
        })?;

        // Capacity is an expected admission failure, not a durability or
        // structural failure. Preflight it before cloning or mutating a tree.
        self.storage.ensure_capacity_for_validated_state(
            persisted_state.notes.len(),
            existing_note.is_none(),
        )?;

        // Settlement progress is tracker-derived local state and is not part of
        // the issuer-signed cumulative-debt message. A newer signed successor
        // must therefore preserve, rather than reset, existing redemptions.
        let mut stored_note = note.clone();
        stored_note.amount_redeemed = existing_note
            .map(|existing| existing.amount_redeemed)
            .unwrap_or(0);

        // Prepare AVL tree key: hash(issuer_pubkey || receiver_pubkey)
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Value is just the totalDebt (amount_collected) as 8-byte big-endian
        // This matches the contract spec: hash(A||B) -> totalDebt
        let value_bytes = note.amount_collected.to_be_bytes().to_vec();

        // Prepare a fully isolated tree candidate. Storage failure leaves the
        // published in-memory root untouched; successful durable storage is
        // followed only by an infallible ownership swap.
        let mut candidate = match self.avl_state.try_clone() {
            Ok(candidate) => candidate,
            Err(error) => {
                self.poison();
                return Err(NoteError::StorageError(error.to_string()));
            }
        };
        if let Err(error) = candidate.update(key_bytes.clone(), value_bytes) {
            self.poison();
            return Err(NoteError::StorageError(error.to_string()));
        }

        let storage_result =
            self.storage
                .store_note(issuer_pubkey, &stored_note, candidate.root_digest());
        self.quarantine_on_storage_failure(storage_result)?;

        self.avl_state = candidate;
        self.update_state();

        // Recompute the confirmation status for this note based on the
        // new local value versus the confirmed/pending values. The local
        // value has just changed, so the note is only Confirmed/Pending
        // if the new value matches what is already on-chain / in-flight.
        let mut key32 = [0u8; 32];
        key32.copy_from_slice(&key_bytes);
        self.recompute_confirmation_status(&key32, note.amount_collected)?;

        Ok(())
    }

    /// Convert an issuer/recipient pair into the fixed-size confirmation key.
    fn confirmation_key(issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> NoteKeyBytes {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let bytes = key.to_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    }

    /// Recompute and durably persist one note's confirmation status.
    fn recompute_confirmation_status(
        &mut self,
        key: &NoteKeyBytes,
        local_value: u64,
    ) -> Result<(), NoteError> {
        let mut updated = self
            .confirmations
            .get(key)
            .cloned()
            .unwrap_or_else(NoteConfirmation::local_only);

        updated.status = if Some(local_value) == updated.confirmed_total_debt {
            NoteConfirmationStatus::Confirmed
        } else if Some(local_value) == updated.pending_total_debt {
            NoteConfirmationStatus::Pending
        } else {
            NoteConfirmationStatus::LocalOnly
        };

        let storage_result = self.storage.store_confirmation(key, &updated);
        self.quarantine_on_storage_failure(storage_result)?;
        self.confirmations.insert(*key, updated);
        Ok(())
    }

    /// Rebuild the in-memory confirmation map from storage. Called on startup.
    ///
    /// A pending publication survives restart only when its checksummed durable
    /// receipt and every per-note pending tx id agree. Stale per-note metadata
    /// without that receipt is demoted to `LocalOnly`.
    pub fn rebuild_confirmations(&mut self) -> Result<(), NoteError> {
        self.ensure_healthy()?;
        let notes = self.validate_complete_snapshot_against_live()?.notes;
        let stored = self.quarantine_on_storage_failure(self.storage.get_all_confirmations())?;
        let pending_publication =
            self.quarantine_on_storage_failure(self.storage.pending_publication())?;
        let stored_map: std::collections::HashMap<NoteKeyBytes, NoteConfirmation> =
            stored.into_iter().collect();
        let mut rebuilt = std::collections::HashMap::with_capacity(notes.len());

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let mut record = stored_map.get(&key).cloned().unwrap_or_default();

            let local_value = note.amount_collected;
            let durable_pending = pending_publication.as_ref().is_some_and(|pending| {
                record.pending_total_debt == Some(local_value)
                    && record
                        .pending_tx_id
                        .as_deref()
                        .is_some_and(|tx_id| tx_id.eq_ignore_ascii_case(&pending.tx_id))
            });
            record.status = if durable_pending {
                NoteConfirmationStatus::Pending
            } else if Some(local_value) == record.confirmed_total_debt {
                NoteConfirmationStatus::Confirmed
            } else {
                record.pending_total_debt = None;
                record.pending_tx_id = None;
                NoteConfirmationStatus::LocalOnly
            };

            rebuilt.insert(key, record);
        }

        self.confirmations = rebuilt;

        tracing::info!(
            "Rebuilt confirmation records for {} notes",
            self.confirmations.len()
        );
        Ok(())
    }

    /// Get a clone of the confirmation record for a note, if one exists.
    pub fn get_confirmation(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Option<NoteConfirmation> {
        if let Err(e) = self.ensure_healthy() {
            panic!("Cannot read confirmation from quarantined tracker: {:?}", e);
        }
        let key = Self::confirmation_key(issuer_pubkey, recipient_pubkey);
        self.confirmations.get(&key).cloned()
    }

    /// Get a snapshot of all confirmation records keyed by note key.
    pub fn all_confirmations(&self) -> std::collections::HashMap<NoteKeyBytes, NoteConfirmation> {
        if let Err(e) = self.ensure_healthy() {
            panic!(
                "Cannot read confirmations from quarantined tracker: {:?}",
                e
            );
        }
        self.confirmations.clone()
    }

    /// Mark every note whose local value differs from its confirmed value as
    /// `Pending`, recording the value that the in-flight update transaction will
    /// commit. Returns the number of notes transitioned to `Pending`.
    pub fn mark_notes_pending(
        &mut self,
        digest: [u8; 33],
        tx_id: &str,
        submitted_height: u64,
    ) -> Result<usize, NoteError> {
        self.ensure_healthy()?;
        let snapshot = self.validate_complete_snapshot_against_live()?;
        if snapshot.avl_root_digest != digest {
            return Err(NoteError::PublicationLeaseMismatch);
        }
        let publication = PendingTrackerPublication {
            digest,
            tx_id: tx_id.to_ascii_lowercase(),
            submitted_height,
        };
        let storage_result = self.storage.store_pending_publication(&publication);
        self.quarantine_on_storage_failure(storage_result)?;
        let notes = snapshot.notes;
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let local_value = note.amount_collected;
            let mut updated = self
                .confirmations
                .get(&key)
                .cloned()
                .unwrap_or_else(NoteConfirmation::local_only);

            if Some(local_value) != updated.confirmed_total_debt {
                updated.pending_total_debt = Some(local_value);
                updated.pending_tx_id = Some(tx_id.to_string());
                updated.status = NoteConfirmationStatus::Pending;
                let storage_result = self.storage.store_confirmation(&key, &updated);
                self.quarantine_on_storage_failure(storage_result)?;
                self.confirmations.insert(key, updated);
                count += 1;
            }
        }

        tracing::info!(
            "Marked {} notes as pending for update tx {} (digest {})",
            count,
            tx_id,
            hex::encode(digest)
        );
        Ok(count)
    }

    /// Return the checksummed external-effect receipt, if a tracker publication
    /// is still awaiting active-chain reconciliation.
    pub fn pending_publication(&self) -> Result<Option<PendingTrackerPublication>, NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        self.quarantine_on_storage_failure(self.storage.pending_publication())
    }

    /// Promote every `Pending` note to `Confirmed`, copying the pending value to
    /// the confirmed value and recording the confirming box metadata. Returns the
    /// number of notes transitioned to `Confirmed`.
    #[cfg(test)]
    pub(crate) fn confirm_pending_notes(
        &mut self,
        box_id: &str,
        height: u64,
    ) -> Result<usize, NoteError> {
        let pending = self
            .pending_publication()?
            .ok_or(NoteError::PublicationLeaseMismatch)?;
        self.confirm_pending_publication(&pending.tx_id, box_id, height)
    }

    /// Confirm exactly the durable publication receipt observed on the active
    /// chain. A different transaction cannot release the publication fence.
    pub fn confirm_pending_publication(
        &mut self,
        tx_id: &str,
        box_id: &str,
        height: u64,
    ) -> Result<usize, NoteError> {
        self.ensure_healthy()?;
        let snapshot = self.validate_complete_snapshot_against_live()?;
        let pending = self
            .quarantine_on_storage_failure(self.storage.pending_publication())?
            .ok_or(NoteError::PublicationLeaseMismatch)?;
        if !pending.tx_id.eq_ignore_ascii_case(tx_id) {
            return Err(NoteError::PublicationLeaseMismatch);
        }
        if pending.digest != snapshot.avl_root_digest {
            return Err(NoteError::PublicationLeaseMismatch);
        }
        let mut count = 0usize;

        // The publication receipt is persisted before the per-note advisory
        // records. A crash can therefore leave any prefix of those records on
        // disk. The confirmed root authenticates the complete current snapshot,
        // so replay confirmation from that snapshot instead of trusting which
        // advisory rows happened to reach storage before the crash.
        for (issuer_pubkey, note) in &snapshot.notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let mut updated = self
                .confirmations
                .get(&key)
                .cloned()
                .unwrap_or_else(NoteConfirmation::local_only);
            let changed = updated.confirmed_total_debt != Some(note.amount_collected)
                || updated.status != NoteConfirmationStatus::Confirmed
                || updated.pending_total_debt.is_some()
                || updated.pending_tx_id.is_some()
                || updated.confirmed_box_id.as_deref() != Some(box_id)
                || updated.confirmed_height != Some(height);
            updated.confirmed_total_debt = Some(note.amount_collected);
            updated.pending_total_debt = None;
            updated.pending_tx_id = None;
            updated.confirmed_box_id = Some(box_id.to_string());
            updated.confirmed_height = Some(height);
            updated.status = NoteConfirmationStatus::Confirmed;
            let storage_result = self.storage.store_confirmation(&key, &updated);
            self.quarantine_on_storage_failure(storage_result)?;
            self.confirmations.insert(key, updated);
            count += usize::from(changed);
        }

        tracing::info!(
            "Confirmed {} notes in tracker box {} at height {}",
            count,
            box_id,
            height
        );
        let clear_result = self.storage.clear_pending_publication(tx_id);
        self.quarantine_on_storage_failure(clear_result)?;
        Ok(count)
    }

    /// Revert every `Pending` note back to its prior state (used when an update
    /// transaction is dropped or rejected). Clears pending metadata and recomputes
    /// the status from the local value versus the confirmed value. Returns the
    /// number of notes reverted.
    #[cfg(test)]
    pub(crate) fn revert_pending_notes(&mut self) -> Result<usize, NoteError> {
        self.ensure_healthy()?;
        let notes = self.validate_complete_snapshot_against_live()?.notes;
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let is_pending = self
                .confirmations
                .get(&key)
                .map(|c| c.status == NoteConfirmationStatus::Pending)
                .unwrap_or(false);

            if is_pending {
                if let Some(mut updated) = self.confirmations.get(&key).cloned() {
                    updated.pending_total_debt = None;
                    updated.pending_tx_id = None;
                    let local_value = note.amount_collected;
                    updated.status = if Some(local_value) == updated.confirmed_total_debt {
                        NoteConfirmationStatus::Confirmed
                    } else {
                        NoteConfirmationStatus::LocalOnly
                    };
                    let storage_result = self.storage.store_confirmation(&key, &updated);
                    self.quarantine_on_storage_failure(storage_result)?;
                    self.confirmations.insert(key, updated);
                    count += 1;
                }
            }
        }

        tracing::info!("Reverted {} pending notes to local state", count);
        if let Some(pending) =
            self.quarantine_on_storage_failure(self.storage.pending_publication())?
        {
            let clear_result = self.storage.clear_pending_publication(&pending.tx_id);
            self.quarantine_on_storage_failure(clear_result)?;
        }
        Ok(count)
    }

    /// Reconcile confirmation records with an observed on-chain digest. When the
    /// confirmed digest equals the current local digest, every note's local value
    /// is the confirmed value, so mark them all as `Confirmed` with the given box
    /// metadata. Returns the number of notes promoted to `Confirmed`.
    pub fn reconcile_with_confirmed_digest(
        &mut self,
        confirmed_digest: &[u8; 33],
        box_id: &str,
        height: u64,
    ) -> Result<usize, NoteError> {
        self.ensure_healthy()?;
        let notes = self.validate_complete_snapshot_against_live()?.notes;
        if confirmed_digest != &self.current_state.avl_root_digest {
            return Ok(0);
        }
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let local_value = note.amount_collected;
            let mut updated = self
                .confirmations
                .get(&key)
                .cloned()
                .unwrap_or_else(NoteConfirmation::local_only);

            if updated.status != NoteConfirmationStatus::Confirmed
                || updated.confirmed_total_debt != Some(local_value)
            {
                updated.confirmed_total_debt = Some(local_value);
                updated.pending_total_debt = None;
                updated.pending_tx_id = None;
                updated.confirmed_box_id = Some(box_id.to_string());
                updated.confirmed_height = Some(height);
                updated.status = NoteConfirmationStatus::Confirmed;
                let storage_result = self.storage.store_confirmation(&key, &updated);
                self.quarantine_on_storage_failure(storage_result)?;
                self.confirmations.insert(key, updated);
                count += 1;
            }
        }

        if count > 0 {
            tracing::info!(
                "Reconciled {} notes to confirmed digest {} (box {})",
                count,
                hex::encode(confirmed_digest),
                box_id
            );
        }
        Ok(count)
    }

    /// Persist tracker-derived settlement progress without accepting an arbitrary
    /// replacement for the issuer-signed note.
    ///
    /// The signed cumulative debt, timestamp, recipient and signature are kept
    /// byte-for-byte. Only the unsigned local `amount_redeemed` field may advance,
    /// with checked arithmetic and a hard cap at `amount_collected`.
    #[cfg(test)]
    pub(crate) fn record_redemption_progress(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        redeemed_amount: u64,
    ) -> Result<IouNote, NoteError> {
        self.ensure_healthy()?;
        let target_key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey).to_bytes();
        let mut note = self
            .validate_complete_snapshot_against_live()?
            .notes
            .into_iter()
            .find_map(|(stored_issuer, note)| {
                (NoteKey::from_keys(&stored_issuer, &note.recipient_pubkey).to_bytes()
                    == target_key)
                    .then_some(note)
            })
            .ok_or_else(|| NoteError::StorageError("Note not found".to_string()))?;
        if note.verify_signature(issuer_pubkey).is_err() {
            self.poison();
            return Err(NoteError::InvalidSignature);
        }

        let committed_total = match self.get_total_debt(issuer_pubkey, recipient_pubkey) {
            Ok(total) => total,
            Err(error) => {
                // A persisted note without the matching live AVL entry means the
                // two authoritative views have diverged. Do not keep serving a
                // root or accepting writes from this manager instance.
                self.poison();
                return Err(error);
            }
        };
        if committed_total != note.amount_collected {
            self.poison();
            return Err(NoteError::StorageError(
                "Stored note does not match the live AVL commitment".to_string(),
            ));
        }

        let new_amount_redeemed = note
            .amount_redeemed
            .checked_add(redeemed_amount)
            .ok_or(NoteError::AmountOverflow)?;
        if new_amount_redeemed > note.amount_collected {
            return Err(NoteError::StorageError(
                "Redeemed amount exceeds cumulative debt".to_string(),
            ));
        }

        note.amount_redeemed = new_amount_redeemed;
        let storage_result =
            self.storage
                .store_note(issuer_pubkey, &note, self.current_state.avl_root_digest);
        self.quarantine_on_storage_failure(storage_result)?;
        Ok(note)
    }

    /// Get the total debt for a specific (issuer, receiver) pair from the AVL tree
    /// Returns the cumulative debt amount (totalDebt) stored in the tracker's AVL tree
    pub fn get_total_debt(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<u64, NoteError> {
        self.ensure_healthy()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Lookup value in AVL tree
        let value_bytes = self.avl_state.get(&key_bytes).ok_or_else(|| {
            NoteError::StorageError("Debt record not found in AVL tree".to_string())
        })?;

        // Convert 8-byte big-endian to u64
        if value_bytes.len() != 8 {
            return Err(NoteError::StorageError(
                "Invalid debt value format in AVL tree".to_string(),
            ));
        }

        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&value_bytes);
        Ok(u64::from_be_bytes(bytes))
    }

    /// Generate a tracker lookup proof for context var #8
    /// This proof verifies that totalDebt exists in the tracker's AVL tree
    pub fn generate_tracker_lookup_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<TrackerLookupProof, NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Get the total debt value
        let total_debt = self.get_total_debt(issuer_pubkey, recipient_pubkey)?;

        // Generate AVL proof for the lookup of this specific key
        let (avl_proof, _returned_value) = self.avl_state.generate_lookup_proof(key_bytes.to_vec());

        Ok(TrackerLookupProof {
            key: key_bytes,
            value: total_debt.to_be_bytes().to_vec(),
            proof: avl_proof,
        })
    }

    /// Get the already_redeemed amount for a specific (issuer, receiver) pair from the reserve AVL tree
    /// Returns the cumulative redeemed amount stored in the reserve's AVL tree
    pub fn get_already_redeemed(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<u64, NoteError> {
        self.ensure_healthy()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Lookup value in reserve AVL tree
        let value_bytes = match self.reserve_avl_state.get(&key_bytes) {
            Some(bytes) => bytes,
            None => return Ok(0u64), // First redemption - no already_redeemed amount
        };

        // Value format: timestamp (8 bytes BE) || redeemedAmount (8 bytes BE) = 16 bytes total
        if value_bytes.len() != 16 {
            return Err(NoteError::StorageError(format!(
                "Invalid reserve tree value format: expected 16 bytes (timestamp || redeemedAmount), got {}",
                value_bytes.len()
            )));
        }

        let mut redeemed_bytes = [0u8; 8];
        redeemed_bytes.copy_from_slice(&value_bytes[8..16]);
        Ok(u64::from_be_bytes(redeemed_bytes))
    }

    /// Generate a reserve lookup proof for context var #7
    /// This proof verifies that already_redeemed exists in the reserve's AVL tree
    /// Returns None proof for first redemption (no lookup proof needed)
    pub fn generate_reserve_lookup_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<ReserveLookupProof, NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Get the already_redeemed value
        let already_redeemed = self.get_already_redeemed(issuer_pubkey, recipient_pubkey)?;

        // For first redemption, no lookup proof is needed (per spec)
        let is_first_redemption = already_redeemed == 0;

        // Value: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes
        let mut value_bytes = Vec::with_capacity(16);
        // For lookup, use the stored timestamp if available; otherwise 0 for first redemption.
        // The persistent tree stores timestamp || already_redeemed, so retrieve the actual value.
        let stored_value = self.reserve_avl_state.get(&key_bytes).unwrap_or_else(|| {
            let mut empty_value = vec![0u8; 8]; // timestamp = 0
            empty_value.extend_from_slice(&already_redeemed.to_be_bytes());
            empty_value
        });
        if stored_value.len() == 16 {
            value_bytes.extend_from_slice(&stored_value);
        } else {
            // Fallback for old-format entries or first redemption: 0 timestamp
            value_bytes.extend_from_slice(&0u64.to_be_bytes());
            value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());
        }

        if is_first_redemption {
            Ok(ReserveLookupProof {
                key: key_bytes,
                value: value_bytes,
                proof: None, // Omitted for first redemption
            })
        } else {
            // Generate AVL proof for the lookup of this specific key
            let (avl_proof, _returned_value) = self
                .reserve_avl_state
                .generate_lookup_proof(key_bytes.to_vec());

            Ok(ReserveLookupProof {
                key: key_bytes,
                value: value_bytes,
                proof: Some(avl_proof),
            })
        }
    }

    /// Generate a reserve insert proof for context var #5 and return updated tree digest for R5.
    ///
    /// This operates on a temporary clone of the reserve AVL tree so that proof generation
    /// is idempotent and does not mutate the persistent tracker state. The persistent tree
    /// is only updated when `update_already_redeemed` is called after a successful on-chain
    /// redemption.
    ///
    /// Value format: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes total
    ///
    /// # Returns
    /// * `(insert_proof, updated_tree_digest)` - Proof bytes and serialized tree digest
    pub fn generate_reserve_insert_proof(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        timestamp: u64,
        new_already_redeemed: u64,
    ) -> Result<(Vec<u8>, Vec<u8>), NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();
        // Value: timestamp (8 bytes BE) || already_redeemed (8 bytes BE)
        let mut value_bytes = Vec::with_capacity(16);
        value_bytes.extend_from_slice(&timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&new_already_redeemed.to_be_bytes());

        // Use the non-mutating proof generator so repeated calls return the same proof.
        let (insert_proof, updated_digest) = self
            .reserve_avl_state
            .generate_insert_proof(key_bytes, value_bytes)
            .map_err(|e| {
                NoteError::StorageError(format!("Reserve AVL tree insert proof failed: {}", e))
            })?;

        Ok((insert_proof, updated_digest.to_vec()))
    }

    /// Current reserve AVL tree root digest (33 bytes). The on-chain reserve box being spent must
    /// have exactly this R5 digest for the insert proof to verify on-chain.
    pub fn reserve_state_digest(&self) -> Result<Vec<u8>, NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        Ok(self.reserve_avl_state.root_digest().to_vec())
    }

    /// Update the already_redeemed amount in the reserve AVL tree.
    /// Called after a successful redemption to prevent double-spending.
    /// Value format: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes total
    #[cfg(test)]
    pub(crate) fn update_already_redeemed(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        timestamp: u64,
        already_redeemed: u64,
    ) -> Result<(), NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();
        let mut value_bytes = Vec::with_capacity(16);
        value_bytes.extend_from_slice(&timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());

        // Update reserve AVL tree
        self.reserve_avl_state
            .update(key_bytes, value_bytes)
            .map_err(|e| {
                NoteError::StorageError(format!("Reserve AVL tree update failed: {}", e))
            })?;

        Ok(())
    }

    /// Generate proof for a specific note
    pub fn generate_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<NoteProof, NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Generate a lookup proof for the key in the AVL tree
        // This captures the path to the key, which can be verified against the root digest
        let (avl_proof, _value) = self.avl_state.generate_lookup_proof(key_bytes.to_vec());

        // Lookup the note to include in proof
        let note = self.lookup_note(issuer_pubkey, recipient_pubkey)?;

        Ok(NoteProof {
            note,
            avl_proof,
            operations: Vec::new(),
        })
    }

    /// Lookup a note by issuer and recipient
    pub fn lookup_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<IouNote, NoteError> {
        self.ensure_healthy()?;
        self.quarantine_on_storage_failure(self.storage.get_note(issuer_pubkey, recipient_pubkey))?
            .ok_or_else(|| NoteError::StorageError("Note not found".to_string()))
    }

    /// Get all notes for a specific issuer
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError> {
        self.ensure_healthy()?;
        self.quarantine_on_storage_failure(self.storage.get_issuer_notes(issuer_pubkey))
    }

    /// Calculate a conservative issuer-wide debt snapshot for an acceptance check.
    ///
    /// Every edge uses the greatest cumulative `totalDebt` known locally, pending,
    /// or confirmed. The candidate replaces its recipient edge exactly once, but a
    /// lower candidate cannot reduce an already observed cumulative value. When no
    /// recipient is supplied, the candidate is treated as a new edge. Gross debt is
    /// intentional here: local redemption state is not yet reconstructed on reorgs,
    /// so subtracting it could make a collateral check fail open.
    pub fn projected_issuer_gross_debt(
        &self,
        issuer_pubkey: &PubKey,
        candidate_recipient: Option<&PubKey>,
        candidate_total_debt: u64,
    ) -> Result<u64, NoteError> {
        self.ensure_healthy()?;
        // The versioned primary snapshot is the sole liability authority. A
        // malformed, missing or root-inconsistent snapshot fails this check.
        let notes = self
            .quarantine_on_storage_failure(self.storage.get_issuer_notes_strict(issuer_pubkey))?;
        let mut total = 0u64;
        let mut replaced_candidate_edge = false;

        for note in notes {
            let confirmation = self.get_confirmation(issuer_pubkey, &note.recipient_pubkey);
            let mut edge_debt = note.amount_collected;

            if let Some(confirmation) = confirmation {
                if let Some(confirmed) = confirmation.confirmed_total_debt {
                    edge_debt = edge_debt.max(confirmed);
                }
                if let Some(pending) = confirmation.pending_total_debt {
                    edge_debt = edge_debt.max(pending);
                }
            }

            if candidate_recipient == Some(&note.recipient_pubkey) {
                edge_debt = edge_debt.max(candidate_total_debt);
                replaced_candidate_edge = true;
            }

            total = total
                .checked_add(edge_debt)
                .ok_or(NoteError::AmountOverflow)?;
        }

        if candidate_recipient.is_none() || !replaced_candidate_edge {
            total = total
                .checked_add(candidate_total_debt)
                .ok_or(NoteError::AmountOverflow)?;
        }

        Ok(total)
    }

    /// Get all notes for a specific recipient
    pub fn get_recipient_notes(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        self.ensure_healthy()?;
        self.quarantine_on_storage_failure(self.storage.get_recipient_notes(recipient_pubkey))
    }

    /// Get all notes for a specific recipient with issuer information
    pub fn get_recipient_notes_with_issuer(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.ensure_healthy()?;
        self.quarantine_on_storage_failure(
            self.storage
                .get_recipient_notes_with_issuer(recipient_pubkey),
        )
    }

    /// Get all notes in the tracker
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError> {
        self.ensure_healthy()?;
        self.quarantine_on_storage_failure(self.storage.get_all_notes())
    }

    /// Get all notes in the tracker with issuer information
    pub fn get_all_notes_with_issuer(&self) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.ensure_healthy()?;
        self.quarantine_on_storage_failure(self.storage.get_all_notes_with_issuer())
    }

    /// Update the current state with latest AVL tree root
    fn update_state(&mut self) {
        self.current_state.avl_root_digest = self.avl_state.root_digest();
        // Update timestamp would be set to current time in real implementation
        self.current_state.last_update_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Return a fully validated snapshot of the current tracker state.
    ///
    /// Root exposure is a publication boundary: the checksummed BNS2 snapshot,
    /// replayed physical AVL state, and cached digest must still agree at the
    /// instant this value is produced.
    pub fn validated_state(&self) -> Result<TrackerState, NoteError> {
        self.ensure_healthy()?;
        self.validate_complete_snapshot_against_live()?;
        Ok(self.current_state.clone())
    }

    /// Compatibility accessor for local callers. It performs the same complete
    /// validation but preserves the historical reference-returning API. Server
    /// publication surfaces use `validated_state` through the sole actor so a
    /// quarantine becomes a typed unavailable response rather than a panic.
    pub fn get_state(&self) -> &TrackerState {
        if let Err(error) = self.validated_state() {
            panic!("Cannot expose invalid tracker state: {error:?}");
        }
        &self.current_state
    }
}

impl TrackerStateManager {
    /// Find the reserve box ID for an issuer using key matching
    pub fn find_reserve_box_id_for_issuer(
        &self,
        issuer_pubkey_hex: &str,
        reserve_tracker: &ReserveTracker,
    ) -> Result<String, NoteError> {
        self.ensure_healthy()?;
        // Get all reserves from the reserve tracker
        let all_reserves = reserve_tracker.get_all_reserves();

        // Since we now strip the 0x07 prefix when reading from registers,
        // we can do a direct match (with normalization for any remaining edge cases)
        for reserve in all_reserves {
            if issuer_pubkey_hex == reserve.owner_pubkey
                || normalize_public_key(issuer_pubkey_hex)
                    == normalize_public_key(&reserve.owner_pubkey)
            {
                return Ok(reserve.box_id);
            }
        }

        // If no matching reserve is found, return an error
        Err(NoteError::StorageError(format!(
            "No reserve found for issuer: {}",
            issuer_pubkey_hex
        )))
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

    /// Create and sign a new IOU note
    ///
    /// Message format: key || totalDebt || timestamp (48 bytes)
    /// where key = blake2b256(ownerKeyBytes || receiverKeyBytes)
    pub fn create_and_sign(
        recipient_pubkey: PubKey,
        amount_collected: u64,
        _timestamp: u64, // Kept for API compatibility but not used in signing message
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

        // Generate the signing message: key || totalDebt || timestamp
        let message = schnorr::signing_message(
            &issuer_pubkey,
            &recipient_pubkey,
            amount_collected,
            _timestamp,
        );

        // Use the chaincash-rs approach for Schnorr signing
        let signature =
            schnorr::schnorr_sign(&message, &secret_key.secret_bytes(), &issuer_pubkey)?;

        Ok(Self {
            recipient_pubkey,
            amount_collected,
            amount_redeemed: 0, // Start with no redemptions
            timestamp: _timestamp,
            signature,
        })
    }

    /// Generate the message that should be signed following the Basis protocol specification.
    ///
    /// message = blake2b256(ownerKeyBytes || receiverKeyBytes) || longToByteArray(totalDebt) || longToByteArray(timestamp)
    ///
    /// Total: 48 bytes
    ///
    /// # Arguments
    /// * `owner_pubkey` - Reserve owner's public key (the issuer of the IOU note)
    pub fn signing_message(&self, owner_pubkey: &PubKey) -> Vec<u8> {
        crate::schnorr::signing_message(
            owner_pubkey,
            &self.recipient_pubkey,
            self.amount_collected,
            self.timestamp,
        )
    }

    /// Verify the signature against an issuer public key using Schnorr signature verification
    /// This follows the chaincash-rs approach for Schnorr signature verification
    pub fn verify_signature(&self, issuer_pubkey: &PubKey) -> Result<(), NoteError> {
        let message = self.signing_message(issuer_pubkey);

        // Use the canonical Schnorr verification from basis_core
        let verifier = SchnorrVerifier;
        match verifier.verify_signature(&self.signature, &message, issuer_pubkey) {
            Ok(()) => Ok(()),
            Err(basis_core::traits::CryptoError::InvalidSignature) => {
                Err(NoteError::InvalidSignature)
            }
            Err(basis_core::traits::CryptoError::InvalidPublicKey) => {
                Err(NoteError::InvalidSignature)
            }
            Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => {
                Err(NoteError::InvalidSignature)
            }
            Err(basis_core::traits::CryptoError::InternalError(_)) => {
                Err(NoteError::InvalidSignature)
            }
        }
    }

    /// Get the recipient public key as a hex-encoded string
    pub fn recipient_pubkey_hex(&self) -> String {
        hex::encode(&self.recipient_pubkey)
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
