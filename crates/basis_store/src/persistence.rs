//! Persistence layer for a bounded, versioned IOU-note state snapshot.

use crate::{
    blake2b256_hash, reserve_tracker::ExtendedReserveInfo, FreshGenerationApproval, IouNote,
    NoteConfirmation, NoteError, NoteKey, PendingTrackerPublication, PubKey, TrackerBoxInfo,
    TrackerGenerationConfig,
};
use fjall::{Config, Keyspace, PartitionCreateOptions, PersistMode};
use fs2::FileExt;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::Mutex,
};

const NOTE_STATE_MAGIC: &[u8; 4] = b"BNS2";
const LEGACY_NOTE_STATE_MAGIC: &[u8; 4] = b"BNS1";
const NOTE_STATE_KEY: &[u8] = b"note_state_v2";
const NOTE_SCHEMA_KEY: &[u8] = b"note_schema_v2";
const NOTE_GENERATION_KEY: &[u8] = b"tracker_generation_v1";
const PENDING_PUBLICATION_KEY: &[u8] = b"pending_publication_v1";
const GENERATION_MAGIC: &[u8; 4] = b"BNG1";
const PENDING_PUBLICATION_MAGIC: &[u8; 4] = b"BPA1";
const SNAPSHOT_CHECKSUM_DOMAIN: &[u8] = b"basis-note-state-snapshot-v2";
const GENERATION_CHECKSUM_DOMAIN: &[u8] = b"basis-tracker-generation-v1";
const PENDING_PUBLICATION_CHECKSUM_DOMAIN: &[u8] = b"basis-pending-publication-v1";
const NOTE_STATE_HEADER_LEN: usize = 4 + 4 + 33 + 32;
const NOTE_RECORD_LEN: usize = 33 + 8 + 8 + 8 + 65 + 33;
const MAX_NOTE_COUNT: usize = 50_000;
const GENERATION_MANIFEST_BODY_LEN: usize = 4 + 32 + 33 + 1 + 33;
const GENERATION_MANIFEST_LEN: usize = GENERATION_MANIFEST_BODY_LEN + 32;
const PENDING_PUBLICATION_BODY_LEN: usize = 4 + 33 + 32 + 8;
const PENDING_PUBLICATION_LEN: usize = PENDING_PUBLICATION_BODY_LEN + 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredNoteState {
    pub avl_root_digest: [u8; 33],
    pub notes: Vec<(PubKey, IouNote)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredGeneration {
    tracker_nft_id: [u8; 32],
    bootstrap_root: [u8; 33],
    anchor_root: Option<[u8; 33]>,
}

/// Database storage for versioned IOU note snapshots.
///
/// One `iou_notes` value contains the complete ordered live-note set and the AVL
/// root it must reproduce. Repeated updates replace a record in place, so disk
/// and restart work are bounded by the live edge count rather than history.
pub(crate) struct NoteStorage {
    keyspace: Keyspace,
    notes_partition: fjall::Partition,
    schema_partition: fjall::Partition,
    confirmations_partition: fjall::Partition,
    write_lock: Mutex<()>,
    _writer_file_lock: File,
    #[cfg(test)]
    fail_next_persist: AtomicBool,
    #[cfg(test)]
    capacity_limit: AtomicUsize,
}

/// Database storage for scanner metadata
#[derive(Clone)]
pub struct ScannerMetadataStorage {
    partition: fjall::Partition,
}

/// Database storage for reserve information
#[derive(Clone)]
pub struct ReserveStorage {
    partition: fjall::Partition,
}

/// Database storage for tracker information
#[derive(Clone)]
pub struct TrackerStorage {
    partition: fjall::Partition,
}

/// Database storage for per-recipient acceptance policies
///
/// Stores signed acceptance policies uploaded by recipients.
/// Key: recipient_pubkey (33 bytes), Value: (timestamp, policy_json, signature)
#[derive(Clone)]
pub struct AcceptancePolicyStorage {
    partition: fjall::Partition,
}

/// Stored acceptance policy record
#[derive(Debug, Clone)]
pub struct StoredPolicy {
    /// Unix timestamp when the policy was uploaded
    pub timestamp: u64,
    /// Serialized policy JSON string
    pub policy_json: String,
    /// Hex-encoded Schnorr signature (65 bytes = 130 hex chars)
    pub signature: String,
}

impl ScannerMetadataStorage {
    /// Open or create a new scanner metadata storage database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NoteError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| NoteError::StorageError(format!("Failed to open database: {}", e)))?;

        let partition = keyspace
            .open_partition("scanner_metadata", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open partition: {}", e)))?;

        Ok(Self { partition })
    }

    /// Store scan ID for a specific scan name
    pub fn store_scan_id(&self, scan_name: &str, scan_id: i32) -> Result<(), NoteError> {
        let value_bytes = scan_id.to_be_bytes().to_vec();
        self.partition
            .insert(scan_name.as_bytes(), &value_bytes)
            .map_err(|e| NoteError::StorageError(format!("Failed to store scan ID: {}", e)))?;
        Ok(())
    }

    /// Retrieve scan ID for a specific scan name
    pub fn get_scan_id(&self, scan_name: &str) -> Result<Option<i32>, NoteError> {
        match self.partition.get(scan_name.as_bytes()) {
            Ok(Some(value_bytes)) => {
                if value_bytes.len() == 4 {
                    let scan_id = i32::from_be_bytes(value_bytes[0..4].try_into().unwrap());
                    Ok(Some(scan_id))
                } else {
                    Err(NoteError::StorageError(
                        "Invalid scan ID format".to_string(),
                    ))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to get scan ID: {}",
                e
            ))),
        }
    }

    /// Remove scan ID for a specific scan name
    pub fn remove_scan_id(&self, scan_name: &str) -> Result<(), NoteError> {
        self.partition
            .remove(scan_name.as_bytes())
            .map_err(|e| NoteError::StorageError(format!("Failed to remove scan ID: {}", e)))?;
        Ok(())
    }

    /// Store blockchain height with fetch timestamp
    /// Key: "blockchain_height", Value: 8 bytes height + 8 bytes timestamp (u64 BE)
    pub fn store_blockchain_height(&self, height: u64, timestamp: u64) -> Result<(), NoteError> {
        let mut value = Vec::with_capacity(16);
        value.extend_from_slice(&height.to_be_bytes());
        value.extend_from_slice(&timestamp.to_be_bytes());
        self.partition
            .insert("blockchain_height", &value)
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to store blockchain height: {}", e))
            })?;
        Ok(())
    }

    /// Retrieve cached blockchain height and fetch timestamp
    /// Returns Some((height, timestamp)) if present, None otherwise
    pub fn get_blockchain_height(&self) -> Result<Option<(u64, u64)>, NoteError> {
        match self.partition.get("blockchain_height") {
            Ok(Some(value_bytes)) => {
                if value_bytes.len() == 16 {
                    let height = u64::from_be_bytes(value_bytes[0..8].try_into().unwrap());
                    let timestamp = u64::from_be_bytes(value_bytes[8..16].try_into().unwrap());
                    Ok(Some((height, timestamp)))
                } else {
                    Err(NoteError::StorageError(
                        "Invalid blockchain height format".to_string(),
                    ))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to get blockchain height: {}",
                e
            ))),
        }
    }
}

impl NoteStorage {
    fn capacity_limit(&self) -> usize {
        #[cfg(test)]
        {
            return self.capacity_limit.load(Ordering::SeqCst);
        }
        #[cfg(not(test))]
        {
            MAX_NOTE_COUNT
        }
    }

    /// Open or create a new note storage database with extra indices
    pub(crate) fn open<P: AsRef<Path>>(
        path: P,
        generation: TrackerGenerationConfig,
    ) -> Result<Self, NoteError> {
        let path = path.as_ref();
        std::fs::create_dir_all(path).map_err(|e| {
            NoteError::StorageError(format!("Failed to create note storage directory: {}", e))
        })?;
        let writer_lock_path = path.join(".basis-writer.lock");
        let writer_file_lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&writer_lock_path)
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to open note writer lock: {}", e))
            })?;
        writer_file_lock.try_lock_exclusive().map_err(|e| {
            NoteError::StorageError(format!(
                "Note storage already has an active writer ({}): {}",
                writer_lock_path.display(),
                e
            ))
        })?;

        let keyspace = Config::new(path)
            .open()
            .map_err(|e| NoteError::StorageError(format!("Failed to open database: {}", e)))?;

        let notes_partition = keyspace
            .open_partition("iou_notes", PartitionCreateOptions::default())
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to open notes partition: {}", e))
            })?;

        if notes_partition
            .get(NOTE_STATE_KEY)
            .map_err(|e| NoteError::StorageError(format!("Failed to inspect note state: {}", e)))?
            .is_none()
            && !notes_partition.is_empty().map_err(|e| {
                NoteError::StorageError(format!("Failed to inspect legacy note state: {}", e))
            })?
        {
            return Err(NoteError::MigrationRequired(
                "Legacy note rows require explicit export/migration or an approved reset"
                    .to_string(),
            ));
        }

        let confirmations_partition = keyspace
            .open_partition("confirmations", PartitionCreateOptions::default())
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to open confirmations partition: {}", e))
            })?;

        let schema_partition = keyspace
            .open_partition("note_schema", PartitionCreateOptions::default())
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to open note schema partition: {}", e))
            })?;

        let storage = Self {
            keyspace,
            notes_partition,
            schema_partition,
            confirmations_partition,
            write_lock: Mutex::new(()),
            _writer_file_lock: writer_file_lock,
            #[cfg(test)]
            fail_next_persist: AtomicBool::new(false),
            #[cfg(test)]
            capacity_limit: AtomicUsize::new(MAX_NOTE_COUNT),
        };
        storage.ensure_state_initialized(generation)?;
        Ok(storage)
    }

    fn ensure_state_initialized(
        &self,
        generation: TrackerGenerationConfig,
    ) -> Result<(), NoteError> {
        let schema = self
            .schema_partition
            .get(NOTE_SCHEMA_KEY)
            .map_err(|e| NoteError::StorageError(format!("Failed to read note schema: {}", e)))?;
        let state = self
            .notes_partition
            .get(NOTE_STATE_KEY)
            .map_err(|e| NoteError::StorageError(format!("Failed to read note state: {}", e)))?;

        // Inspect the generation binding before mutating either authoritative
        // partition.  A manifest without a complete schema/state pair is an
        // interrupted or foreign initialization, not permission to synthesize
        // a fresh empty state over the same tracker NFT.
        let stored_generation = self
            .schema_partition
            .get(NOTE_GENERATION_KEY)
            .map_err(|e| NoteError::StorageError(format!("Failed to read generation: {}", e)))?;
        let pending_publication =
            self.schema_partition
                .get(PENDING_PUBLICATION_KEY)
                .map_err(|e| {
                    NoteError::StorageError(format!("Failed to read pending publication: {}", e))
                })?;
        if let Some(bytes) = pending_publication.as_ref() {
            // Validate before any initialization write. A malformed or orphaned
            // external-effect receipt is never permission to synthesize a new
            // empty generation over potentially published state.
            Self::deserialize_pending_publication(bytes.as_ref())?;
        }

        match (schema, state, stored_generation) {
            (Some(schema), Some(_), Some(generation_bytes)) => {
                if schema.as_ref() != NOTE_STATE_MAGIC {
                    return Err(NoteError::MigrationRequired(
                        "Unsupported note storage schema requires an explicit migration"
                            .to_string(),
                    ));
                }
                self.read_state_strict()?;
                self.validate_generation_manifest(generation, generation_bytes.as_ref())
            }
            (None, None, None) => {
                if pending_publication.is_some() {
                    return Err(NoteError::GenerationMismatch(
                        "Pending tracker publication exists without a complete authoritative generation"
                            .to_string(),
                    ));
                }
                if generation.fresh_generation != FreshGenerationApproval::Approve {
                    return Err(NoteError::GenerationBindingRequired(
                        "A new data directory requires explicit fresh tracker generation approval"
                            .to_string(),
                    ));
                }
                if !self.notes_partition.is_empty().map_err(|e| {
                    NoteError::StorageError(format!("Failed to inspect legacy note state: {}", e))
                })? {
                    return Err(NoteError::MigrationRequired(
                        "Legacy note rows require explicit export/migration or an approved reset"
                            .to_string(),
                    ));
                }
                let empty_root = basis_trees::BasisAvlTree::new()
                    .map_err(|e| NoteError::StorageError(e.to_string()))?
                    .root_digest();
                let empty = StoredNoteState {
                    avl_root_digest: empty_root,
                    notes: Vec::new(),
                };
                let manifest = StoredGeneration {
                    tracker_nft_id: generation.tracker_nft_id,
                    bootstrap_root: empty.avl_root_digest,
                    anchor_root: None,
                };

                // Publish all three initialization records as one durability
                // attempt.  If any insert or the sync has an unknown outcome,
                // the next open sees a partial tuple and fails closed without
                // writing a replacement.
                self.schema_partition
                    .insert(NOTE_GENERATION_KEY, Self::serialize_generation(&manifest))
                    .map_err(|e| {
                        NoteError::StorageOutcomeUnknown(format!(
                            "Tracker generation binding outcome is unknown: {}",
                            e
                        ))
                    })?;
                self.notes_partition
                    .insert(NOTE_STATE_KEY, Self::serialize_note_state(&empty)?)
                    .map_err(|e| {
                        NoteError::StorageOutcomeUnknown(format!(
                            "Empty note state initialization outcome is unknown: {}",
                            e
                        ))
                    })?;
                self.schema_partition
                    .insert(NOTE_SCHEMA_KEY, NOTE_STATE_MAGIC)
                    .map_err(|e| {
                        NoteError::StorageOutcomeUnknown(format!(
                            "Note schema initialization outcome is unknown: {}",
                            e
                        ))
                    })?;
                self.keyspace.persist(PersistMode::SyncData).map_err(|e| {
                    NoteError::StorageOutcomeUnknown(format!(
                        "Tracker generation initialization durability is unknown: {}",
                        e
                    ))
                })
            }
            (Some(_), Some(_), None) => Err(NoteError::GenerationBindingRequired(
                "Complete note state is missing its tracker generation manifest; explicit migration is required"
                    .to_string(),
            )),
            (None, None, Some(_)) | (None, Some(_), Some(_)) | (Some(_), None, Some(_)) => {
                Err(NoteError::GenerationMismatch(
                    "Tracker generation manifest exists without a complete authoritative schema/state pair"
                        .to_string(),
                ))
            }
            (Some(_), None, None) => Err(NoteError::StorageError(
                "Note schema exists without authoritative state".to_string(),
            )),
            (None, Some(_), None) => Err(NoteError::StorageError(
                "Authoritative note state exists without its schema and generation binding"
                    .to_string(),
            )),
        }
    }

    fn validate_generation_manifest(
        &self,
        generation: TrackerGenerationConfig,
        stored: &[u8],
    ) -> Result<(), NoteError> {
        let manifest = Self::deserialize_generation(stored)?;
        if manifest.tracker_nft_id != generation.tracker_nft_id {
            return Err(NoteError::GenerationMismatch(
                "Configured tracker NFT does not match the generation bound to this data directory"
                    .to_string(),
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn remove_state_for_test(&self) -> Result<(), NoteError> {
        self.notes_partition
            .remove(NOTE_STATE_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn remove_schema_for_test(&self) -> Result<(), NoteError> {
        self.schema_partition
            .remove(NOTE_SCHEMA_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn remove_generation_for_test(&self) -> Result<(), NoteError> {
        self.schema_partition
            .remove(NOTE_GENERATION_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn persist_for_test(&self) -> Result<(), NoteError> {
        self.keyspace
            .persist(PersistMode::SyncData)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn remove_confirmation_for_test(
        &self,
        key_bytes: &[u8; 32],
    ) -> Result<(), NoteError> {
        self.confirmations_partition
            .remove(key_bytes)
            .map_err(|e| NoteError::StorageError(e.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncData)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn corrupt_state_for_test(&self) -> Result<(), NoteError> {
        self.notes_partition
            .insert(NOTE_STATE_KEY, [0u8])
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn tamper_first_total_debt_for_test(&self) -> Result<(), NoteError> {
        let mut bytes = self
            .notes_partition
            .get(NOTE_STATE_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))?
            .ok_or_else(|| NoteError::StorageError("Note state missing".to_string()))?
            .to_vec();
        let amount_last_byte = NOTE_STATE_HEADER_LEN + 33 + 7;
        if bytes.len() <= amount_last_byte {
            return Err(NoteError::StorageError(
                "Note state has no record to tamper".to_string(),
            ));
        }
        bytes[amount_last_byte] ^= 1;
        self.notes_partition
            .insert(NOTE_STATE_KEY, bytes)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn rewrite_first_total_debt_with_valid_checksum_for_test(
        &self,
        amount_collected: u64,
    ) -> Result<(), NoteError> {
        let mut state = self.read_state_strict()?;
        let (_, note) = state
            .notes
            .first_mut()
            .ok_or_else(|| NoteError::StorageError("Note state has no record".to_string()))?;
        note.amount_collected = amount_collected;
        self.notes_partition
            .insert(NOTE_STATE_KEY, Self::serialize_note_state(&state)?)
            .map_err(|e| NoteError::StorageError(e.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncData)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn set_declared_note_count_for_test(&self, count: u32) -> Result<(), NoteError> {
        let mut bytes = self
            .notes_partition
            .get(NOTE_STATE_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))?
            .ok_or_else(|| NoteError::StorageError("Note state missing".to_string()))?
            .to_vec();
        if bytes.len() < 8 {
            return Err(NoteError::StorageError(
                "Note state header is truncated".to_string(),
            ));
        }
        bytes[4..8].copy_from_slice(&count.to_be_bytes());
        self.notes_partition
            .insert(NOTE_STATE_KEY, bytes)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn insert_unexpected_note_row_for_test(&self) -> Result<(), NoteError> {
        self.notes_partition
            .insert(b"unexpected", [0u8])
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn fail_next_persist_for_test(&self) {
        self.fail_next_persist.store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn set_capacity_limit_for_test(&self, limit: usize) {
        self.capacity_limit.store(limit, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn tamper_first_redeemed_amount_for_test(&self) -> Result<(), NoteError> {
        let mut bytes = self
            .notes_partition
            .get(NOTE_STATE_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))?
            .ok_or_else(|| NoteError::StorageError("Note state missing".to_string()))?
            .to_vec();
        let amount_last_byte = NOTE_STATE_HEADER_LEN + 33 + 8 + 7;
        if bytes.len() <= amount_last_byte {
            return Err(NoteError::StorageError(
                "Note state has no redeemed amount to tamper".to_string(),
            ));
        }
        bytes[amount_last_byte] ^= 1;
        self.notes_partition
            .insert(NOTE_STATE_KEY, bytes)
            .map_err(|e| NoteError::StorageError(e.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncData)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn tamper_generation_manifest_for_test(&self) -> Result<(), NoteError> {
        let mut bytes = self
            .schema_partition
            .get(NOTE_GENERATION_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))?
            .ok_or_else(|| NoteError::StorageError("Generation missing".to_string()))?
            .to_vec();
        bytes[4] ^= 1;
        self.schema_partition
            .insert(NOTE_GENERATION_KEY, bytes)
            .map_err(|e| NoteError::StorageError(e.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncData)
            .map_err(|e| NoteError::StorageError(e.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn generation_for_test(
        &self,
    ) -> Result<([u8; 32], [u8; 33], Option<[u8; 33]>), NoteError> {
        let bytes = self
            .schema_partition
            .get(NOTE_GENERATION_KEY)
            .map_err(|e| NoteError::StorageError(e.to_string()))?
            .ok_or_else(|| NoteError::StorageError("Generation missing".to_string()))?;
        let generation = Self::deserialize_generation(bytes.as_ref())?;
        Ok((
            generation.tracker_nft_id,
            generation.bootstrap_root,
            generation.anchor_root,
        ))
    }

    fn deserialize_note_state(value_bytes: &[u8]) -> Result<StoredNoteState, NoteError> {
        if value_bytes.len() >= 4 && &value_bytes[..4] == LEGACY_NOTE_STATE_MAGIC {
            return Err(NoteError::MigrationRequired(
                "BNS1 note state requires explicit migration to checksummed BNS2".to_string(),
            ));
        }
        if value_bytes.len() < NOTE_STATE_HEADER_LEN || &value_bytes[..4] != NOTE_STATE_MAGIC {
            return Err(NoteError::StorageError(
                "Unsupported or malformed note state; explicit migration is required".to_string(),
            ));
        }

        let count = u32::from_be_bytes(value_bytes[4..8].try_into().unwrap()) as usize;
        if count > MAX_NOTE_COUNT {
            return Err(NoteError::StorageError(
                "Stored note count exceeds configured bound".to_string(),
            ));
        }
        let records_len = count.checked_mul(NOTE_RECORD_LEN).ok_or_else(|| {
            NoteError::StorageError("Stored note state length overflow".to_string())
        })?;
        let expected_len = NOTE_STATE_HEADER_LEN
            .checked_add(records_len)
            .ok_or_else(|| {
                NoteError::StorageError("Stored note state length overflow".to_string())
            })?;
        if value_bytes.len() != expected_len {
            return Err(NoteError::StorageError(
                "Stored note state length does not match count".to_string(),
            ));
        }

        let mut avl_root_digest = [0u8; 33];
        avl_root_digest.copy_from_slice(&value_bytes[8..41]);
        let stored_checksum = &value_bytes[41..73];
        let calculated_checksum = Self::snapshot_checksum(
            count,
            &avl_root_digest,
            &value_bytes[NOTE_STATE_HEADER_LEN..],
        );
        if stored_checksum != calculated_checksum {
            return Err(NoteError::StorageError(
                "Authoritative note snapshot checksum mismatch".to_string(),
            ));
        }
        let mut notes = Vec::with_capacity(count);
        let mut seen_keys = std::collections::HashSet::with_capacity(count);
        let mut offset = NOTE_STATE_HEADER_LEN;
        for _ in 0..count {
            let issuer_pubkey: PubKey = value_bytes[offset..offset + 33].try_into().unwrap();
            offset += 33;
            let amount_collected =
                u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let amount_redeemed =
                u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let timestamp = u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let signature: [u8; 65] = value_bytes[offset..offset + 65].try_into().unwrap();
            offset += 65;
            let recipient_pubkey: PubKey = value_bytes[offset..offset + 33].try_into().unwrap();
            offset += 33;
            let key = NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey).to_bytes();
            if !seen_keys.insert(key) {
                return Err(NoteError::StorageError(
                    "Duplicate issuer-recipient edge in note state".to_string(),
                ));
            }
            notes.push((
                issuer_pubkey,
                IouNote {
                    recipient_pubkey,
                    amount_collected,
                    amount_redeemed,
                    timestamp,
                    signature,
                },
            ));
        }

        Ok(StoredNoteState {
            avl_root_digest,
            notes,
        })
    }

    fn serialize_note_state(state: &StoredNoteState) -> Result<Vec<u8>, NoteError> {
        if state.notes.len() > MAX_NOTE_COUNT {
            return Err(NoteError::CapacityExceeded {
                limit: MAX_NOTE_COUNT,
            });
        }
        let count = u32::try_from(state.notes.len())
            .map_err(|_| NoteError::StorageError("Note count overflow".to_string()))?;
        let capacity = NOTE_STATE_HEADER_LEN
            .checked_add(
                state
                    .notes
                    .len()
                    .checked_mul(NOTE_RECORD_LEN)
                    .ok_or_else(|| {
                        NoteError::StorageError("Note state length overflow".to_string())
                    })?,
            )
            .ok_or_else(|| NoteError::StorageError("Note state length overflow".to_string()))?;
        let mut records = Vec::with_capacity(state.notes.len() * NOTE_RECORD_LEN);
        for (issuer_pubkey, note) in &state.notes {
            records.extend_from_slice(issuer_pubkey);
            records.extend_from_slice(&note.amount_collected.to_be_bytes());
            records.extend_from_slice(&note.amount_redeemed.to_be_bytes());
            records.extend_from_slice(&note.timestamp.to_be_bytes());
            records.extend_from_slice(&note.signature);
            records.extend_from_slice(&note.recipient_pubkey);
        }
        let checksum = Self::snapshot_checksum(state.notes.len(), &state.avl_root_digest, &records);
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(NOTE_STATE_MAGIC);
        bytes.extend_from_slice(&count.to_be_bytes());
        bytes.extend_from_slice(&state.avl_root_digest);
        bytes.extend_from_slice(&checksum);
        bytes.extend_from_slice(&records);
        Ok(bytes)
    }

    fn snapshot_checksum(count: usize, root: &[u8; 33], records: &[u8]) -> [u8; 32] {
        let mut bytes = Vec::with_capacity(
            SNAPSHOT_CHECKSUM_DOMAIN.len()
                + std::mem::size_of::<u32>()
                + root.len()
                + records.len(),
        );
        bytes.extend_from_slice(SNAPSHOT_CHECKSUM_DOMAIN);
        bytes.extend_from_slice(&(count as u32).to_be_bytes());
        bytes.extend_from_slice(root);
        bytes.extend_from_slice(records);
        blake2b256_hash(&bytes)
    }

    fn serialize_generation(generation: &StoredGeneration) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(GENERATION_MANIFEST_LEN);
        bytes.extend_from_slice(GENERATION_MAGIC);
        bytes.extend_from_slice(&generation.tracker_nft_id);
        bytes.extend_from_slice(&generation.bootstrap_root);
        match generation.anchor_root {
            Some(root) => {
                bytes.push(1);
                bytes.extend_from_slice(&root);
            }
            None => {
                bytes.push(0);
                bytes.extend_from_slice(&[0u8; 33]);
            }
        }
        let mut checksum_input = Vec::with_capacity(GENERATION_CHECKSUM_DOMAIN.len() + bytes.len());
        checksum_input.extend_from_slice(GENERATION_CHECKSUM_DOMAIN);
        checksum_input.extend_from_slice(&bytes);
        bytes.extend_from_slice(&blake2b256_hash(&checksum_input));
        bytes
    }

    fn deserialize_generation(bytes: &[u8]) -> Result<StoredGeneration, NoteError> {
        if bytes.len() != GENERATION_MANIFEST_LEN || &bytes[..4] != GENERATION_MAGIC {
            return Err(NoteError::StorageError(
                "Malformed tracker generation manifest".to_string(),
            ));
        }
        let mut checksum_input =
            Vec::with_capacity(GENERATION_CHECKSUM_DOMAIN.len() + GENERATION_MANIFEST_BODY_LEN);
        checksum_input.extend_from_slice(GENERATION_CHECKSUM_DOMAIN);
        checksum_input.extend_from_slice(&bytes[..GENERATION_MANIFEST_BODY_LEN]);
        if bytes[GENERATION_MANIFEST_BODY_LEN..] != blake2b256_hash(&checksum_input) {
            return Err(NoteError::StorageError(
                "Tracker generation manifest checksum mismatch".to_string(),
            ));
        }
        let mut tracker_nft_id = [0u8; 32];
        tracker_nft_id.copy_from_slice(&bytes[4..36]);
        let mut bootstrap_root = [0u8; 33];
        bootstrap_root.copy_from_slice(&bytes[36..69]);
        let anchor_root = match bytes[69] {
            0 if bytes[70..103].iter().all(|byte| *byte == 0) => None,
            1 => {
                let mut root = [0u8; 33];
                root.copy_from_slice(&bytes[70..103]);
                Some(root)
            }
            _ => {
                return Err(NoteError::StorageError(
                    "Malformed tracker generation anchor".to_string(),
                ))
            }
        };
        Ok(StoredGeneration {
            tracker_nft_id,
            bootstrap_root,
            anchor_root,
        })
    }

    fn serialize_pending_publication(
        publication: &PendingTrackerPublication,
    ) -> Result<Vec<u8>, NoteError> {
        let tx_id = hex::decode(&publication.tx_id).map_err(|_| NoteError::InvalidTransactionId)?;
        if tx_id.len() != 32 {
            return Err(NoteError::InvalidTransactionId);
        }
        let mut bytes = Vec::with_capacity(PENDING_PUBLICATION_LEN);
        bytes.extend_from_slice(PENDING_PUBLICATION_MAGIC);
        bytes.extend_from_slice(&publication.digest);
        bytes.extend_from_slice(&tx_id);
        bytes.extend_from_slice(&publication.submitted_height.to_be_bytes());
        let mut checksum_input =
            Vec::with_capacity(PENDING_PUBLICATION_CHECKSUM_DOMAIN.len() + bytes.len());
        checksum_input.extend_from_slice(PENDING_PUBLICATION_CHECKSUM_DOMAIN);
        checksum_input.extend_from_slice(&bytes);
        bytes.extend_from_slice(&blake2b256_hash(&checksum_input));
        Ok(bytes)
    }

    fn deserialize_pending_publication(
        bytes: &[u8],
    ) -> Result<PendingTrackerPublication, NoteError> {
        if bytes.len() != PENDING_PUBLICATION_LEN || &bytes[..4] != PENDING_PUBLICATION_MAGIC {
            return Err(NoteError::StorageError(
                "Malformed pending tracker publication".to_string(),
            ));
        }
        let mut checksum_input = Vec::with_capacity(
            PENDING_PUBLICATION_CHECKSUM_DOMAIN.len() + PENDING_PUBLICATION_BODY_LEN,
        );
        checksum_input.extend_from_slice(PENDING_PUBLICATION_CHECKSUM_DOMAIN);
        checksum_input.extend_from_slice(&bytes[..PENDING_PUBLICATION_BODY_LEN]);
        if bytes[PENDING_PUBLICATION_BODY_LEN..] != blake2b256_hash(&checksum_input) {
            return Err(NoteError::StorageError(
                "Pending tracker publication checksum mismatch".to_string(),
            ));
        }
        let mut digest = [0u8; 33];
        digest.copy_from_slice(&bytes[4..37]);
        let tx_id = hex::encode(&bytes[37..69]);
        let submitted_height =
            u64::from_be_bytes(bytes[69..77].try_into().map_err(|_| {
                NoteError::StorageError("Malformed publication height".to_string())
            })?);
        Ok(PendingTrackerPublication {
            digest,
            tx_id,
            submitted_height,
        })
    }

    pub(crate) fn pending_publication(
        &self,
    ) -> Result<Option<PendingTrackerPublication>, NoteError> {
        self.schema_partition
            .get(PENDING_PUBLICATION_KEY)
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to read pending publication: {}", e))
            })?
            .map(|bytes| Self::deserialize_pending_publication(bytes.as_ref()))
            .transpose()
    }

    pub(crate) fn store_pending_publication(
        &self,
        publication: &PendingTrackerPublication,
    ) -> Result<(), NoteError> {
        if publication.tx_id.len() != 64
            || hex::decode(&publication.tx_id)
                .map(|bytes| bytes.len() != 32)
                .unwrap_or(true)
        {
            return Err(NoteError::InvalidTransactionId);
        }
        let _guard = self.write_lock.lock().map_err(|_| {
            NoteError::StorageError("Note storage write lock is poisoned".to_string())
        })?;
        if let Some(existing) = self.pending_publication()? {
            return if existing == *publication {
                Ok(())
            } else {
                Err(NoteError::PublicationInProgress)
            };
        }
        self.schema_partition
            .insert(
                PENDING_PUBLICATION_KEY,
                Self::serialize_pending_publication(publication)?,
            )
            .map_err(|e| {
                NoteError::StorageOutcomeUnknown(format!(
                    "Pending publication write outcome is unknown: {}",
                    e
                ))
            })?;
        self.keyspace.persist(PersistMode::SyncData).map_err(|e| {
            NoteError::StorageOutcomeUnknown(format!(
                "Pending publication durability is unknown: {}",
                e
            ))
        })
    }

    pub(crate) fn clear_pending_publication(&self, tx_id: &str) -> Result<(), NoteError> {
        let _guard = self.write_lock.lock().map_err(|_| {
            NoteError::StorageError("Note storage write lock is poisoned".to_string())
        })?;
        let existing = self
            .pending_publication()?
            .ok_or(NoteError::PublicationLeaseMismatch)?;
        if !existing.tx_id.eq_ignore_ascii_case(tx_id) {
            return Err(NoteError::PublicationLeaseMismatch);
        }
        self.schema_partition
            .remove(PENDING_PUBLICATION_KEY)
            .map_err(|e| {
                NoteError::StorageOutcomeUnknown(format!(
                    "Pending publication removal outcome is unknown: {}",
                    e
                ))
            })?;
        self.keyspace.persist(PersistMode::SyncData).map_err(|e| {
            NoteError::StorageOutcomeUnknown(format!(
                "Pending publication removal durability is unknown: {}",
                e
            ))
        })
    }

    /// Store an IOU note with its issuer public key
    pub(crate) fn store_note(
        &self,
        issuer_pubkey: &PubKey,
        note: &IouNote,
        avl_root_digest: [u8; 33],
    ) -> Result<(), NoteError> {
        let _guard = self.write_lock.lock().map_err(|_| {
            NoteError::StorageError("Note storage write lock is poisoned".to_string())
        })?;
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey).to_bytes();
        let mut state = self.read_state_strict()?;
        if let Some((_, stored_note)) =
            state.notes.iter_mut().find(|(stored_issuer, stored_note)| {
                NoteKey::from_keys(stored_issuer, &stored_note.recipient_pubkey).to_bytes() == key
            })
        {
            *stored_note = note.clone();
        } else {
            if state.notes.len() >= self.capacity_limit() {
                return Err(NoteError::CapacityExceeded {
                    limit: self.capacity_limit(),
                });
            }
            state.notes.push((*issuer_pubkey, note.clone()));
        }
        state.avl_root_digest = avl_root_digest;
        let value_bytes = Self::serialize_note_state(&state)?;
        self.notes_partition
            .insert(NOTE_STATE_KEY, value_bytes.as_slice())
            .map_err(|e| {
                NoteError::StorageOutcomeUnknown(format!(
                    "Authoritative note write outcome is unknown; restart and reconcile: {}",
                    e
                ))
            })?;
        #[cfg(test)]
        if self.fail_next_persist.swap(false, Ordering::SeqCst) {
            return Err(NoteError::StorageOutcomeUnknown(
                "Injected durability outcome uncertainty".to_string(),
            ));
        }
        self.keyspace.persist(PersistMode::SyncData).map_err(|e| {
            NoteError::StorageOutcomeUnknown(format!(
                "Durable note write outcome is unknown; restart and reconcile: {}",
                e
            ))
        })?;

        Ok(())
    }

    pub(crate) fn ensure_capacity_for_validated_state(
        &self,
        note_count: usize,
        is_new_edge: bool,
    ) -> Result<(), NoteError> {
        if is_new_edge && note_count >= self.capacity_limit() {
            return Err(NoteError::CapacityExceeded {
                limit: self.capacity_limit(),
            });
        }
        Ok(())
    }

    pub(crate) fn validate_or_anchor_generation(
        &self,
        tracker_nft_id: &[u8; 32],
        observed_root: [u8; 33],
    ) -> Result<(), NoteError> {
        let _guard = self.write_lock.lock().map_err(|_| {
            NoteError::StorageError("Note storage write lock is poisoned".to_string())
        })?;
        let bytes = self
            .schema_partition
            .get(NOTE_GENERATION_KEY)
            .map_err(|e| NoteError::StorageError(format!("Failed to read generation: {}", e)))?
            .ok_or_else(|| {
                NoteError::GenerationBindingRequired(
                    "Tracker generation manifest is missing".to_string(),
                )
            })?;
        let mut generation = Self::deserialize_generation(bytes.as_ref())?;
        if &generation.tracker_nft_id != tracker_nft_id {
            return Err(NoteError::GenerationMismatch(
                "Observed tracker NFT does not match the persisted generation".to_string(),
            ));
        }
        if let Some(_anchor_root) = generation.anchor_root {
            return Ok(());
        }
        if observed_root != generation.bootstrap_root {
            return Err(NoteError::GenerationMismatch(
                "First observed on-chain root does not match the explicitly approved fresh generation root"
                    .to_string(),
            ));
        }

        generation.anchor_root = Some(observed_root);
        self.schema_partition
            .insert(NOTE_GENERATION_KEY, Self::serialize_generation(&generation))
            .map_err(|e| {
                NoteError::StorageOutcomeUnknown(format!(
                    "Tracker generation anchor outcome is unknown: {}",
                    e
                ))
            })?;
        self.keyspace.persist(PersistMode::SyncData).map_err(|e| {
            NoteError::StorageOutcomeUnknown(format!(
                "Tracker generation anchor durability is unknown: {}",
                e
            ))
        })
    }

    pub(crate) fn read_state_strict(&self) -> Result<StoredNoteState, NoteError> {
        let schema = self
            .schema_partition
            .get(NOTE_SCHEMA_KEY)
            .map_err(|e| NoteError::StorageError(format!("Failed to read note schema: {}", e)))?
            .ok_or_else(|| NoteError::StorageError("Note schema is missing".to_string()))?;
        if schema.as_ref() != NOTE_STATE_MAGIC {
            return Err(NoteError::StorageError(
                "Unsupported note storage schema".to_string(),
            ));
        }

        self.read_state_partition_strict()
    }

    fn read_state_partition_strict(&self) -> Result<StoredNoteState, NoteError> {
        let mut state = None;
        for item in self.notes_partition.iter() {
            let (stored_key, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate note partition: {}", e))
            })?;
            if stored_key.as_ref() != NOTE_STATE_KEY || state.is_some() {
                return Err(NoteError::StorageError(
                    "Unexpected row in authoritative note partition".to_string(),
                ));
            }
            state = Some(Self::deserialize_note_state(value_bytes.as_ref())?);
        }
        state.ok_or_else(|| {
            NoteError::StorageError("Authoritative note state is missing".to_string())
        })
    }

    #[cfg(test)]
    pub(crate) fn reverse_note_order_for_test(&self) -> Result<(), NoteError> {
        let mut state = self.read_state_strict()?;
        state.notes.reverse();
        self.notes_partition
            .insert(NOTE_STATE_KEY, Self::serialize_note_state(&state)?)
            .map_err(|e| NoteError::StorageError(e.to_string()))?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn note_row_count_for_test(&self) -> Result<usize, NoteError> {
        self.read_state_strict().map(|state| state.notes.len())
    }

    /// Retrieve an IOU note by issuer and recipient public keys
    pub fn get_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<Option<IouNote>, NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey).to_bytes();
        let state = self.read_state_strict()?;
        Ok(state.notes.into_iter().find_map(|(stored_issuer, note)| {
            (NoteKey::from_keys(&stored_issuer, &note.recipient_pubkey).to_bytes() == key)
                .then_some(note)
        }))
    }

    /// Persist a confirmation record for a note key.
    pub fn store_confirmation(
        &self,
        key_bytes: &[u8; 32],
        confirmation: &NoteConfirmation,
    ) -> Result<(), NoteError> {
        let _guard = self.write_lock.lock().map_err(|_| {
            NoteError::StorageError("Note storage write lock is poisoned".to_string())
        })?;
        let value = serde_json::to_vec(confirmation).map_err(|e| {
            NoteError::StorageError(format!("Failed to serialize confirmation: {}", e))
        })?;
        self.confirmations_partition
            .insert(key_bytes, &value)
            .map_err(|e| {
                NoteError::StorageOutcomeUnknown(format!(
                    "Confirmation write outcome is unknown; restart and reconcile: {}",
                    e
                ))
            })?;
        #[cfg(test)]
        if self.fail_next_persist.swap(false, Ordering::SeqCst) {
            return Err(NoteError::StorageOutcomeUnknown(
                "Injected confirmation durability outcome uncertainty".to_string(),
            ));
        }
        self.keyspace.persist(PersistMode::SyncData).map_err(|e| {
            NoteError::StorageOutcomeUnknown(format!(
                "Confirmation durability is unknown; restart and reconcile: {}",
                e
            ))
        })
    }

    /// Retrieve all confirmation records.
    pub fn get_all_confirmations(&self) -> Result<Vec<([u8; 32], NoteConfirmation)>, NoteError> {
        let mut results = Vec::new();
        for entry in self.confirmations_partition.iter() {
            let (key, value) = entry.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate confirmations: {}", e))
            })?;
            if key.len() != 32 {
                continue;
            }
            let mut key_bytes = [0u8; 32];
            key_bytes.copy_from_slice(&key);
            let confirmation: NoteConfirmation = serde_json::from_slice(&value).map_err(|e| {
                NoteError::StorageError(format!("Failed to parse confirmation: {}", e))
            })?;
            results.push((key_bytes, confirmation));
        }
        Ok(results)
    }

    /// Get all notes for a specific issuer from the strict primary snapshot.
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError> {
        self.read_state_strict().map(|state| {
            state
                .notes
                .into_iter()
                .filter_map(|(issuer, note)| (issuer == *issuer_pubkey).then_some(note))
                .collect()
        })
    }

    /// Read an issuer's liabilities from the primary note partition.
    ///
    /// The versioned primary note rows are the only liability authority.
    pub fn get_issuer_notes_strict(
        &self,
        issuer_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        let notes = self.get_issuer_notes(issuer_pubkey)?;
        for note in &notes {
            note.verify_signature(issuer_pubkey)
                .map_err(|_| NoteError::InvalidSignature)?;
            if note.amount_redeemed > note.amount_collected {
                return Err(NoteError::StorageError(
                    "Stored redeemed amount exceeds cumulative debt".to_string(),
                ));
            }
        }
        Ok(notes)
    }

    /// Read every primary note row without tolerating malformed or misplaced data.
    ///
    /// The versioned state vector is the insertion order. Malformed state or a
    /// duplicate logical edge stops recovery.
    pub(crate) fn get_all_notes_with_issuer_strict(
        &self,
    ) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.read_state_strict().map(|state| state.notes)
    }

    /// Get all notes for a specific recipient from the strict primary snapshot.
    pub fn get_recipient_notes(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        self.read_state_strict().map(|state| {
            state
                .notes
                .into_iter()
                .filter_map(|(_, note)| {
                    (note.recipient_pubkey == *recipient_pubkey).then_some(note)
                })
                .collect()
        })
    }

    /// Get all notes for a specific recipient with issuer information
    pub fn get_recipient_notes_with_issuer(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.read_state_strict().map(|state| {
            state
                .notes
                .into_iter()
                .filter_map(|(issuer, note)| {
                    (note.recipient_pubkey == *recipient_pubkey).then_some((issuer, note))
                })
                .collect()
        })
    }

    /// Get all notes in the database
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError> {
        self.read_state_strict()
            .map(|state| state.notes.into_iter().map(|(_, note)| note).collect())
    }

    /// Get all notes with issuer information
    pub fn get_all_notes_with_issuer(&self) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        self.get_all_notes_with_issuer_strict()
    }
}

impl ReserveStorage {
    /// Open or create a new reserve storage database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NoteError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| NoteError::StorageError(format!("Failed to open database: {}", e)))?;

        let partition = keyspace
            .open_partition("reserves", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open partition: {}", e)))?;

        Ok(Self { partition })
    }

    /// Store a reserve in the database
    pub fn store_reserve(&self, reserve: &ExtendedReserveInfo) -> Result<(), NoteError> {
        let key = reserve.box_id.as_bytes();
        let value = serde_json::to_vec(reserve)
            .map_err(|e| NoteError::StorageError(format!("Failed to serialize reserve: {}", e)))?;

        self.partition
            .insert(key, &value)
            .map_err(|e| NoteError::StorageError(format!("Failed to store reserve: {}", e)))?;

        Ok(())
    }

    /// Retrieve a reserve by box ID
    pub fn get_reserve(&self, box_id: &str) -> Result<Option<ExtendedReserveInfo>, NoteError> {
        match self.partition.get(box_id.as_bytes()) {
            Ok(Some(value_bytes)) => {
                let reserve: ExtendedReserveInfo =
                    serde_json::from_slice(&value_bytes).map_err(|e| {
                        NoteError::StorageError(format!("Failed to deserialize reserve: {}", e))
                    })?;
                Ok(Some(reserve))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to get reserve: {}",
                e
            ))),
        }
    }

    /// Get all reserves from the database
    pub fn get_all_reserves(&self) -> Result<Vec<ExtendedReserveInfo>, NoteError> {
        let mut reserves = Vec::new();

        for item in self.partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            let reserve: ExtendedReserveInfo =
                serde_json::from_slice(&value_bytes).map_err(|e| {
                    NoteError::StorageError(format!("Failed to deserialize reserve: {}", e))
                })?;

            reserves.push(reserve);
        }

        Ok(reserves)
    }

    /// Remove a reserve from the database
    pub fn remove_reserve(&self, box_id: &str) -> Result<(), NoteError> {
        self.partition
            .remove(box_id.as_bytes())
            .map_err(|e| NoteError::StorageError(format!("Failed to remove reserve: {}", e)))?;

        Ok(())
    }
}

impl TrackerStorage {
    /// Open or create a new tracker storage database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NoteError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| NoteError::StorageError(format!("Failed to open database: {}", e)))?;

        let partition = keyspace
            .open_partition("tracker_metadata", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open partition: {}", e)))?;

        Ok(Self { partition })
    }

    /// Store tracker box information in the database
    pub fn store_tracker_box(&self, tracker_box: &TrackerBoxInfo) -> Result<(), NoteError> {
        let key = tracker_box.box_id.as_bytes();
        let value = serde_json::to_vec(tracker_box).map_err(|e| {
            NoteError::StorageError(format!("Failed to serialize tracker box: {}", e))
        })?;

        self.partition
            .insert(key, &value)
            .map_err(|e| NoteError::StorageError(format!("Failed to store tracker box: {}", e)))?;

        Ok(())
    }

    /// Retrieve tracker box by box ID
    pub fn get_tracker_box(&self, box_id: &str) -> Result<Option<TrackerBoxInfo>, NoteError> {
        match self.partition.get(box_id.as_bytes()) {
            Ok(Some(value_bytes)) => {
                let tracker_box: TrackerBoxInfo =
                    serde_json::from_slice(&value_bytes).map_err(|e| {
                        NoteError::StorageError(format!("Failed to deserialize tracker box: {}", e))
                    })?;
                Ok(Some(tracker_box))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to get tracker box: {}",
                e
            ))),
        }
    }

    /// Get all tracker boxes from the database
    pub fn get_all_tracker_boxes(&self) -> Result<Vec<TrackerBoxInfo>, NoteError> {
        let mut tracker_boxes = Vec::new();

        for item in self.partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            let tracker_box: TrackerBoxInfo =
                serde_json::from_slice(&value_bytes).map_err(|e| {
                    NoteError::StorageError(format!("Failed to deserialize tracker box: {}", e))
                })?;

            tracker_boxes.push(tracker_box);
        }

        Ok(tracker_boxes)
    }

    /// Get the latest tracker box ID (highest last_verified_height)
    pub fn get_latest_tracker_box_id(&self) -> Result<Option<String>, NoteError> {
        let tracker_boxes = self.get_all_tracker_boxes()?;

        if tracker_boxes.is_empty() {
            return Ok(None);
        }

        // Find the box with the highest last_verified_height
        let latest_box = tracker_boxes
            .into_iter()
            .max_by_key(|b| b.last_verified_height);

        Ok(latest_box.map(|b| b.box_id))
    }

    /// Remove a tracker box from the database
    pub fn remove_tracker_box(&self, box_id: &str) -> Result<(), NoteError> {
        self.partition
            .remove(box_id.as_bytes())
            .map_err(|e| NoteError::StorageError(format!("Failed to remove tracker box: {}", e)))?;

        Ok(())
    }
}

impl AcceptancePolicyStorage {
    /// Open or create a new acceptance policy storage database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NoteError> {
        let keyspace = Config::new(path).open().map_err(|e| {
            NoteError::StorageError(format!("Failed to open policy database: {}", e))
        })?;

        let partition = keyspace
            .open_partition("acceptance_policies", PartitionCreateOptions::default())
            .map_err(|e| {
                NoteError::StorageError(format!("Failed to open policy partition: {}", e))
            })?;

        Ok(Self { partition })
    }

    /// Store a signed acceptance policy for a recipient
    ///
    /// Key: recipient_pubkey (33 bytes)
    /// Value: 8 bytes timestamp (u64 BE) + 4 bytes policy_json_len (u32 BE) + policy_json bytes + 4 bytes sig_len (u32 BE) + signature bytes
    pub fn store_policy(
        &self,
        recipient_pubkey: &PubKey,
        policy_json: &str,
        signature: &str,
    ) -> Result<(), NoteError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let policy_json_bytes = policy_json.as_bytes();
        let signature_bytes = signature.as_bytes();

        let mut value =
            Vec::with_capacity(8 + 4 + policy_json_bytes.len() + 4 + signature_bytes.len());
        value.extend_from_slice(&timestamp.to_be_bytes());
        value.extend_from_slice(&(policy_json_bytes.len() as u32).to_be_bytes());
        value.extend_from_slice(policy_json_bytes);
        value.extend_from_slice(&(signature_bytes.len() as u32).to_be_bytes());
        value.extend_from_slice(signature_bytes);

        self.partition
            .insert(recipient_pubkey, &value)
            .map_err(|e| NoteError::StorageError(format!("Failed to store policy: {}", e)))?;

        Ok(())
    }

    /// Retrieve a stored acceptance policy for a recipient
    pub fn get_policy(&self, recipient_pubkey: &PubKey) -> Result<Option<StoredPolicy>, NoteError> {
        match self.partition.get(recipient_pubkey) {
            Ok(Some(value_bytes)) => {
                if value_bytes.len() < 12 {
                    return Err(NoteError::StorageError(
                        "Invalid policy record format (too short)".to_string(),
                    ));
                }

                let mut offset = 0;
                let timestamp =
                    u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
                offset += 8;

                let policy_len =
                    u32::from_be_bytes(value_bytes[offset..offset + 4].try_into().unwrap())
                        as usize;
                offset += 4;

                if value_bytes.len() < offset + policy_len + 4 {
                    return Err(NoteError::StorageError(
                        "Invalid policy record format (policy length mismatch)".to_string(),
                    ));
                }

                let policy_json =
                    String::from_utf8(value_bytes[offset..offset + policy_len].to_vec()).map_err(
                        |e| NoteError::StorageError(format!("Invalid policy JSON encoding: {}", e)),
                    )?;
                offset += policy_len;

                let sig_len =
                    u32::from_be_bytes(value_bytes[offset..offset + 4].try_into().unwrap())
                        as usize;
                offset += 4;

                if value_bytes.len() < offset + sig_len {
                    return Err(NoteError::StorageError(
                        "Invalid policy record format (signature length mismatch)".to_string(),
                    ));
                }

                let signature = String::from_utf8(value_bytes[offset..offset + sig_len].to_vec())
                    .map_err(|e| {
                    NoteError::StorageError(format!("Invalid signature encoding: {}", e))
                })?;

                Ok(Some(StoredPolicy {
                    timestamp,
                    policy_json,
                    signature,
                }))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to get policy: {}",
                e
            ))),
        }
    }

    /// Remove a stored policy for a recipient
    pub fn remove_policy(&self, recipient_pubkey: &PubKey) -> Result<(), NoteError> {
        self.partition
            .remove(recipient_pubkey)
            .map_err(|e| NoteError::StorageError(format!("Failed to remove policy: {}", e)))?;
        Ok(())
    }

    /// List all stored policies with their recipient pubkeys
    ///
    /// Returns a vector of (recipient_pubkey_hex, timestamp) tuples
    pub fn list_policies(&self) -> Result<Vec<(String, u64)>, NoteError> {
        let mut policies = Vec::new();
        for item in self.partition.iter() {
            match item {
                Ok((key, value)) => {
                    if key.len() == 33 && value.len() >= 8 {
                        let pubkey_hex = hex::encode(&key);
                        let timestamp = u64::from_be_bytes(value[0..8].try_into().unwrap());
                        policies.push((pubkey_hex, timestamp));
                    }
                }
                Err(e) => {
                    tracing::warn!("Error iterating policies: {}", e);
                }
            }
        }
        Ok(policies)
    }
}
