//! Persistence layer for IouNote storage using fjall database

use crate::{IouNote, NoteError, NoteKey, PubKey};
use fjall::{Config, PartitionCreateOptions};
use std::path::Path;

/// Database storage for IOU notes
pub struct NoteStorage {
    partition: fjall::Partition,
}

impl NoteStorage {
    /// Open or create a new note storage database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NoteError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| NoteError::StorageError(format!("Failed to open database: {}", e)))?;

        let partition = keyspace
            .open_partition("iou_notes", PartitionCreateOptions::default())
            .map_err(|e| NoteError::StorageError(format!("Failed to open partition: {}", e)))?;

        Ok(Self { partition })
    }

    /// Store an IOU note with its issuer public key
    pub fn store_note(&self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let key_bytes = key.to_bytes();

        // Manual serialization to avoid serde issues with arrays
        let mut value_bytes = Vec::new();
        value_bytes.extend_from_slice(issuer_pubkey);
        value_bytes.extend_from_slice(&note.amount.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        self.partition
            .insert(&key_bytes, &value_bytes)
            .map_err(|e| NoteError::StorageError(format!("Failed to insert note: {}", e)))?;

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

        match self.partition.get(&key_bytes) {
            Ok(Some(value_bytes)) => {
                // Manual deserialization
                if value_bytes.len() != 33 + 8 + 8 + 65 + 33 {
                    return Err(NoteError::StorageError(
                        "Invalid stored note format".to_string(),
                    ));
                }

                let mut offset = 0;
                let _stored_issuer_pubkey: PubKey =
                    value_bytes[offset..offset + 33].try_into().unwrap();
                offset += 33;

                let amount =
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
                    amount,
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

    /// Remove an IOU note by issuer and recipient public keys
    pub fn remove_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<(), NoteError> {
        let key = NoteKey::from_keys(issuer_pubkey, recipient_pubkey);
        let key_bytes = key.to_bytes();

        self.partition
            .remove(&key_bytes)
            .map_err(|e| NoteError::StorageError(format!("Failed to remove note: {}", e)))?;

        Ok(())
    }

    /// Get all notes for a specific issuer
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError> {
        let mut notes = Vec::new();
        
        tracing::debug!("Looking for notes from issuer: {:?}", issuer_pubkey);
        tracing::debug!("Partition item count: {:?}", self.partition.iter().count());
        
        // Debug: log the actual issuer pubkey we're looking for
        tracing::debug!("Searching for issuer: {:?}", issuer_pubkey);

        for item in self.partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            // Manual deserialization
            if value_bytes.len() != 33 + 8 + 8 + 65 + 33 {
                continue; // Skip invalid entries
            }

            let stored_issuer_pubkey: PubKey = value_bytes[0..33].try_into().unwrap();

            if stored_issuer_pubkey == *issuer_pubkey {
                let amount = u64::from_be_bytes(value_bytes[33..41].try_into().unwrap());
                let timestamp = u64::from_be_bytes(value_bytes[41..49].try_into().unwrap());
                let signature: [u8; 65] = value_bytes[49..114].try_into().unwrap();
                let recipient_pubkey: PubKey = value_bytes[114..147].try_into().unwrap();

                let note = IouNote {
                    recipient_pubkey,
                    amount,
                    timestamp,
                    signature,
                };

                notes.push(note);
            }
        }

        Ok(notes)
    }

    /// Get all notes for a specific recipient
    pub fn get_recipient_notes(
        &self,
        recipient_pubkey: &PubKey,
    ) -> Result<Vec<IouNote>, NoteError> {
        let mut notes = Vec::new();

        for item in self.partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            // Manual deserialization
            if value_bytes.len() != 33 + 8 + 8 + 65 + 33 {
                continue; // Skip invalid entries
            }

            let note_recipient_pubkey: PubKey = value_bytes[114..147].try_into().unwrap();

            if note_recipient_pubkey == *recipient_pubkey {
                let amount = u64::from_be_bytes(value_bytes[33..41].try_into().unwrap());
                let timestamp = u64::from_be_bytes(value_bytes[41..49].try_into().unwrap());
                let signature: [u8; 65] = value_bytes[49..114].try_into().unwrap();

                let note = IouNote {
                    recipient_pubkey: note_recipient_pubkey,
                    amount,
                    timestamp,
                    signature,
                };

                notes.push(note);
            }
        }

        Ok(notes)
    }

    /// Get all notes in the database
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError> {
        let mut notes = Vec::new();

        for item in self.partition.iter() {
            let (_key_bytes, value_bytes) = item.map_err(|e| {
                NoteError::StorageError(format!("Failed to iterate partition: {}", e))
            })?;

            // Manual deserialization
            if value_bytes.len() != 33 + 8 + 8 + 65 + 33 {
                continue; // Skip invalid entries
            }

            let amount = u64::from_be_bytes(value_bytes[33..41].try_into().unwrap());
            let timestamp = u64::from_be_bytes(value_bytes[41..49].try_into().unwrap());
            let signature: [u8; 65] = value_bytes[49..114].try_into().unwrap();
            let recipient_pubkey: PubKey = value_bytes[114..147].try_into().unwrap();

            let note = IouNote {
                recipient_pubkey,
                amount,
                timestamp,
                signature,
            };

            notes.push(note);
        }

        Ok(notes)
    }
}
