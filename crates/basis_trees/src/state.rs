//! State commitment structures for Basis tracker

use serde::{Deserialize, Serialize};

/// Tracker state commitment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerState {
    /// AVL tree root digest (32 bytes hash + 1 byte height)
    pub avl_root_digest: Vec<u8>,
    /// Block height of last on-chain commitment
    pub last_commit_height: u64,
    /// Timestamp of last state update
    pub last_update_timestamp: u64,
}

impl TrackerState {
    /// Create a new tracker state
    pub fn new(root_digest: [u8; 33], commit_height: u64, timestamp: u64) -> Self {
        Self {
            avl_root_digest: root_digest.to_vec(),
            last_commit_height: commit_height,
            last_update_timestamp: timestamp,
        }
    }

    /// Create an empty state
    pub fn empty() -> Self {
        Self {
            avl_root_digest: vec![0u8; 33],
            last_commit_height: 0,
            last_update_timestamp: 0,
        }
    }

    /// Check if state is empty (no commitments yet)
    pub fn is_empty(&self) -> bool {
        self.avl_root_digest == vec![0u8; 33]
    }

    /// Serialize state to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.avl_root_digest);
        bytes.extend_from_slice(&self.last_commit_height.to_be_bytes());
        bytes.extend_from_slice(&self.last_update_timestamp.to_be_bytes());
        bytes
    }

    /// Deserialize state from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 33 + 8 + 8 {
            return None;
        }

        let avl_root_digest = data[0..33].to_vec();

        let last_commit_height = u64::from_be_bytes(data[33..41].try_into().ok()?);
        let last_update_timestamp = u64::from_be_bytes(data[41..49].try_into().ok()?);

        Some(Self {
            avl_root_digest,
            last_commit_height,
            last_update_timestamp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_state_creation() {
        let root = [1u8; 33];
        let state = TrackerState::new(root, 1000, 1234567890);
        
        assert_eq!(state.avl_root_digest, root);
        assert_eq!(state.last_commit_height, 1000);
        assert_eq!(state.last_update_timestamp, 1234567890);
        assert!(!state.is_empty());
    }

    #[test]
    fn test_empty_state() {
        let state = TrackerState::empty();
        assert!(state.is_empty());
        assert_eq!(state.last_commit_height, 0);
        assert_eq!(state.last_update_timestamp, 0);
    }

    #[test]
    fn test_serialization() {
        let state = TrackerState::new([42u8; 33], 999, 987654321);
        let bytes = state.to_bytes();
        let restored = TrackerState::from_bytes(&bytes).unwrap();
        
        assert_eq!(state, restored);
    }

    #[test]
    fn test_invalid_deserialization() {
        let short_data = vec![1u8; 10];
        assert!(TrackerState::from_bytes(&short_data).is_none());
    }
}