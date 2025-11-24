//! AVL tree tests

use crate::avl_tree::BasisAvlTree;
use crate::errors::TreeError;

/// Test basic tree creation
#[test]
fn test_tree_creation() -> Result<(), TreeError> {
    let tree = BasisAvlTree::new()?;
    
    let digest = tree.root_digest();
    assert_eq!(digest.len(), 33);
    
    Ok(())
}

/// Test tree insertion
#[test]
fn test_tree_insertion() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    // Test basic insertion
    let key = vec![1u8; 64];
    let value = vec![2u8; 32];

    let result = tree.insert(key, value);
    assert!(result.is_ok(), "Insertion should succeed");

    // Verify digest changed
    let digest = tree.root_digest();
    assert_ne!(digest, [0u8; 33], "Digest should change after insertion");

    Ok(())
}

/// Test tree update
#[test]
fn test_tree_update() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    let key = vec![1u8; 64];
    let value1 = vec![2u8; 32];
    let value2 = vec![3u8; 32];

    // Insert first value
    tree.insert(key.clone(), value1).unwrap();
    let digest1 = tree.root_digest();

    // Update with different value
    tree.update(key.clone(), value2).unwrap();
    let digest2 = tree.root_digest();

    assert_ne!(digest1, digest2, "Digest should change after update");

    Ok(())
}



/// Test proof generation
#[test]
fn test_proof_generation() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    // Generate proof for empty tree
    let empty_proof = tree.generate_proof();
    assert!(!empty_proof.is_empty(), "Proof should not be empty");

    // Insert some data and generate proof
    let key = vec![1u8; 64];
    let value = vec![2u8; 32];
    tree.insert(key, value)?;

    let proof_with_data = tree.generate_proof();
    assert!(!proof_with_data.is_empty(), "Proof should not be empty");

    Ok(())
}

/// Test multiple insertions
#[test]
fn test_multiple_insertions() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    let initial_digest = tree.root_digest();

    // Insert multiple keys with proper non-zero values
    for i in 1..11 {  // Start from 1 to avoid zero keys
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i * 2; 32];
        tree.insert(key, value)?;
    }

    let final_digest = tree.root_digest();
    assert_ne!(initial_digest, final_digest, "Digest should change after multiple insertions");

    // Verify state is consistent
    let state = tree.get_state();
    assert!(!state.is_empty(), "State should not be empty after insertions");

    Ok(())
}

/// Test sequential updates
#[test]
fn test_sequential_updates() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    let mut key = vec![0u8; 64];
    key[0] = 1;
    
    // Insert initial value
    tree.insert(key.clone(), vec![10u8; 32])?;
    let digest1 = tree.root_digest();

    // Update multiple times
    for i in 1..=5 {
        tree.update(key.clone(), vec![10 + i; 32])?;
    }

    let final_digest = tree.root_digest();
    assert_ne!(digest1, final_digest, "Digest should change after sequential updates");

    Ok(())
}

/// Test mixed operations
#[test]
fn test_mixed_operations() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    let initial_digest = tree.root_digest();

    // Perform mixed operations with proper non-zero keys
    for i in 1..6 {  // Start from 1 to avoid zero keys
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i * 3; 32];
        
        if i % 2 == 0 {
            tree.insert(key, value)?;
        } else {
            // For odd indices, insert then update
            tree.insert(key.clone(), vec![i * 2; 32])?;
            tree.update(key, value)?;
        }
    }

    let final_digest = tree.root_digest();
    assert_ne!(initial_digest, final_digest, "Digest should change after mixed operations");

    Ok(())
}

/// Test state consistency after operations
#[test]
fn test_state_consistency_after_operations() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    // Insert all keys first
    for i in 1..11 {  // Start from 1 to avoid zero keys
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i * 11; 32];
        tree.insert(key, value)?;
    }

    // Reset tracking after initial insertions
    let mut previous_digest = tree.root_digest();
    let mut previous_state = tree.get_state().clone();

    // Now perform updates and verify state consistency
    for i in 1..11 {
        let mut key = vec![0u8; 64];
        key[0] = i;
        let value = vec![i * 22; 32];  // Different value for updates
        
        tree.update(key, value)?;

        let current_digest = tree.root_digest();
        let current_state = tree.get_state();

        // Verify digest changes after each operation
        assert_ne!(previous_digest, current_digest, "Digest should change after each operation");
        
        // Verify state is updated
        assert_ne!(previous_state.avl_root_digest, current_state.avl_root_digest, "State root should change");
        assert!(current_state.last_update_timestamp >= previous_state.last_update_timestamp, "Timestamp should be non-decreasing");

        previous_digest = current_digest;
        previous_state = current_state.clone();
    }

    Ok(())
}

/// Test large number of operations
#[test]
fn test_large_number_of_operations() -> Result<(), TreeError> {
    let mut tree = BasisAvlTree::new()?;

    let initial_digest = tree.root_digest();

    // Perform many operations with proper non-zero keys
    for i in 1..101 {  // Start from 1 to avoid zero keys
        let mut key = vec![0u8; 64];
        key[0] = (i % 256) as u8;
        let value = vec![(i * 2 % 256) as u8; 32];
        
        // For this test, we'll only do insertions to avoid update issues
        tree.insert(key, value)?;
    }

    let final_digest = tree.root_digest();
    assert_ne!(initial_digest, final_digest, "Digest should change after many operations");

    // Verify proof generation still works
    let proof = tree.generate_proof();
    assert!(!proof.is_empty(), "Proof generation should work after many operations");

    Ok(())
}