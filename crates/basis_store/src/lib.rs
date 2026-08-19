//! Core data structures for Basis tracker

pub mod avl_tree;

pub mod contract_compiler;
#[cfg(test)]
pub mod cross_validation_tests;
pub mod cross_verification;
pub mod ergo_scanner;
pub mod http;
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
pub mod basis_token_spec_tests;
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

use basis_core::impls::SchnorrVerifier;
use basis_core::traits::SignatureVerifier;
use std::path::Path;

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

/// Reserve information for a public key
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReserveInfo {
    /// On-chain collateral amount (ERG nanoERG for ERG reserves, raw token units for token reserves)
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
    /// Reserve token ID (hex-encoded) for token-backed reserves, or empty for ERG reserves.
    #[serde(default)]
    pub reserve_token_id: String,
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
    /// Per-issuer reserve AVL trees tracking hash(ownerKey || receiverKey) ->
    /// (timestamp, already_redeemed) (16 bytes BE). Each issuer has its own tree so that
    /// a fresh reserve for a new issuer starts from the empty tree digest and first
    /// redemptions succeed even if other issuers have redemption history.
    ///
    /// NOTE: Each on-chain reserve box has its own R5 digest. This design assumes one
    /// reserve per issuer; multi-reserve issuers would need per-reserve tracking.
    reserve_avl_states: std::collections::HashMap<PubKey, basis_trees::BasisAvlTree>,
    /// Per-note confirmation records, keyed by note key (32 bytes).
    confirmations: std::collections::HashMap<NoteKeyBytes, NoteConfirmation>,
}

impl TrackerStateManager {
    /// Create a new tracker state manager with the configured storage location.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        tracing::debug!("Creating TrackerStateManager...");

        // Use the configured storage location
        tracing::debug!("Opening note storage...");
        let storage_path = data_dir.as_ref().join("notes");
        let storage = match persistence::NoteStorage::open(&storage_path) {
            Ok(storage) => {
                tracing::debug!("Note storage opened successfully at: {:?}", storage_path);
                // Rebuild indices to ensure all existing notes are indexed
                // (especially important after upgrading to indexed storage)
                match storage.rebuild_indices() {
                    Ok(count) => tracing::info!("Note indices rebuilt: {} notes indexed", count),
                    Err(e) => tracing::warn!("Failed to rebuild note indices: {:?}", e),
                }
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

        // Per-issuer reserve AVL trees start empty; they are populated as redemptions complete.
        let reserve_avl_states = std::collections::HashMap::new();

        // Rebuild AVL tree from all stored notes to ensure consistency after restart
        let mut manager = Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
            reserve_avl_states,
            confirmations: std::collections::HashMap::new(),
        };

        if let Err(e) = manager.rebuild_avl_tree() {
            tracing::warn!("Failed to rebuild AVL tree from storage: {:?}", e);
        }

        // Rebuild confirmation records from storage and mark every stored note as
        // LocalOnly until the updater confirms otherwise.
        manager.rebuild_confirmations();

        // Rebuild per-issuer reserve AVL trees from the persisted journal so that
        // redemptions can continue after a server restart without manual repair.
        if let Err(e) = manager.rebuild_reserve_avl_trees() {
            tracing::warn!("Failed to rebuild reserve AVL trees from storage: {:?}", e);
        }

        tracing::debug!("TrackerStateManager created successfully");
        manager
    }

    /// Rebuild the AVL tree from all notes stored in the database.
    /// This is critical after server restart to ensure the AVL tree matches
    /// the on-chain commitment. AVL trees are insertion-order sensitive,
    /// so notes must be inserted in chronological order (by timestamp).
    pub fn rebuild_avl_tree(&mut self) -> Result<(), NoteError> {
        tracing::info!("Rebuilding AVL tree from stored notes...");

        let mut notes_with_issuer = self
            .storage
            .get_all_notes_with_issuer()
            .map_err(|e| NoteError::StorageError(format!("Failed to get all notes: {:?}", e)))?;

        if notes_with_issuer.is_empty() {
            tracing::info!("No stored notes found, AVL tree remains empty");
            return Ok(());
        }

        // Sort notes by timestamp ascending to ensure deterministic insertion order
        // AVL tree structure depends on insertion order, so we must insert in the
        // same order as when notes were originally created
        notes_with_issuer.sort_by_key(|(_, note)| note.timestamp);

        tracing::info!(
            "Inserting {} notes into AVL tree in chronological order...",
            notes_with_issuer.len()
        );

        for (issuer_pubkey, note) in &notes_with_issuer {
            let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
            let key_bytes = key.to_bytes();
            let value_bytes = note.amount_collected.to_be_bytes().to_vec();

            self.avl_state.update(key_bytes, value_bytes).map_err(|e| {
                NoteError::StorageError(format!("AVL tree update failed during rebuild: {:?}", e))
            })?;
        }

        self.update_state();
        let root_digest = self.current_state.avl_root_digest;
        tracing::info!(
            "AVL tree rebuilt successfully with root digest: {}",
            hex::encode(root_digest)
        );

        Ok(())
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

        let storage = match persistence::NoteStorage::open(&storage_path) {
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

                match persistence::NoteStorage::open(&storage_path_retry) {
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

        // Per-issuer reserve AVL trees start empty in test instances.
        let reserve_avl_states = std::collections::HashMap::new();

        tracing::debug!("TrackerStateManager created successfully");
        let mut manager = Self {
            avl_state,
            current_state: TrackerState {
                avl_root_digest: [0u8; 33],
                last_commit_height: 0,
                last_update_timestamp: 0,
            },
            storage,
            reserve_avl_states,
            confirmations: std::collections::HashMap::new(),
        };

        // Rebuild AVL tree and confirmations so test instances mirror production.
        if let Err(e) = manager.rebuild_avl_tree() {
            tracing::warn!("Failed to rebuild AVL tree in test instance: {:?}", e);
        }
        manager.rebuild_confirmations();

        manager
    }

    /// Add a new note to the tracker state
    /// Updates the AVL tree with hash(issuer||receiver) -> totalDebt mapping
    pub fn add_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;

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
        note.verify_signature(issuer_pubkey).map_err(|e| {
            tracing::error!("Invalid note signature when adding note: {:?}", e);
            NoteError::InvalidSignature
        })?;

        // Prepare AVL tree key: hash(issuer_pubkey || receiver_pubkey)
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Value is just the totalDebt (amount_collected) as 8-byte big-endian
        // This matches the contract spec: hash(A||B) -> totalDebt
        let value_bytes = note.amount_collected.to_be_bytes().to_vec();

        // Update AVL tree state first to ensure consistency
        let avl_result = self.avl_state.update(key_bytes.clone(), value_bytes);

        // Only proceed with database storage if AVL tree update succeeded
        match avl_result {
            Ok(()) => {
                // Now store note in persistent storage
                self.storage.store_note(issuer_pubkey, note)?;
                self.update_state();

                // Recompute the confirmation status for this note based on the
                // new local value versus the confirmed/pending values. The local
                // value has just changed, so the note is only Confirmed/Pending
                // if the new value matches what is already on-chain / in-flight.
                let mut key32 = [0u8; 32];
                key32.copy_from_slice(&key_bytes);
                self.recompute_confirmation_status(&key32, note.amount_collected);

                Ok(())
            }
            Err(e) => Err(NoteError::StorageError(e.to_string())),
        }
    }

    /// Convert an issuer/recipient pair into the fixed-size confirmation key.
    fn confirmation_key(issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> NoteKeyBytes {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let bytes = key.to_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    }

    /// Recompute a single note's confirmation status from its local value versus
    /// the cached confirmed/pending values. Persists the result best-effort.
    fn recompute_confirmation_status(&mut self, key: &NoteKeyBytes, local_value: u64) {
        let entry = self
            .confirmations
            .entry(*key)
            .or_insert_with(NoteConfirmation::local_only);

        let confirmed = entry.confirmed_total_debt;
        let pending = entry.pending_total_debt;

        entry.status = if Some(local_value) == confirmed {
            NoteConfirmationStatus::Confirmed
        } else if Some(local_value) == pending {
            NoteConfirmationStatus::Pending
        } else {
            NoteConfirmationStatus::LocalOnly
        };

        let _ = self.storage.store_confirmation(key, entry);
    }

    /// Rebuild the in-memory confirmation map from storage. Called on startup.
    ///
    /// On-chain state may have advanced while the server was offline, so we keep
    /// the persisted `confirmed_total_debt` metadata but recompute the status from
    /// the current local value and clear any pending in-flight state (pending
    /// transactions do not survive a restart).
    pub fn rebuild_confirmations(&mut self) {
        self.confirmations.clear();

        let notes = match self.storage.get_all_notes_with_issuer() {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Failed to load notes for confirmation rebuild: {:?}", e);
                return;
            }
        };

        let stored = self.storage.get_all_confirmations().unwrap_or_default();
        let stored_map: std::collections::HashMap<NoteKeyBytes, NoteConfirmation> =
            stored.into_iter().collect();

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let mut record = stored_map.get(&key).cloned().unwrap_or_default();

            // Pending state does not survive a restart.
            record.pending_total_debt = None;
            record.pending_tx_id = None;

            let local_value = note.amount_collected;
            record.status = if Some(local_value) == record.confirmed_total_debt {
                NoteConfirmationStatus::Confirmed
            } else {
                NoteConfirmationStatus::LocalOnly
            };

            self.confirmations.insert(key, record);
        }

        tracing::info!(
            "Rebuilt confirmation records for {} notes",
            self.confirmations.len()
        );
    }

    /// Get a clone of the confirmation record for a note, if one exists.
    pub fn get_confirmation(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Option<NoteConfirmation> {
        let key = Self::confirmation_key(issuer_pubkey, recipient_pubkey);
        self.confirmations.get(&key).cloned()
    }

    /// Get a snapshot of all confirmation records keyed by note key.
    pub fn all_confirmations(&self) -> std::collections::HashMap<NoteKeyBytes, NoteConfirmation> {
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
        let _ = (digest, submitted_height);
        let notes = self.storage.get_all_notes_with_issuer()?;
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let local_value = note.amount_collected;
            let entry = self
                .confirmations
                .entry(key)
                .or_insert_with(NoteConfirmation::local_only);

            if Some(local_value) != entry.confirmed_total_debt {
                entry.pending_total_debt = Some(local_value);
                entry.pending_tx_id = Some(tx_id.to_string());
                entry.status = NoteConfirmationStatus::Pending;
                let _ = self.storage.store_confirmation(&key, entry);
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

    /// Promote every `Pending` note to `Confirmed`, copying the pending value to
    /// the confirmed value and recording the confirming box metadata. Returns the
    /// number of notes transitioned to `Confirmed`.
    pub fn confirm_pending_notes(&mut self, box_id: &str, height: u64) -> Result<usize, NoteError> {
        let keys: Vec<NoteKeyBytes> = self.confirmations.keys().copied().collect();
        let mut count = 0usize;

        for key in keys {
            let should_confirm = self
                .confirmations
                .get(&key)
                .map(|c| c.status == NoteConfirmationStatus::Pending)
                .unwrap_or(false);

            if should_confirm {
                if let Some(entry) = self.confirmations.get_mut(&key) {
                    entry.confirmed_total_debt = entry.pending_total_debt;
                    entry.pending_total_debt = None;
                    entry.pending_tx_id = None;
                    entry.confirmed_box_id = Some(box_id.to_string());
                    entry.confirmed_height = Some(height);
                    entry.status = NoteConfirmationStatus::Confirmed;
                    let _ = self.storage.store_confirmation(&key, entry);
                    count += 1;
                }
            }
        }

        tracing::info!(
            "Confirmed {} notes in tracker box {} at height {}",
            count,
            box_id,
            height
        );
        Ok(count)
    }

    /// Revert every `Pending` note back to its prior state (used when an update
    /// transaction is dropped or rejected). Clears pending metadata and recomputes
    /// the status from the local value versus the confirmed value. Returns the
    /// number of notes reverted.
    pub fn revert_pending_notes(&mut self) -> Result<usize, NoteError> {
        let notes = self.storage.get_all_notes_with_issuer()?;
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let is_pending = self
                .confirmations
                .get(&key)
                .map(|c| c.status == NoteConfirmationStatus::Pending)
                .unwrap_or(false);

            if is_pending {
                if let Some(entry) = self.confirmations.get_mut(&key) {
                    entry.pending_total_debt = None;
                    entry.pending_tx_id = None;
                    let local_value = note.amount_collected;
                    entry.status = if Some(local_value) == entry.confirmed_total_debt {
                        NoteConfirmationStatus::Confirmed
                    } else {
                        NoteConfirmationStatus::LocalOnly
                    };
                    let _ = self.storage.store_confirmation(&key, entry);
                    count += 1;
                }
            }
        }

        tracing::info!("Reverted {} pending notes to local state", count);
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
        if confirmed_digest != &self.current_state.avl_root_digest {
            return Ok(0);
        }

        let notes = self.storage.get_all_notes_with_issuer()?;
        let mut count = 0usize;

        for (issuer_pubkey, note) in &notes {
            let key = Self::confirmation_key(issuer_pubkey, &note.recipient_pubkey);
            let local_value = note.amount_collected;
            let entry = self
                .confirmations
                .entry(key)
                .or_insert_with(NoteConfirmation::local_only);

            if entry.status != NoteConfirmationStatus::Confirmed
                || entry.confirmed_total_debt != Some(local_value)
            {
                entry.confirmed_total_debt = Some(local_value);
                entry.pending_total_debt = None;
                entry.pending_tx_id = None;
                entry.confirmed_box_id = Some(box_id.to_string());
                entry.confirmed_height = Some(height);
                entry.status = NoteConfirmationStatus::Confirmed;
                let _ = self.storage.store_confirmation(&key, entry);
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

    /// Update an existing note in the tracker state
    /// Updates the AVL tree with hash(issuer||receiver) -> totalDebt mapping
    pub fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Validate that timestamp is not in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NoteError::StorageError("Failed to get current time".to_string()))?
            .as_millis() as u64;

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

        // Prepare AVL tree key: hash(issuer_pubkey || receiver_pubkey)
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Value is just the totalDebt (amount_collected) as 8-byte big-endian
        // This matches the contract spec: hash(A||B) -> totalDebt
        let value_bytes = note.amount_collected.to_be_bytes().to_vec();

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

    /// Get the total debt for a specific (issuer, receiver) pair from the AVL tree
    /// Returns the cumulative debt amount (totalDebt) stored in the tracker's AVL tree
    pub fn get_total_debt(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<u64, NoteError> {
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

    /// Get or create the mutable reserve AVL tree for an issuer. Trees are created lazily so a
    /// fresh issuer starts from the empty tree digest, matching a newly created reserve's R5.
    fn reserve_tree_mut(
        &mut self,
        issuer_pubkey: &PubKey,
    ) -> Result<&mut basis_trees::BasisAvlTree, NoteError> {
        match self.reserve_avl_states.entry(*issuer_pubkey) {
            std::collections::hash_map::Entry::Occupied(e) => Ok(e.into_mut()),
            std::collections::hash_map::Entry::Vacant(e) => {
                let tree = basis_trees::BasisAvlTree::new().map_err(|e| {
                    NoteError::StorageError(format!(
                        "Failed to create reserve AVL tree for issuer: {:?}",
                        e
                    ))
                })?;
                Ok(e.insert(tree))
            }
        }
    }

    /// Get the already_redeemed amount for a specific (issuer, receiver) pair from the reserve AVL tree
    /// Returns the cumulative redeemed amount stored in the reserve's AVL tree
    pub fn get_already_redeemed(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<u64, NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        let reserve_tree = match self.reserve_avl_states.get(issuer_pubkey) {
            Some(tree) => tree,
            None => return Ok(0u64), // Fresh issuer: no reserve tree yet, first redemption.
        };

        // Lookup value in issuer's reserve AVL tree
        let value_bytes = match reserve_tree.get(&key_bytes) {
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
        let reserve_tree = match self.reserve_avl_states.get(issuer_pubkey) {
            Some(tree) => tree,
            None => {
                // Fresh issuer: value is 0 || already_redeemed.
                value_bytes.extend_from_slice(&0u64.to_be_bytes());
                value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());
                return Ok(ReserveLookupProof {
                    key: key_bytes,
                    value: value_bytes,
                    proof: None, // Omitted for first redemption
                });
            }
        };
        let stored_value = reserve_tree.get(&key_bytes).unwrap_or_else(|| {
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
            // Generate AVL proof for the lookup of this specific key from the issuer's tree
            let reserve_tree = match self.reserve_avl_states.get_mut(issuer_pubkey) {
                Some(tree) => tree,
                None => {
                    // No tree means no prior redemptions; this should have been a first redemption.
                    return Ok(ReserveLookupProof {
                        key: key_bytes,
                        value: value_bytes,
                        proof: None,
                    });
                }
            };
            let (avl_proof, _returned_value) =
                reserve_tree.generate_lookup_proof(key_bytes.to_vec());

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
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();
        // Value: timestamp (8 bytes BE) || already_redeemed (8 bytes BE)
        let mut value_bytes = Vec::with_capacity(16);
        value_bytes.extend_from_slice(&timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&new_already_redeemed.to_be_bytes());

        // Use the issuer's reserve tree (or an empty one if this is the first redemption for
        // the issuer). The proof's starting digest must match the on-chain reserve R5.
        let reserve_tree = match self.reserve_avl_states.get(issuer_pubkey) {
            Some(tree) => tree,
            None => {
                // Fresh issuer: generate proof against an empty tree.
                let empty_tree = basis_trees::BasisAvlTree::new().map_err(|e| {
                    NoteError::StorageError(format!(
                        "Failed to create empty reserve tree for issuer: {:?}",
                        e
                    ))
                })?;
                let (insert_proof, updated_digest) = empty_tree
                    .generate_insert_proof(key_bytes, value_bytes)
                    .map_err(|e| {
                        NoteError::StorageError(format!(
                            "Reserve AVL tree insert proof failed: {}",
                            e
                        ))
                    })?;
                return Ok((insert_proof, updated_digest.to_vec()));
            }
        };

        // Use the non-mutating proof generator so repeated calls return the same proof.
        let (insert_proof, updated_digest) = reserve_tree
            .generate_insert_proof(key_bytes, value_bytes)
            .map_err(|e| {
                NoteError::StorageError(format!("Reserve AVL tree insert proof failed: {}", e))
            })?;

        Ok((insert_proof, updated_digest.to_vec()))
    }

    /// Current reserve AVL tree root digest (33 bytes) for the given issuer. The on-chain reserve
    /// box being spent must have exactly this R5 digest for the insert proof to verify on-chain.
    pub fn reserve_state_digest(&self, issuer_pubkey: &PubKey) -> Vec<u8> {
        match self.reserve_avl_states.get(issuer_pubkey) {
            Some(tree) => tree.root_digest().to_vec(),
            None => {
                // Fresh issuer: return the empty-tree digest.
                basis_trees::BasisAvlTree::new()
                    .map(|t| t.root_digest().to_vec())
                    .unwrap_or_else(|_| vec![0u8; 33])
            }
        }
    }

    /// Apply a reserve AVL tree update in memory without writing to the journal.
    /// Used during journal replay on startup.
    fn apply_reserve_avl_update(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        timestamp: u64,
        already_redeemed: u64,
    ) -> Result<(), NoteError> {
        let reserve_tree = self.reserve_tree_mut(issuer_pubkey)?;
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();
        let mut value_bytes = Vec::with_capacity(16);
        value_bytes.extend_from_slice(&timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&already_redeemed.to_be_bytes());

        reserve_tree.update(key_bytes, value_bytes).map_err(|e| {
            NoteError::StorageError(format!("Reserve AVL tree update failed: {}", e))
        })
    }

    /// Rebuild per-issuer reserve AVL trees from the persisted journal.
    /// This must be called after `rebuild_avl_tree` so the note keys exist, but
    /// it only operates on the reserve trees, not the note tracker tree.
    pub fn rebuild_reserve_avl_trees(&mut self) -> Result<(), NoteError> {
        tracing::info!("Rebuilding reserve AVL trees from journal...");

        let updates = self
            .storage
            .iter_reserve_avl_updates()
            .map_err(|e| NoteError::StorageError(format!("Failed to read reserve AVL journal: {:?}", e)))?;

        if updates.is_empty() {
            tracing::info!("No reserve AVL journal entries found");
            return Ok(());
        }

        tracing::info!(
            "Replaying {} reserve AVL journal entries...",
            updates.len()
        );

        for (issuer_pubkey, note_key, timestamp, already_redeemed) in updates {
            // Reconstruct recipient pubkey from the note key. The note key is
            // blake2b256(issuer_pubkey || recipient_pubkey), so we look up the
            // stored note to obtain the recipient pubkey.
            let note = match self.storage.get_note_by_key(&note_key)? {
                Some(note) => note,
                None => {
                    tracing::warn!(
                        "Reserve AVL journal references unknown note key {}; skipping",
                        hex::encode(&note_key.to_bytes())
                    );
                    continue;
                }
            };

            if let Err(e) = self.apply_reserve_avl_update(
                &issuer_pubkey,
                &note.recipient_pubkey,
                timestamp,
                already_redeemed,
            ) {
                tracing::warn!(
                    "Failed to replay reserve AVL update for issuer {}: {:?}",
                    hex::encode(&issuer_pubkey),
                    e
                );
            }
        }

        tracing::info!("Reserve AVL trees rebuilt successfully");
        Ok(())
    }

    /// Update the already_redeemed amount in the issuer's reserve AVL tree.
    /// Called after a successful redemption to prevent double-spending.
    /// Value format: timestamp (8 bytes BE) || already_redeemed (8 bytes BE) = 16 bytes total
    pub fn update_already_redeemed(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
        timestamp: u64,
        already_redeemed: u64,
    ) -> Result<(), NoteError> {
        self.apply_reserve_avl_update(issuer_pubkey, recipient_pubkey, timestamp, already_redeemed)?;

        // Persist the update so the reserve tree can be rebuilt after a restart.
        self.storage.append_reserve_avl_update(
            issuer_pubkey,
            recipient_pubkey,
            timestamp,
            already_redeemed,
        )
    }

    /// Generate proof for a specific note
    pub fn generate_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<NoteProof, NoteError> {
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

    /// Get all notes for a specific recipient with issuer information
    pub fn get_recipient_notes_with_issuer(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.storage
            .get_recipient_notes_with_issuer(recipient_pubkey)
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
            .as_millis() as u64;
    }

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState {
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
        hex::encode(self.recipient_pubkey)
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

    if pubkey_bytes.is_empty() {
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
