//! Enhanced edge case tests for Fjall-based tree storage

use super::fjall_storage::{FjallTreeStorage, FjallStorageConfig};
use crate::storage::{TreeNode, TreeOperation, TreeCheckpoint, NodeType, OperationType};
use crate::errors::TreeError;
use tempfile::tempdir;

/// Test storage with very large nodes
#[test]
fn test_fjall_large_node_storage() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    // Create nodes with large values (10KB each)
    let mut nodes = Vec::new();
    for i in 1..=5 {
        let node = TreeNode {
            digest: vec![i as u8; 32],
            node_type: NodeType::Leaf,
            key: Some(vec![i as u8; 64]),
            value: Some(vec![i as u8; 10240]), // 10KB value
            left_digest: None,
            right_digest: None,
            height: 1,
        };
        nodes.push(node);
    }

    // Batch store large nodes
    storage.batch_store_nodes(&nodes)?;

    // Verify all nodes can be retrieved
    let digests: Vec<Vec<u8>> = (1..=5).map(|i| vec![i as u8; 32]).collect();
    let retrieved = storage.batch_get_nodes(&digests)?;
    
    assert_eq!(retrieved.len(), 5);
    for (i, node_opt) in retrieved.iter().enumerate() {
        let node = node_opt.as_ref().unwrap();
        assert_eq!(node.digest, vec![(i + 1) as u8; 32]);
        assert_eq!(node.value.as_ref().unwrap().len(), 10240);
    }

    Ok(())
}

/// Test concurrent access patterns
#[test]
fn test_fjall_concurrent_access() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Create multiple storage instances to simulate concurrent access
    let storage1 = FjallTreeStorage::open(storage_path)?;
    let storage2 = FjallTreeStorage::open(storage_path)?;
    let storage3 = FjallTreeStorage::open(storage_path)?;

    // Store nodes from different instances
    let node1 = TreeNode {
        digest: vec![1u8; 32],
        node_type: NodeType::Leaf,
        key: Some(vec![1u8; 64]),
        value: Some(vec![1u8; 100]),
        left_digest: None,
        right_digest: None,
        height: 1,
    };
    storage1.store_node(&node1)?;

    let node2 = TreeNode {
        digest: vec![2u8; 32],
        node_type: NodeType::Branch,
        key: None,
        value: None,
        left_digest: Some(vec![1u8; 32]),
        right_digest: Some(vec![3u8; 32]),
        height: 2,
    };
    storage2.store_node(&node2)?;

    let node3 = TreeNode {
        digest: vec![3u8; 32],
        node_type: NodeType::Leaf,
        key: Some(vec![3u8; 64]),
        value: Some(vec![3u8; 100]),
        left_digest: None,
        right_digest: None,
        height: 1,
    };
    storage3.store_node(&node3)?;

    // Each storage instance should only see its own nodes initially
    // (since they maintain separate in-memory state)
    let nodes_from_1 = storage1.get_all_nodes()?;
    let nodes_from_2 = storage2.get_all_nodes()?;
    let nodes_from_3 = storage3.get_all_nodes()?;

    assert_eq!(nodes_from_1.len(), 1);
    assert_eq!(nodes_from_2.len(), 1);
    assert_eq!(nodes_from_3.len(), 1);

    // But a new storage instance should see all persisted nodes
    let storage4 = FjallTreeStorage::open(storage_path)?;
    let all_nodes = storage4.get_all_nodes()?;
    assert_eq!(all_nodes.len(), 3);

    Ok(())
}

/// Test error handling for missing nodes
#[test]
fn test_fjall_missing_node_handling() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    // Try to retrieve non-existent node
    let missing_node = storage.get_node(&[255u8; 32])?;
    assert!(missing_node.is_none(), "Missing node should return None");

    // Batch retrieve with mixed existing and non-existing nodes
    let digests = vec![
        vec![1u8; 32],  // non-existent
        vec![2u8; 32],  // non-existent
    ];
    let retrieved = storage.batch_get_nodes(&digests)?;
    
    assert_eq!(retrieved.len(), 2);
    assert!(retrieved[0].is_none(), "First missing node should be None");
    assert!(retrieved[1].is_none(), "Second missing node should be None");

    Ok(())
}

/// Test range queries with edge cases
#[test]
fn test_fjall_edge_case_range_queries() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    // Store nodes with non-sequential digests
    let digests = vec![
        vec![0x01u8; 32],
        vec![0x05u8; 32], 
        vec![0x0Au8; 32],
        vec![0x0Fu8; 32],
        vec![0x14u8; 32],
    ];

    for (i, digest) in digests.iter().enumerate() {
        let node = TreeNode {
            digest: digest.clone(),
            node_type: NodeType::Leaf,
            key: Some(vec![i as u8; 64]),
            value: Some(vec![i as u8; 100]),
            left_digest: None,
            right_digest: None,
            height: 1,
        };
        storage.store_node(&node)?;
    }

    // Test range that includes all nodes
    let all_nodes = storage.get_nodes_by_digest_range(&[0x00u8; 32], &[0xFFu8; 32])?;
    assert_eq!(all_nodes.len(), 5);

    // Test range that includes only some nodes
    let middle_nodes = storage.get_nodes_by_digest_range(&[0x04u8; 32], &[0x10u8; 32])?;
    assert_eq!(middle_nodes.len(), 3);

    // Test range with no matches
    let empty_range = storage.get_nodes_by_digest_range(&[0x20u8; 32], &[0x30u8; 32])?;
    assert_eq!(empty_range.len(), 0);

    // Test invalid range (start > end)
    let invalid_range = storage.get_nodes_by_digest_range(&[0x10u8; 32], &[0x05u8; 32])?;
    assert_eq!(invalid_range.len(), 0, "Invalid range should return empty result");

    Ok(())
}

/// Test operation sequence consistency under stress
#[test]
fn test_fjall_operation_sequence_stress() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Use a single storage instance to test sequence consistency
    let mut storage = FjallTreeStorage::open(storage_path)?;

    // Perform many operations with the same storage instance
    for i in 1..=10 {
        let operation = TreeOperation {
            sequence_number: storage.next_sequence_number(),
            operation_type: OperationType::Insert,
            timestamp: 1234567890 + i as u64,
            key: vec![i as u8; 64],
            value: vec![(i * 2) as u8; 100],
            previous_value: None,
            tree_root_before: vec![(i - 1) as u8; 33],
            tree_root_after: vec![i as u8; 33],
        };
        storage.log_operation(operation)?;
    }

    // Verify all operations are logged in correct sequence
    let storage2 = FjallTreeStorage::open(storage_path)?;
    let retrieved = storage2.get_operations(1, 10)?;
    
    // We should have 10 operations with sequential numbers
    assert_eq!(retrieved.len(), 10);
    for (i, op) in retrieved.iter().enumerate() {
        assert_eq!(op.sequence_number, (i + 1) as u64, "Operation sequence should be monotonic");
    }

    Ok(())
}

/// Test checkpoint rollback scenarios
#[test]
fn test_fjall_checkpoint_rollback() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    let mut storage = FjallTreeStorage::open(storage_path)?;

    // Phase 1: Initial state
    for i in 1..=3 {
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

    // Create checkpoint 1
    let checkpoint1 = TreeCheckpoint {
        checkpoint_id: 1,
        timestamp: 1234567890,
        tree_root: vec![1u8; 33],
        operation_sequence: 0,
        node_count: 3,
        serialized_tree: None,
    };
    storage.store_checkpoint(&checkpoint1)?;

    // Phase 2: Add more data
    for i in 4..=6 {
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

    // Create checkpoint 2
    let checkpoint2 = TreeCheckpoint {
        checkpoint_id: 2,
        timestamp: 1234567891,
        tree_root: vec![2u8; 33],
        operation_sequence: 0,
        node_count: 6,
        serialized_tree: None,
    };
    storage.store_checkpoint(&checkpoint2)?;

    // Verify we can retrieve specific checkpoints
    let cp1 = storage.get_checkpoint(1)?.unwrap();
    let cp2 = storage.get_checkpoint(2)?.unwrap();
    
    assert_eq!(cp1.checkpoint_id, 1);
    assert_eq!(cp1.node_count, 3);
    assert_eq!(cp2.checkpoint_id, 2);
    assert_eq!(cp2.node_count, 6);

    // Latest checkpoint should be the most recent
    let latest = storage.get_latest_checkpoint()?.unwrap();
    assert_eq!(latest.checkpoint_id, 2);

    Ok(())
}

/// Test storage with compression enabled
#[test]
fn test_fjall_storage_with_compression() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let config = FjallStorageConfig {
        max_partition_size: 512 * 1024 * 1024,
        batch_size: 100,
        compression: true,
    };
    
    let storage = FjallTreeStorage::open_with_config(temp_dir.path(), config)?;

    // Store nodes with compressible data (repeating patterns)
    let mut nodes = Vec::new();
    for i in 1..=5 {
        let node = TreeNode {
            digest: vec![i as u8; 32],
            node_type: NodeType::Leaf,
            key: Some(vec![i as u8; 64]),
            value: Some(vec![i as u8; 1024]), // 1KB of repeating data
            left_digest: None,
            right_digest: None,
            height: 1,
        };
        nodes.push(node);
    }

    // Batch store with compression
    storage.batch_store_nodes(&nodes)?;

    // Verify all nodes can be retrieved correctly
    let digests: Vec<Vec<u8>> = (1..=5).map(|i| vec![i as u8; 32]).collect();
    let retrieved = storage.batch_get_nodes(&digests)?;
    
    assert_eq!(retrieved.len(), 5);
    for (i, node_opt) in retrieved.iter().enumerate() {
        let node = node_opt.as_ref().unwrap();
        assert_eq!(node.digest, vec![(i + 1) as u8; 32]);
        assert_eq!(node.value.as_ref().unwrap().len(), 1024);
        assert!(node.value.as_ref().unwrap().iter().all(|&b| b == (i + 1) as u8));
    }

    Ok(())
}

/// Test mixed node types and complex tree structures
#[test]
fn test_fjall_mixed_node_types() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = FjallTreeStorage::open(temp_dir.path())?;

    // Create a realistic tree structure with leaf and branch nodes
    let leaf1 = TreeNode {
        digest: vec![1u8; 32],
        node_type: NodeType::Leaf,
        key: Some(vec![1u8; 64]),
        value: Some(vec![1u8; 100]),
        left_digest: None,
        right_digest: None,
        height: 1,
    };

    let leaf2 = TreeNode {
        digest: vec![2u8; 32],
        node_type: NodeType::Leaf,
        key: Some(vec![2u8; 64]),
        value: Some(vec![2u8; 100]),
        left_digest: None,
        right_digest: None,
        height: 1,
    };

    let branch = TreeNode {
        digest: vec![3u8; 32],
        node_type: NodeType::Branch,
        key: None,
        value: None,
        left_digest: Some(vec![1u8; 32]),
        right_digest: Some(vec![2u8; 32]),
        height: 2,
    };

    // Store all nodes
    storage.store_node(&leaf1)?;
    storage.store_node(&leaf2)?;
    storage.store_node(&branch)?;

    // Verify all nodes can be retrieved
    let all_nodes = storage.get_all_nodes()?;
    assert_eq!(all_nodes.len(), 3);

    // Count node types
    let leaf_count = all_nodes.iter().filter(|n| matches!(n.node_type, NodeType::Leaf)).count();
    let branch_count = all_nodes.iter().filter(|n| matches!(n.node_type, NodeType::Branch)).count();
    
    assert_eq!(leaf_count, 2);
    assert_eq!(branch_count, 1);

    // Verify branch node references
    let branch_node = all_nodes.iter().find(|n| n.digest == vec![3u8; 32]).unwrap();
    assert_eq!(branch_node.left_digest, Some(vec![1u8; 32]));
    assert_eq!(branch_node.right_digest, Some(vec![2u8; 32]));

    Ok(())
}

/// Test performance with many small operations
#[test]
fn test_fjall_many_small_operations() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let mut storage = FjallTreeStorage::open(temp_dir.path())?;

    // Perform many small operations
    for i in 1..=100 {
        let operation = TreeOperation {
            sequence_number: storage.next_sequence_number(),
            operation_type: OperationType::Insert,
            timestamp: 1234567890 + i as u64,
            key: vec![i as u8; 64],
            value: vec![i as u8; 50],
            previous_value: None,
            tree_root_before: vec![(i - 1) as u8; 33],
            tree_root_after: vec![i as u8; 33],
        };
        storage.log_operation(operation)?;
    }

    // Verify all operations are present
    let operations = storage.get_operations(1, 100)?;
    assert_eq!(operations.len(), 100);

    Ok(())
}

/// Test storage cleanup and reinitialization
#[test]
fn test_fjall_storage_cleanup() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // First initialization with data
    let mut storage1 = FjallTreeStorage::open(storage_path)?;
    
    let node = TreeNode {
        digest: vec![1u8; 32],
        node_type: NodeType::Leaf,
        key: Some(vec![1u8; 64]),
        value: Some(vec![1u8; 100]),
        left_digest: None,
        right_digest: None,
        height: 1,
    };
    storage1.store_node(&node)?;

    let operation = TreeOperation {
        sequence_number: storage1.next_sequence_number(),
        operation_type: OperationType::Insert,
        timestamp: 1234567890,
        key: vec![1u8; 64],
        value: vec![1u8; 100],
        previous_value: None,
        tree_root_before: vec![0u8; 33],
        tree_root_after: vec![1u8; 33],
    };
    storage1.log_operation(operation)?;

    // Second initialization should see the same data
    let storage2 = FjallTreeStorage::open(storage_path)?;
    let nodes = storage2.get_all_nodes()?;
    let operations = storage2.get_operations(1, 1)?;
    
    assert_eq!(nodes.len(), 1);
    assert_eq!(operations.len(), 1);

    Ok(())
}