//! Simple in-memory storage layer for AVL tree
//! 
//! Since Fjall persistence doesn't work well with AVL+ trees due to resolver limitations,
//! this provides a simple in-memory storage implementation.

use crate::errors::TreeError;
use serde::{Deserialize, Serialize};

/// Tree node storage structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    /// Node digest (32 bytes)
    pub digest: Vec<u8>,
    /// Node type
    pub node_type: NodeType,
    /// Key (for leaf nodes)
    pub key: Option<Vec<u8>>,
    /// Value (for leaf nodes)
    pub value: Option<Vec<u8>>,
    /// Left child digest
    pub left_digest: Option<Vec<u8>>,
    /// Right child digest
    pub right_digest: Option<Vec<u8>>,
    /// Node height
    pub height: u8,
}

/// Node type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Leaf,
    Branch,
}

/// Tree operation for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeOperation {
    /// Monotonically increasing sequence number
    pub sequence_number: u64,
    /// Operation type
    pub operation_type: OperationType,
    /// Operation timestamp
    pub timestamp: u64,
    /// Note key
    pub key: Vec<u8>,
    /// Note value (for insert/update)
    pub value: Vec<u8>,
    /// Previous value (for updates)
    pub previous_value: Option<Vec<u8>>,
    /// Tree state before operation
    pub tree_root_before: Vec<u8>,
    /// Tree state after operation
    pub tree_root_after: Vec<u8>,
}

/// Operation type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationType {
    Insert,
    Update,
}

/// Tree checkpoint for periodic snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeCheckpoint {
    /// Unique identifier
    pub checkpoint_id: u64,
    /// Checkpoint creation time
    pub timestamp: u64,
    /// Tree state at checkpoint
    pub tree_root: Vec<u8>,
    /// Last included operation sequence
    pub operation_sequence: u64,
    /// Total nodes in tree
    pub node_count: u64,
}

/// Simple in-memory storage manager
pub struct TreeStorage {
    /// In-memory node storage
    nodes: std::collections::HashMap<Vec<u8>, TreeNode>,
    /// In-memory operation log
    operations: std::collections::HashMap<u64, TreeOperation>,
    /// In-memory checkpoint storage
    checkpoints: std::collections::HashMap<u64, TreeCheckpoint>,
    /// Current operation sequence number
    pub current_sequence: u64,
}

impl TreeStorage {
    /// Create a new in-memory tree storage
    pub fn new() -> Self {
        Self {
            nodes: std::collections::HashMap::new(),
            operations: std::collections::HashMap::new(),
            checkpoints: std::collections::HashMap::new(),
            current_sequence: 0,
        }
    }

    /// Store a tree node
    pub fn store_node(&mut self, node: &TreeNode) -> Result<(), TreeError> {
        self.nodes.insert(node.digest.clone(), node.clone());
        Ok(())
    }

    /// Retrieve a tree node by digest
    pub fn get_node(&self, digest: &[u8]) -> Result<Option<TreeNode>, TreeError> {
        Ok(self.nodes.get(digest).cloned())
    }

    /// Log a tree operation
    pub fn log_operation(&mut self, operation: TreeOperation) -> Result<(), TreeError> {
        self.operations.insert(operation.sequence_number, operation.clone());
        self.current_sequence = operation.sequence_number;
        Ok(())
    }

    /// Get next operation sequence number
    pub fn next_sequence_number(&mut self) -> u64 {
        self.current_sequence += 1;
        self.current_sequence
    }

    /// Get operations in sequence range
    pub fn get_operations(&self, start: u64, end: u64) -> Result<Vec<TreeOperation>, TreeError> {
        let mut operations = Vec::new();
        
        for seq in start..=end {
            if let Some(operation) = self.operations.get(&seq) {
                operations.push(operation.clone());
            }
        }
        
        Ok(operations)
    }

    /// Store a checkpoint
    pub fn store_checkpoint(&mut self, checkpoint: &TreeCheckpoint) -> Result<(), TreeError> {
        self.checkpoints.insert(checkpoint.checkpoint_id, checkpoint.clone());
        Ok(())
    }

    /// Get latest checkpoint
    pub fn get_latest_checkpoint(&self) -> Result<Option<TreeCheckpoint>, TreeError> {
        let latest_id = self.checkpoints.keys().max().copied();
        Ok(latest_id.and_then(|id| self.checkpoints.get(&id).cloned()))
    }

    /// Get all nodes in storage
    pub fn get_all_nodes(&self) -> Result<Vec<TreeNode>, TreeError> {
        Ok(self.nodes.values().cloned().collect())
    }

    /// Batch store multiple nodes
    pub fn batch_store_nodes(&mut self, nodes: &[TreeNode]) -> Result<(), TreeError> {
        for node in nodes {
            self.store_node(node)?;
        }
        Ok(())
    }

    /// Delete a node by digest
    pub fn delete_node(&mut self, digest: &[u8]) -> Result<(), TreeError> {
        self.nodes.remove(digest);
        Ok(())
    }

    /// Batch delete multiple nodes
    pub fn batch_delete_nodes(&mut self, digests: &[Vec<u8>]) -> Result<(), TreeError> {
        for digest in digests {
            self.delete_node(digest)?;
        }
        Ok(())
    }

    /// Get nodes by digest range
    pub fn get_nodes_by_digest_range(
        &self,
        start_digest: &[u8],
        end_digest: &[u8],
    ) -> Result<Vec<TreeNode>, TreeError> {
        let mut nodes = Vec::new();
        
        for (digest, node) in &self.nodes {
            if digest.as_slice() >= start_digest && digest.as_slice() <= end_digest {
                nodes.push(node.clone());
            }
        }
        
        nodes.sort_by(|a, b| a.digest.cmp(&b.digest));
        Ok(nodes)
    }
}

impl Default for TreeStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_storage_creation() {
        let storage = TreeStorage::new();
        
        // Should be able to create storage without errors
        assert_eq!(storage.current_sequence, 0);
    }

    #[test]
    fn test_node_storage() {
        let mut storage = TreeStorage::new();

        let node = TreeNode {
            digest: vec![1u8; 32],
            node_type: NodeType::Leaf,
            key: Some(vec![2u8; 64]),
            value: Some(vec![3u8; 100]),
            left_digest: None,
            right_digest: None,
            height: 1,
        };

        // Store node
        storage.store_node(&node).unwrap();

        // Retrieve node
        let retrieved = storage.get_node(&[1u8; 32]).unwrap().unwrap();
        assert_eq!(retrieved.digest, node.digest);
        assert_eq!(retrieved.node_type, node.node_type);
        assert_eq!(retrieved.key, node.key);
        assert_eq!(retrieved.value, node.value);
        assert_eq!(retrieved.height, node.height);
    }

    #[test]
    fn test_operation_logging() {
        let mut storage = TreeStorage::new();

        let operation = TreeOperation {
            sequence_number: storage.next_sequence_number(),
            operation_type: OperationType::Insert,
            timestamp: 1234567890,
            key: vec![1u8; 64],
            value: vec![2u8; 100],
            previous_value: None,
            tree_root_before: vec![0u8; 33],
            tree_root_after: vec![1u8; 33],
        };

        // Log operation
        storage.log_operation(operation.clone()).unwrap();

        // Retrieve operation
        let operations = storage.get_operations(1, 1).unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].sequence_number, operation.sequence_number);
        assert_eq!(operations[0].operation_type, operation.operation_type);
    }

    #[test]
    fn test_checkpoint_storage() {
        let mut storage = TreeStorage::new();

        let checkpoint = TreeCheckpoint {
            checkpoint_id: 1,
            timestamp: 1234567890,
            tree_root: vec![1u8; 33],
            operation_sequence: 100,
            node_count: 50,
        };

        // Store checkpoint
        storage.store_checkpoint(&checkpoint).unwrap();

        // Retrieve latest checkpoint
        let retrieved = storage.get_latest_checkpoint().unwrap().unwrap();
        assert_eq!(retrieved.checkpoint_id, checkpoint.checkpoint_id);
        assert_eq!(retrieved.timestamp, checkpoint.timestamp);
        assert_eq!(retrieved.tree_root, checkpoint.tree_root);
        assert_eq!(retrieved.operation_sequence, checkpoint.operation_sequence);
        assert_eq!(retrieved.node_count, checkpoint.node_count);
    }

    #[test]
    fn test_sequence_number_increment() {
        let mut storage = TreeStorage::new();

        let seq1 = storage.next_sequence_number();
        let seq2 = storage.next_sequence_number();
        let seq3 = storage.next_sequence_number();

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert_eq!(seq3, 3);
    }
}