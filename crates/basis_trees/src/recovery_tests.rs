//! Recovery tests for AVL tree persistence

use crate::avl_tree::BasisAvlTree;
use crate::storage::TreeStorage;
use crate::errors::TreeError;
use tempfile::tempdir;

/// Test basic recovery from checkpoint
#[test]
fn test_basic_recovery() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Create first tree and perform operations
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Insert some data
    for i in 1..=5 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    // Create checkpoint
    tree1.create_checkpoint()?;
    
    // Insert more data after checkpoint
    for i in 6..=10 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    let final_digest1 = tree1.root_digest();
    let final_state1 = tree1.get_state().clone();

    // Recover tree from storage
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    // Verify recovered state matches original
    let final_digest2 = tree2.root_digest();
    let final_state2 = tree2.get_state();

    assert_eq!(final_digest1, final_digest2, "Digests should match after recovery");
    assert_eq!(final_state1.avl_root_digest, final_state2.avl_root_digest, "State roots should match");

    Ok(())
}

/// Test recovery with no checkpoint (should create new tree)
#[test]
fn test_recovery_no_checkpoint() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Create storage but no operations
    let storage = TreeStorage::open(storage_path)?;
    
    // Recover from empty storage
    let tree = BasisAvlTree::from_storage(storage)?;
    
    // Should create new empty tree
    assert!(tree.get_state().is_empty(), "Should create empty tree when no checkpoint exists");
    
    Ok(())
}

/// Test recovery with operations after checkpoint
#[test]
fn test_recovery_with_post_checkpoint_operations() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    // Create first tree
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Insert initial data
    for i in 1..=3 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    let digest_before_checkpoint = tree1.root_digest();
    
    // Create checkpoint
    tree1.create_checkpoint()?;
    
    // Insert more data after checkpoint
    for i in 4..=6 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    let final_digest1 = tree1.root_digest();
    
    // Recover tree
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let final_digest2 = tree2.root_digest();
    
    // Verify recovery worked correctly
    assert_ne!(digest_before_checkpoint, final_digest1, "Digest should change after checkpoint");
    assert_eq!(final_digest1, final_digest2, "Recovered digest should match original");

    Ok(())
}

/// Test recovery with mixed operations
#[test]
fn test_recovery_with_mixed_operations() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Insert initial data
    for i in 1..=5 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key.clone(), value)?;
    }

    // Create checkpoint
    tree1.create_checkpoint()?;
    
    // Perform mixed operations after checkpoint
    for i in 1..=5 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        
        if i % 2 == 0 {
            // Update existing keys
            let value = vec![i * 2; 32];
            tree1.update(key, value)?;
        } else {
            // Insert new keys
            let mut new_key = vec![0u8; 64];
            new_key[0] = i + 10;
            let value = vec![i + 10; 32];
            tree1.insert(new_key, value)?;
        }
    }

    let final_digest1 = tree1.root_digest();
    
    // Recover tree
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let final_digest2 = tree2.root_digest();
    
    assert_eq!(final_digest1, final_digest2, "Recovered state should match after mixed operations");

    Ok(())
}

/// Test multiple checkpoints and recovery
#[test]
fn test_multiple_checkpoints_recovery() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Insert data and create first checkpoint
    for i in 1..=3 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }
    tree1.create_checkpoint()?;

    // Insert more data and create second checkpoint
    for i in 4..=6 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }
    tree1.create_checkpoint()?;

    // Insert final data
    for i in 7..=9 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    let final_digest1 = tree1.root_digest();
    
    // Recover tree - should use latest checkpoint
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let final_digest2 = tree2.root_digest();
    
    assert_eq!(final_digest1, final_digest2, "Recovery should work with multiple checkpoints");

    Ok(())
}

/// Test recovery with large number of operations
#[test]
fn test_recovery_with_many_operations() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Insert initial batch
    for i in 1..=20 {
        let mut key = vec![0u8; 64];
        key[0] = (i % 256) as u8;
        let value = vec![(i % 256) as u8; 32];
        tree1.insert(key, value)?;
    }

    // Create checkpoint
    tree1.create_checkpoint()?;
    
    // Insert many more operations
    for i in 21..=100 {
        let mut key = vec![0u8; 64];
        key[0] = (i % 256) as u8;
        let value = vec![(i % 256) as u8; 32];
        tree1.insert(key, value)?;
    }

    let final_digest1 = tree1.root_digest();
    
    // Recover tree
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let final_digest2 = tree2.root_digest();
    
    assert_eq!(final_digest1, final_digest2, "Recovery should handle many operations");

    Ok(())
}

/// Test recovery state consistency
#[test]
fn test_recovery_state_consistency() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Perform various operations
    let mut operations = Vec::new();
    
    for i in 1..=10 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        
        if i % 3 == 0 {
            // Insert
            tree1.insert(key.clone(), value.clone())?;
            operations.push(("insert", key.clone(), value.clone()));
        } else if i % 3 == 1 {
            // Insert then update
            tree1.insert(key.clone(), vec![i; 32])?;
            tree1.update(key.clone(), value.clone())?;
            operations.push(("update", key.clone(), value.clone()));
        } else {
            // Just insert
            tree1.insert(key.clone(), value.clone())?;
            operations.push(("insert", key.clone(), value.clone()));
        }
    }

    // Create checkpoint
    tree1.create_checkpoint()?;
    
    // Perform more operations
    for i in 11..=15 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    let final_state1 = tree1.get_state().clone();
    
    // Recover tree
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let final_state2 = tree2.get_state();
    
    // Verify state consistency
    assert_eq!(final_state1.avl_root_digest, final_state2.avl_root_digest, "State roots should be consistent");
    assert!(!final_state2.is_empty(), "Recovered state should not be empty");

    Ok(())
}

/// Test recovery with no operations after checkpoint
#[test]
fn test_recovery_no_operations_after_checkpoint() -> Result<(), TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path();
    
    let storage1 = TreeStorage::open(storage_path)?;
    let mut tree1 = BasisAvlTree::new(storage1)?;

    // Insert data
    for i in 1..=5 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i; 32];
        tree1.insert(key, value)?;
    }

    let digest_before_checkpoint = tree1.root_digest();
    
    // Create checkpoint
    tree1.create_checkpoint()?;
    
    // No operations after checkpoint
    
    // Recover tree
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let digest_after_recovery = tree2.root_digest();
    
    // Should match checkpoint state exactly
    assert_eq!(digest_before_checkpoint, digest_after_recovery, "Recovery should match checkpoint state when no operations after");

    Ok(())
}