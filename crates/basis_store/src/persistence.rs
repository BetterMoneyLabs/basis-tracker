//! Persistence layer for IouNote storage using fjall database with extra indices
//!
//! This module provides efficient storage and retrieval of IOU notes with secondary indices
//! for fast lookups by issuer, recipient, and timestamp without full partition scans.

use crate::{reserve_tracker::ExtendedReserveInfo, IouNote, NoteError, NoteKey, PubKey, TrackerBoxInfo};
use fjall::{Config, PartitionCreateOptions};
use std::path::Path;

/// Database storage for IOU notes with extra indices for efficient querying
///
/// Uses three partitions:
/// - `iou_notes`: Main data storage (issuer+recipient -> note data)
/// - `issuer_index`: Secondary index (issuer_pubkey -> list of note keys)
/// - `recipient_index`: Secondary index (recipient_pubkey -> list of note keys)
pub struct NoteStorage {
    notes_partition: fjall::Partition,
    issuer_index: fjall::Partition,
    recipient_index: fjall::Partition,
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
            .map_err(|e| NoteError::StorageError(format!("Failed to store blockchain height: {}", e)))?;
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
    /// Open or create a new note storage database with extra indices
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NoteError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| NoteError::StorageError(format!("Failed to open database: {}", e)))?;

        let notes_partition = keyspace
            .open_partition("iou_notes", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open notes partition: {}", e)))?;

        let issuer_index = keyspace
            .open_partition("issuer_index", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open issuer index partition: {}", e)))?;

        let recipient_index = keyspace
            .open_partition("recipient_index", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open recipient index partition: {}", e)))?;

        Ok(Self { notes_partition, issuer_index, recipient_index })
    }

    /// Serialize a list of note keys to bytes
    fn serialize_note_keys(keys: &[NoteKey]) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Store count as u32
        bytes.extend_from_slice(&(keys.len() as u32).to_be_bytes());
        // Store each key (66 bytes)
        for key in keys {
            bytes.extend_from_slice(&key.to_bytes());
        }
        bytes
    }

    /// Deserialize a list of note keys from bytes
    fn deserialize_note_keys(bytes: &[u8]) -> Result<Vec<NoteKey>, NoteError> {
        if bytes.len() < 4 {
            return Ok(Vec::new());
        }
        let count = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let mut keys = Vec::with_capacity(count);
        let expected_len = 4 + count * 32; // NoteKey is 32 bytes (blake2b hash)
        if bytes.len() < expected_len {
            return Err(NoteError::StorageError("Invalid note key list format".to_string()));
        }
        let mut offset = 4;
        for _ in 0..count {
            let key_bytes: [u8; 32] = bytes[offset..offset + 32].try_into().unwrap();
            keys.push(NoteKey::from_bytes(&key_bytes));
            offset += 32;
        }
        Ok(keys)
    }

    /// Add a note key to an index partition
    fn add_to_index(
        index: &fjall::Partition,
        pubkey: &PubKey,
        note_key: &NoteKey,
    ) -> Result<(), NoteError> {
        let pubkey_bytes = pubkey;
        let existing = index.get(pubkey_bytes).map_err(|e| {
            NoteError::StorageError(format!("Failed to read index: {}", e))
        })?;

        let mut keys = match existing {
            Some(bytes) => Self::deserialize_note_keys(&bytes)?,
            None => Vec::new(),
        };

        // Check if key already exists to avoid duplicates
        let key_bytes = note_key.to_bytes();
        if !keys.iter().any(|k| k.to_bytes() == key_bytes) {
            keys.push(note_key.clone());
            let serialized = Self::serialize_note_keys(&keys);
            index.insert(pubkey_bytes, &serialized).map_err(|e| {
                NoteError::StorageError(format!("Failed to update index: {}", e))
            })?;
        }

        Ok(())
    }

    /// Remove a note key from an index partition
    fn remove_from_index(
        index: &fjall::Partition,
        pubkey: &PubKey,
        note_key: &NoteKey,
    ) -> Result<(), NoteError> {
        let pubkey_bytes = pubkey;
        let existing = index.get(pubkey_bytes).map_err(|e| {
            NoteError::StorageError(format!("Failed to read index: {}", e))
        })?;

        let mut keys = match existing {
            Some(bytes) => Self::deserialize_note_keys(&bytes)?,
            None => return Ok(()),
        };

        let key_bytes = note_key.to_bytes();
        keys.retain(|k| k.to_bytes() != key_bytes);

        if keys.is_empty() {
            index.remove(pubkey_bytes).map_err(|e| {
                NoteError::StorageError(format!("Failed to remove index entry: {}", e))
            })?;
        } else {
            let serialized = Self::serialize_note_keys(&keys);
            index.insert(pubkey_bytes, &serialized).map_err(|e| {
                NoteError::StorageError(format!("Failed to update index: {}", e))
            })?;
        }

        Ok(())
    }

    /// Store an IOU note with its issuer public key
    pub fn store_note(&self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Manual serialization to avoid serde issues with arrays
        let mut value_bytes = Vec::new();
        value_bytes.extend_from_slice(issuer_pubkey);
        value_bytes.extend_from_slice(&note.amount_collected.to_be_bytes());
        value_bytes.extend_from_slice(&note.amount_redeemed.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        self.notes_partition
            .insert(&key_bytes, &value_bytes)
            .map_err(|e| NoteError::StorageError(format!("Failed to insert note: {}", e)))?;

        // Update indices for efficient querying
        Self::add_to_index(&self.issuer_index, issuer_pubkey, &key)?;
        Self::add_to_index(&self.recipient_index, &note.recipient_pubkey, &key)?;

        Ok(())
    }

    /// Retrieve an IOU note by issuer and recipient public keys
    pub fn get_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<Option<IouNote>, NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        match self.notes_partition.get(&key_bytes) {
            Ok(Some(value_bytes)) => {
                // Manual deserialization
                if value_bytes.len() != 33 + 8 + 8 + 8 + 65 + 33 {
                    return Err(NoteError::StorageError(
                        "Invalid stored note format".to_string(),
                    ));
                }

                let mut offset = 0;
                let _stored_issuer_pubkey: PubKey =
                    value_bytes[offset..offset + 33].try_into().unwrap();
                offset += 33;

                let amount_collected =
                    u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
                offset += 8;

                let amount_redeemed =
                    u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
                offset += 8;

                let timestamp =
                    u64::from_be_bytes(value_bytes[offset..offset + 8].try_into().unwrap());
                offset += 8;

                let signature: [u8; 65] = value_bytes[offset..offset + 65].try_into().unwrap();
                offset += 65;

                let recipient_pubkey: PubKey = value_bytes[offset..offset + 33].try_into().unwrap();

                let note = IouNote {
                    recipient_pubkey,
                    amount_collected,
                    amount_redeemed,
                    timestamp,
                    signature,
                };

                Ok(Some(note))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to get note: {}",
                e
            ))),
        }
    }

    /// Retrieve notes by their keys using the main partition
    fn get_notes_by_keys(&self, keys: &[NoteKey]) -> Result<Vec<IouNote>, NoteError> {
        let mut notes = Vec::new();
        for key in keys {
            let key_bytes = key.to_bytes();
            match self.notes_partition.get(&key_bytes) {
                Ok(Some(value_bytes)) => {
                    if value_bytes.len() != 33 + 8 + 8 + 8 + 65 + 33 {
                        continue; // Skip invalid entries
                    }
                    let amount_collected = u64::from_be_bytes(value_bytes[33..41].try_into().unwrap());
                    let amount_redeemed = u64::from_be_bytes(value_bytes[41..49].try_into().unwrap());
                    let timestamp = u64::from_be_bytes(value_bytes[49..57].try_into().unwrap());
                    let signature: [u8; 65] = value_bytes[57..122].try_into().unwrap();
                    let recipient_pubkey: PubKey = value_bytes[122..155].try_into().unwrap();

                    notes.push(IouNote {
                        recipient_pubkey,
                        amount_collected,
                        amount_redeemed,
                        timestamp,
                        signature,
                    });
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
        Ok(notes)
    }

    /// Get all notes for a specific issuer (uses issuer index for O(1) lookup)
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError> {
        tracing::debug!("Looking for notes from issuer using index: {:?}", issuer_pubkey);

        // Use the issuer index for efficient lookup
        match self.issuer_index.get(issuer_pubkey) {
            Ok(Some(bytes)) => {
                let keys = Self::deserialize_note_keys(&bytes)?;
                tracing::debug!("Found {} note keys in issuer index", keys.len());
                self.get_notes_by_keys(&keys)
            }
            Ok(None) => {
                tracing::debug!("No notes found in issuer index");
                Ok(Vec::new())
            }
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to read issuer index: {}",
                e
            ))),
        }
    }

    /// Get all notes for a specific recipient (uses recipient index for O(1) lookup)
    pub fn get_recipient_notes(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        tracing::debug!("Looking for notes for recipient using index: {:?}", recipient_pubkey);

        // Use the recipient index for efficient lookup
        match self.recipient_index.get(recipient_pubkey) {
            Ok(Some(bytes)) => {
                let keys = Self::deserialize_note_keys(&bytes)?;
                tracing::debug!("Found {} note keys in recipient index", keys.len());
                self.get_notes_by_keys(&keys)
            }
            Ok(None) => {
                tracing::debug!("No notes found in recipient index");
                Ok(Vec::new())
            }
            Err(e) => Err(NoteError::StorageError(format!(
                "Failed to read recipient index: {}",
                e
            ))),
        }
    }

    /// Rebuild secondary indices from existing notes in the database
    /// This should be called after upgrading to a version with indices when
    /// existing data may not have index entries
    pub fn rebuild_indices(&self) -> Result<usize, NoteError> {
        tracing::info!("Rebuilding note indices from existing data...");
        let mut count = 0;

        for item in self.notes_partition.iter() {
            let (key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            // Manual deserialization to extract issuer and recipient
            if value_bytes.len() != 33 + 8 + 8 + 8 + 65 + 33 {
                continue; // Skip invalid entries
            }

            let issuer_pubkey: PubKey = value_bytes[0..33].try_into().unwrap();
            let recipient_pubkey: PubKey = value_bytes[122..155].try_into().unwrap();

            // Reconstruct the note key from the stored key bytes
            let note_key = if key_bytes.len() == 32 {
                NoteKey::from_bytes(&key_bytes.as_ref().try_into().unwrap())
            } else {
                // Fallback: compute from pubkeys
                NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey)
            };

            // Rebuild indices
            Self::add_to_index(&self.issuer_index, &issuer_pubkey, &note_key)?;
            Self::add_to_index(&self.recipient_index, &recipient_pubkey, &note_key)?;
            count += 1;
        }

        tracing::info!("Index rebuild complete: {} notes indexed", count);
        Ok(count)
    }

    /// Get all notes in the database
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError> {
        let mut notes = Vec::new();

        for item in self.notes_partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            // Manual deserialization
            if value_bytes.len() != 33 + 8 + 8 + 8 + 65 + 33 {
                continue; // Skip invalid entries
            }

            let _stored_issuer_pubkey: PubKey = value_bytes[0..33].try_into().unwrap();
            let amount_collected = u64::from_be_bytes(value_bytes[33..41].try_into().unwrap());
            let amount_redeemed = u64::from_be_bytes(value_bytes[41..49].try_into().unwrap());
            let timestamp = u64::from_be_bytes(value_bytes[49..57].try_into().unwrap());
            let signature: [u8; 65] = value_bytes[57..122].try_into().unwrap();
            let recipient_pubkey: PubKey = value_bytes[122..155].try_into().unwrap();

            let note = IouNote {
                recipient_pubkey,
                amount_collected,
                amount_redeemed,
                timestamp,
                signature,
            };

            notes.push(note);
        }

        Ok(notes)
    }

    /// Get all notes with issuer information
    pub fn get_all_notes_with_issuer(&self) -> Result<Vec<(PubKey, IouNote)>, NoteError> {
        let mut notes_with_issuer = Vec::new();

        for item in self.notes_partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            // Manual deserialization
            if value_bytes.len() != 33 + 8 + 8 + 8 + 65 + 33 {
                continue; // Skip invalid entries
            }

            let issuer_pubkey: PubKey = value_bytes[0..33].try_into().unwrap();
            let amount_collected = u64::from_be_bytes(value_bytes[33..41].try_into().unwrap());
            let amount_redeemed = u64::from_be_bytes(value_bytes[41..49].try_into().unwrap());
            let timestamp = u64::from_be_bytes(value_bytes[49..57].try_into().unwrap());
            let signature: [u8; 65] = value_bytes[57..122].try_into().unwrap();
            let recipient_pubkey: PubKey = value_bytes[122..155].try_into().unwrap();

            let note = IouNote {
                recipient_pubkey,
                amount_collected,
                amount_redeemed,
                timestamp,
                signature,
            };

            notes_with_issuer.push((issuer_pubkey, note));
        }

        Ok(notes_with_issuer)
    }

    /// Delete a note and update indices
    pub fn delete_note(&self, issuer_pubkey: &PubKey, recipient_pubkey: &PubKey) -> Result<(), NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Remove from main storage
        self.notes_partition
            .remove(&key_bytes)
            .map_err(|e| NoteError::StorageError(format!("Failed to remove note: {}", e)))?;

        // Update indices
        Self::remove_from_index(&self.issuer_index, issuer_pubkey, &key)?;
        Self::remove_from_index(&self.recipient_index, recipient_pubkey, &key)?;

        Ok(())
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
        let value = serde_json::to_vec(tracker_box)
            .map_err(|e| NoteError::StorageError(format!("Failed to serialize tracker box: {}", e)))?;

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
