//! Test helpers for Basis trees module

use crate::storage::TreeNode;
use crate::errors::TreeError;
use ergo_avltree_rust::batch_node::Node;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// In-memory node storage for testing
#[derive(Clone)]
pub struct InMemoryNodeStorage {
    nodes: Arc<Mutex<HashMap<Vec<u8>, TreeNode>>>,
}

impl InMemoryNodeStorage {
    /// Create a new in-memory node storage
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Store a node in memory
    pub fn store_node(&self, node: TreeNode) -> Result<(), TreeError> {
        let mut nodes = self.nodes.lock().unwrap();
        nodes.insert(node.digest.clone(), node);
        Ok(())
    }

    /// Retrieve a node by digest
    pub fn get_node(&self, digest: &[u8]) -> Result<Option<TreeNode>, TreeError> {
        let nodes = self.nodes.lock().unwrap();
        Ok(nodes.get(digest).cloned())
    }

    /// Get all stored nodes
    pub fn get_all_nodes(&self) -> Result<Vec<TreeNode>, TreeError> {
        let nodes = self.nodes.lock().unwrap();
        Ok(nodes.values().cloned().collect())
    }

    /// Clear all stored nodes
    pub fn clear(&self) -> Result<(), TreeError> {
        let mut nodes = self.nodes.lock().unwrap();
        nodes.clear();
        Ok(())
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        let nodes = self.nodes.lock().unwrap();
        nodes.len()
    }
}

/// In-memory resolver for testing
/// Note: This is a simplified placeholder implementation.
/// In a real implementation, this would properly convert TreeNode to ergo_avltree_rust::Node
pub fn in_memory_resolver(_storage: &InMemoryNodeStorage) -> impl Fn(&[u8; 32]) -> Node + '_ {
    move |_digest: &[u8; 32]| -> Node {
        // Placeholder implementation for testing
        // In a real implementation, we would:
        // 1. Fetch node from storage by digest
        // 2. Convert TreeNode to ergo_avltree_rust::Node
        // 3. Return the proper node structure
        
        // For now, we'll use the same approach as the current resolver
        panic!("In-memory resolver not fully implemented - needs proper node conversion");
    }
}

/// Test tree configuration
pub struct TestTreeConfig {
    pub key_size: usize,
    pub value_size: Option<usize>,
    pub enable_persistence: bool,
}

impl Default for TestTreeConfig {
    fn default() -> Self {
        Self {
            key_size: 64,
            value_size: None,
            enable_persistence: false,
        }
    }
}

/// Helper to create test note data
pub fn create_test_note_data(issuer_pubkey: &[u8], recipient_pubkey: &[u8], amount: u64, timestamp: u64) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(issuer_pubkey);
    data.extend_from_slice(recipient_pubkey);
    data.extend_from_slice(&amount.to_be_bytes());
    data.extend_from_slice(&timestamp.to_be_bytes());
    data
}

/// Helper to create test keys
pub fn create_test_key(issuer_hash: &[u8], recipient_hash: &[u8]) -> Vec<u8> {
    [issuer_hash, recipient_hash].concat()
}

/// Helper to generate test digests
pub fn create_test_digest(data: &[u8]) -> Vec<u8> {
    // Simple hash for testing - in production would use proper cryptographic hash
    let mut digest = vec![0u8; 32];
    for (i, &byte) in data.iter().enumerate() {
        digest[i % 32] ^= byte;
    }
    digest
}