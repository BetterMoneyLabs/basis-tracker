//! Comprehensive tests for Fjall-based tree storage

use super::fjall_storage::{FjallTreeStorage, FjallStorageConfig};
use crate::storage::{TreeNode, TreeOperation, TreeCheckpoint, NodeType, OperationType};
use crate::errors::TreeError;
use tempfile::tempdir;

/// Test basic storage creation
#[test]
fn test_fjall_storage_creation() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let _storage = FjallTreeStorage::open(temp_dir.path())?;
    
    // Should be able to create storage without errors
    assert!(true, "Fjall storage creation should succeed");
    
    Ok(())
}

/// Test storage with custom configuration
#[test]
fn test_fjall_storage_with_config() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let config = FjallStorageConfig {
        max_partition_size: 512 * 1024 * 1024, // 512MB
        batch_size: 500,
        compression: false,
    };
    
    let _storage = FjallTreeStorage::open_with_config(temp_dir.path(), config)?;
    
    // Should be able to create storage with custom config
    assert!(true, "Fjall storage with custom config should succeed");
    
    Ok(())
}

/// Test node storage operations
#[test]
fn test_fjall_node_storage() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

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
    storage.store_node(&node)?;

    // Retrieve node
    let retrieved = storage.get_node(&[1u8; 32])?.unwrap();
    assert_eq!(retrieved.digest, node.digest);
    assert_eq!(retrieved.node_type, node.node_type);
    assert_eq!(retrieved.key, node.key);
    assert_eq!(retrieved.value, node.value);
    assert_eq!(retrieved.height, node.height);

    Ok(())
}

/// Test batch node storage
#[test]
fn test_fjall_batch_node_storage() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    let mut nodes = Vec::new();
    for i in 1..=10 {
        let node = TreeNode {
            digest: vec![i as u8; 32],
            node_type: NodeType::Leaf,
            key: Some(vec![(i * 2) as u8; 64]),
            value: Some(vec![(i * 3) as u8; 100]),
            left_digest: None,
            right_digest: None,
            height: (i % 10) as u8,
        };
        nodes.push(node);
    }

    // Batch store nodes
    storage.batch_store_nodes(&nodes)?;

    // Batch retrieve nodes
    let digests: Vec<Vec<u8>> = (1..=10).map(|i| vec![i as u8; 32]).collect();
    let retrieved = storage.batch_get_nodes(&digests)?;

    assert_eq!(retrieved.len(), 10);
    for (i, node_opt) in retrieved.iter().enumerate() {
        let node = node_opt.as_ref().unwrap();
        assert_eq!(node.digest, vec![(i + 1) as u8; 32]);
    }

    Ok(())
}

/// Test node range queries
#[test]
fn test_fjall_node_range_queries() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    // Store nodes with sequential digests
    for i in 1..=5 {
        let node = TreeNode {
            digest: vec![i as u8; 32],
            node_type: NodeType::Leaf,
            key: Some(vec![i as u8; 64]),
            value: Some(vec![i as u8; 100]),
            left_digest: None,
            right_digest: None,
            height: 1,
        };
        storage.store_node(&node)?;
    }

    // Query range [2, 4]
    let nodes = storage.get_nodes_by_digest_range(&[2u8; 32], &[4u8; 32])?;
    
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].digest, vec![2u8; 32]);
    assert_eq!(nodes[1].digest, vec![3u8; 32]);
    assert_eq!(nodes[2].digest, vec![4u8; 32]);

    Ok(())
}

/// Test operation logging
#[test]
fn test_fjall_operation_logging() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let mut storage = FjallTreeStorage::open(temp_dir.path())?;

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
    storage.log_operation(operation.clone())?;

    // Retrieve operation
    let operations = storage.get_operations(1, 1)?;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].sequence_number, operation.sequence_number);
    assert_eq!(operations[0].operation_type, operation.operation_type);
    assert_eq!(operations[0].key, operation.key);
    assert_eq!(operations[0].value, operation.value);

    Ok(())
}

/// Test batch operation logging
#[test]
fn test_fjall_batch_operation_logging() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let mut storage = FjallTreeStorage::open(temp_dir.path())?;

    let mut operations = Vec::new();
    for i in 1..=10 {
        let operation = TreeOperation {
            sequence_number: storage.next_sequence_number(),
            operation_type: if i % 2 == 0 { OperationType::Insert } else { OperationType::Update },
            timestamp: 1234567890 + i as u64,
            key: vec![i as u8; 64],
            value: vec![(i * 2) as u8; 100],
            previous_value: None,
            tree_root_before: vec![(i - 1) as u8; 33],
            tree_root_after: vec![i as u8; 33],
        };
        operations.push(operation);
    }

    // Batch log operations
    storage.batch_log_operations(&operations)?;

    // Retrieve operations
    let retrieved = storage.get_operations(1, 10)?;
    assert_eq!(retrieved.len(), 10);

    for (i, op) in retrieved.iter().enumerate() {
        assert_eq!(op.sequence_number, (i + 1) as u64);
        assert_eq!(op.key, vec![(i + 1) as u8; 64]);
    }

    Ok(())
}

/// Test checkpoint storage
#[test]
fn test_fjall_checkpoint_storage() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    let checkpoint = TreeCheckpoint {
        checkpoint_id: 1,
        timestamp: 1234567890,
        tree_root: vec![1u8; 33],
        operation_sequence: 100,
        node_count: 50,
        serialized_tree: None,
    };

    // Store checkpoint
    storage.store_checkpoint(&checkpoint)?;

    // Retrieve latest checkpoint
    let retrieved = storage.get_latest_checkpoint()?.unwrap();
    assert_eq!(retrieved.checkpoint_id, checkpoint.checkpoint_id);
    assert_eq!(retrieved.timestamp, checkpoint.timestamp);
    assert_eq!(retrieved.tree_root, checkpoint.tree_root);
    assert_eq!(retrieved.operation_sequence, checkpoint.operation_sequence);
    assert_eq!(retrieved.node_count, checkpoint.node_count);

    Ok(())
}

/// Test multiple checkpoints
#[test]
fn test_fjall_multiple_checkpoints() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    // Store multiple checkpoints
    for i in 1..=5 {
        let checkpoint = TreeCheckpoint {
            checkpoint_id: i,
            timestamp: 1234567890 + i as u64,
            tree_root: vec![i as u8; 33],
            operation_sequence: i * 100,
            node_count: i * 10,
            serialized_tree: None,
        };
        storage.store_checkpoint(&checkpoint)?;
    }

    // Latest checkpoint should be the one with highest ID
    let latest = storage.get_latest_checkpoint()?.unwrap();
    assert_eq!(latest.checkpoint_id, 5);
    assert_eq!(latest.tree_root, vec![5u8; 33]);
    assert_eq!(latest.operation_sequence, 500);

    Ok(())
}

/// Test recovery scenario
#[test]
fn test_fjall_recovery_scenario() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Create first storage instance and perform operations
    let mut storage1 = FjallTreeStorage::open(storage_path)?;

    // Store some nodes
    for i in 1..=5 {
        let node = TreeNode {
            digest: vec![i as u8; 32],
            node_type: NodeType::Leaf,
            key: Some(vec![i as u8; 64]),
            value: Some(vec![i as u8; 100]),
            left_digest: None,
            right_digest: None,
            height: 1,
        };
        storage1.store_node(&node)?;
    }

    // Log some operations
    for i in 1..=3 {
        let operation = TreeOperation {
            sequence_number: storage1.next_sequence_number(),
            operation_type: OperationType::Insert,
            timestamp: 1234567890 + i as u64,
            key: vec![i as u8; 64],
            value: vec![(i * 2) as u8; 100],
            previous_value: None,
            tree_root_before: vec![(i - 1) as u8; 33],
            tree_root_after: vec![i as u8; 33],
        };
        storage1.log_operation(operation)?;
    }

    // Create checkpoint
    let checkpoint = TreeCheckpoint {
        checkpoint_id: 1,
        timestamp: 1234567899,
        tree_root: vec![3u8; 33],
        operation_sequence: 3,
        node_count: 5,
        serialized_tree: None,
    };
    storage1.store_checkpoint(&checkpoint)?;

    // Create second storage instance (simulating recovery)
    let storage2 = FjallTreeStorage::open(storage_path)?;

    // Verify data is preserved
    let nodes = storage2.get_all_nodes()?;
    assert_eq!(nodes.len(), 5);

    let operations = storage2.get_operations(1, 3)?;
    assert_eq!(operations.len(), 3);

    let recovered_checkpoint = storage2.get_latest_checkpoint()?.unwrap();
    assert_eq!(recovered_checkpoint.checkpoint_id, 1);
    assert_eq!(recovered_checkpoint.operation_sequence, 3);

    Ok(())
}

/// Test sequence number persistence
#[test]
fn test_fjall_sequence_persistence() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Create first storage and generate sequence numbers
    let mut storage1 = FjallTreeStorage::open(storage_path)?;
    
    let seq1 = storage1.next_sequence_number();
    let seq2 = storage1.next_sequence_number();
    let seq3 = storage1.next_sequence_number();
    
    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert_eq!(seq3, 3);
    
    // Log an operation to persist sequence
    let operation = TreeOperation {
        sequence_number: seq3,
        operation_type: OperationType::Insert,
        timestamp: 1234567890,
        key: vec![1u8; 64],
        value: vec![2u8; 100],
        previous_value: None,
        tree_root_before: vec![0u8; 33],
        tree_root_after: vec![1u8; 33],
    };
    storage1.log_operation(operation)?;

    // Create second storage (should continue from persisted sequence)
    let mut storage2 = FjallTreeStorage::open(storage_path)?;
    
    let seq4 = storage2.next_sequence_number();
    assert_eq!(seq4, 4, "Sequence should continue from persisted value");

    Ok(())
}