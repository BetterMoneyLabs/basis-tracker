//! In-memory recovery implementation for testing

use crate::avl_tree::BasisAvlTree;
use crate::storage::TreeStorage;
use crate::errors::TreeError;
use tempfile::tempdir;

/// Create an in-memory tree for testing recovery
pub fn create_in_memory_tree() -> Result<BasisAvlTree, TreeError> {
    let temp_dir = tempdir().unwrap();
    let storage = TreeStorage::open(temp_dir.path())?;
    BasisAvlTree::new(storage)
}

/// Test recovery with in-memory storage
pub fn test_in_memory_recovery() -> Result<(), TreeError> {
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

    // Recover tree from storage
    let storage2 = TreeStorage::open(storage_path)?;
    let tree2 = BasisAvlTree::from_storage(storage2)?;

    let final_digest2 = tree2.root_digest();

    // Verify recovery worked
    assert_eq!(final_digest1, final_digest2, "Recovery should work with in-memory storage");

    Ok(())
}

/// Test recovery with no operations after checkpoint
pub fn test_in_memory_recovery_no_operations_after_checkpoint() -> Result<(), TreeError> {
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

/// Test recovery with mixed operations
pub fn test_in_memory_recovery_mixed_operations() -> Result<(), TreeError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_recovery_basic() -> Result<(), TreeError> {
        test_in_memory_recovery()
    }

    #[test]
    fn test_in_memory_recovery_no_ops_after_checkpoint() -> Result<(), TreeError> {
        test_in_memory_recovery_no_operations_after_checkpoint()
    }

    #[test]
    fn test_in_memory_recovery_mixed_ops() -> Result<(), TreeError> {
        test_in_memory_recovery_mixed_operations()
    }

    #[test]
    fn test_in_memory_tree_creation() -> Result<(), TreeError> {
        let tree = create_in_memory_tree()?;
        
        // Should be able to create tree without errors
        assert!(tree.get_state().is_empty(), "Should create empty tree");
        
        Ok(())
    }
}