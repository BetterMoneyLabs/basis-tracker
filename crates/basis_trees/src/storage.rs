//! Storage layer for AVL tree persistence

use crate::errors::TreeError;
// Note: Node type from ergo_avltree_rust not needed for storage layer
use fjall::{Config, Keyspace, Partition, PartitionCreateOptions};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    /// Optional: full tree serialization
    pub serialized_tree: Option<Vec<u8>>,
}

/// Main storage manager for tree persistence
pub struct TreeStorage {
    /// Keyspace for all tree-related data
    keyspace: Keyspace,
    /// Node storage partition
    node_partition: Partition,
    /// Operation log partition
    operation_partition: Partition,
    /// Checkpoint storage partition
    checkpoint_partition: Partition,
    /// Metadata partition
    metadata_partition: Partition,
    /// Current operation sequence number
    pub current_sequence: u64,
}

impl TreeStorage {
    /// Open or create a new tree storage database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, TreeError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| TreeError::StorageError(format!("Failed to open database: {}", e)))?;

        // Open partitions for different storage types
        let node_partition = keyspace
            .open_partition("tree_nodes", PartitionCreateOptions::default())
            .map_err(|e| TreeError::StorageError(format!("Failed to open node partition: {}", e)))?;

        let operation_partition = keyspace
            .open_partition("tree_operations", PartitionCreateOptions::default())
            .map_err(|e| TreeError::StorageError(format!("Failed to open operation partition: {}", e)))?;

        let checkpoint_partition = keyspace
            .open_partition("tree_checkpoints", PartitionCreateOptions::default())
            .map_err(|e| TreeError::StorageError(format!("Failed to open checkpoint partition: {}", e)))?;

        let metadata_partition = keyspace
            .open_partition("tree_metadata", PartitionCreateOptions::default())
            .map_err(|e| TreeError::StorageError(format!("Failed to open metadata partition: {}", e)))?;

        // Load current sequence number from metadata
        let current_sequence = Self::load_current_sequence(&metadata_partition)?;

        Ok(Self {
            keyspace,
            node_partition,
            operation_partition,
            checkpoint_partition,
            metadata_partition,
            current_sequence,
        })
    }

    /// Store a tree node
    pub fn store_node(&self, node: &TreeNode) -> Result<(), TreeError> {
        let key = Self::node_key(&node.digest);
        let value = bincode::serialize(node)
            .map_err(|e| TreeError::StorageError(format!("Failed to serialize node: {}", e)))?;

        self.node_partition
            .insert(&key, &value)
            .map_err(|e| TreeError::StorageError(format!("Failed to store node: {}", e)))?;

        Ok(())
    }

    /// Retrieve a tree node by digest
    pub fn get_node(&self, digest: &[u8]) -> Result<Option<TreeNode>, TreeError> {
        let key = Self::node_key(digest);

        match self.node_partition.get(&key) {
            Ok(Some(value_bytes)) => {
                let node = bincode::deserialize(&value_bytes)
                    .map_err(|e| TreeError::StorageError(format!("Failed to deserialize node: {}", e)))?;
                Ok(Some(node))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(TreeError::StorageError(format!("Failed to get node: {}", e))),
        }
    }

    /// Log a tree operation
    pub fn log_operation(&mut self, operation: TreeOperation) -> Result<(), TreeError> {
        let key = Self::operation_key(operation.sequence_number);
        let value = bincode::serialize(&operation)
            .map_err(|e| TreeError::StorageError(format!("Failed to serialize operation: {}", e)))?;

        self.operation_partition
            .insert(&key, &value)
            .map_err(|e| TreeError::StorageError(format!("Failed to store operation: {}", e)))?;

        // Update current sequence number
        self.current_sequence = operation.sequence_number;
        self.store_current_sequence()?;

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
            let key = Self::operation_key(seq);
            if let Some(value_bytes) = self.operation_partition.get(&key)
                .map_err(|e| TreeError::StorageError(format!("Failed to get operation: {}", e)))? {
                let operation = bincode::deserialize(&value_bytes)
                    .map_err(|e| TreeError::StorageError(format!("Failed to deserialize operation: {}", e)))?;
                operations.push(operation);
            }
        }

        Ok(operations)
    }

    /// Store a checkpoint
    pub fn store_checkpoint(&self, checkpoint: &TreeCheckpoint) -> Result<(), TreeError> {
        let key = Self::checkpoint_key(checkpoint.checkpoint_id);
        let value = bincode::serialize(checkpoint)
            .map_err(|e| TreeError::StorageError(format!("Failed to serialize checkpoint: {}", e)))?;

        self.checkpoint_partition
            .insert(&key, &value)
            .map_err(|e| TreeError::StorageError(format!("Failed to store checkpoint: {}", e)))?;

        Ok(())
    }

    /// Get latest checkpoint
    pub fn get_latest_checkpoint(&self) -> Result<Option<TreeCheckpoint>, TreeError> {
        // This is a simplified implementation - in production we'd need proper ordering
        let mut latest_checkpoint: Option<TreeCheckpoint> = None;

        for item in self.checkpoint_partition.iter() {
            let (_key_bytes, value_bytes) = item
                .map_err(|e| TreeError::StorageError(format!("Failed to iterate checkpoints: {}", e)))?;

            let checkpoint: TreeCheckpoint = bincode::deserialize(&value_bytes)
                .map_err(|e| TreeError::StorageError(format!("Failed to deserialize checkpoint: {}", e)))?;

            if latest_checkpoint.as_ref().map_or(true, |c| checkpoint.checkpoint_id > c.checkpoint_id) {
                latest_checkpoint = Some(checkpoint);
            }
        }

        Ok(latest_checkpoint)
    }

    /// Get all nodes in storage
    pub fn get_all_nodes(&self) -> Result<Vec<TreeNode>, TreeError> {
        let mut nodes = Vec::new();

        for item in self.node_partition.iter() {
            let (_key_bytes, value_bytes) = item
                .map_err(|e| TreeError::StorageError(format!("Failed to iterate nodes: {}", e)))?;

            let node: TreeNode = bincode::deserialize(&value_bytes)
                .map_err(|e| TreeError::StorageError(format!("Failed to deserialize node: {}", e)))?;

            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Helper: Generate node storage key
    fn node_key(digest: &[u8]) -> Vec<u8> {
        let mut key = b"nodes/".to_vec();
        key.extend_from_slice(digest);
        key
    }

    /// Helper: Generate operation storage key
    fn operation_key(sequence_number: u64) -> Vec<u8> {
        let mut key = b"operations/".to_vec();
        key.extend_from_slice(&sequence_number.to_be_bytes());
        key
    }

    /// Helper: Generate checkpoint storage key
    fn checkpoint_key(checkpoint_id: u64) -> Vec<u8> {
        let mut key = b"checkpoints/".to_vec();
        key.extend_from_slice(&checkpoint_id.to_be_bytes());
        key
    }

    /// Load current sequence number from metadata
    fn load_current_sequence(metadata_partition: &Partition) -> Result<u64, TreeError> {
        match metadata_partition.get(b"current_sequence") {
            Ok(Some(value_bytes)) => {
                if value_bytes.len() == 8 {
                    Ok(u64::from_be_bytes(value_bytes[0..8].try_into().unwrap()))
                } else {
                    Ok(0) // Default to 0 if invalid
                }
            }
            Ok(None) => Ok(0), // Start from 0 if not found
            Err(e) => Err(TreeError::StorageError(format!("Failed to get current sequence: {}", e))),
        }
    }

    /// Store current sequence number to metadata
    fn store_current_sequence(&self) -> Result<(), TreeError> {
        let value_bytes = self.current_sequence.to_be_bytes().to_vec();
        self.metadata_partition
            .insert(b"current_sequence", &value_bytes)
            .map_err(|e| TreeError::StorageError(format!("Failed to store current sequence: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tree_storage_creation() {
        let temp_dir = tempdir().unwrap();
        let storage = TreeStorage::open(temp_dir.path()).unwrap();
        
        // Should be able to create storage without errors
        assert!(true, "Storage creation should succeed");
    }

    #[test]
    fn test_node_storage() {
        let temp_dir = tempdir().unwrap();
        let storage = TreeStorage::open(temp_dir.path()).unwrap();

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
        let temp_dir = tempdir().unwrap();
        let mut storage = TreeStorage::open(temp_dir.path()).unwrap();

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
        let temp_dir = tempdir().unwrap();
        let storage = TreeStorage::open(temp_dir.path()).unwrap();

        let checkpoint = TreeCheckpoint {
            checkpoint_id: 1,
            timestamp: 1234567890,
            tree_root: vec![1u8; 33],
            operation_sequence: 100,
            node_count: 50,
            serialized_tree: None,
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
        let temp_dir = tempdir().unwrap();
        let mut storage = TreeStorage::open(temp_dir.path()).unwrap();

        let seq1 = storage.next_sequence_number();
        let seq2 = storage.next_sequence_number();
        let seq3 = storage.next_sequence_number();

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert_eq!(seq3, 3);
    }
}