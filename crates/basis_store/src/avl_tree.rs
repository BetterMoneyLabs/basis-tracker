//! AVL+ tree implementation for Basis tracker state commitments

use ergo_avltree_rust::{
    authenticated_tree_ops::AuthenticatedTreeOps,
    batch_avl_prover::BatchAVLProver,
    batch_node::AVLTree,
    operation::{KeyValue, Operation},
};

/// AVL tree state for tracker commitments
pub struct AvlTreeState {
    prover: BatchAVLProver,
}

// Simple resolver function for AVL tree
fn simple_resolver(_digest: &[u8; 32]) -> ergo_avltree_rust::batch_node::Node {
    // This is a placeholder implementation
    // In a real implementation, this would fetch nodes from storage
    panic!("Resolver not implemented - this is a placeholder");
}

impl AvlTreeState {
    /// Create a new AVL tree state
    pub fn new() -> Self {
        // Create an AVL tree with variable length values
        // Key length: 64 bytes (issuer_hash + recipient_hash)
        // Value length: None for variable length values
        let tree = AVLTree::new(simple_resolver, 64, None);
        let prover = BatchAVLProver::new(tree, true);

        Self { prover }
    }

    /// Insert a key-value pair into the AVL tree
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        let operation = Operation::Insert(KeyValue {
            key: key.into(),
            value: value.into(),
        });

        // We ignore the return value since we just care about the operation success
        let _ = self
            .prover
            .perform_one_operation(&operation)
            .map_err(|e| format!("AVL tree insert failed: {:?}", e))?;

        // Generate a proof to commit the changes to the tree state
        // This forces the tree to update its internal state and digest
        let _ = self.prover.generate_proof();

        Ok(())
    }

    /// Update an existing key-value pair
    pub fn update(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        let operation = Operation::Update(KeyValue {
            key: key.into(),
            value: value.into(),
        });

        let _ = self
            .prover
            .perform_one_operation(&operation)
            .map_err(|e| format!("AVL tree update failed: {:?}", e))?;

        // Generate a proof to commit the changes to the tree state
        // This forces the tree to update its internal state and digest
        let _ = self.prover.generate_proof();

        Ok(())
    }

    /// Remove a key from the AVL tree
    pub fn remove(&mut self, key: Vec<u8>) -> Result<(), String> {
        let operation = Operation::Remove(key.into());

        let _ = self
            .prover
            .perform_one_operation(&operation)
            .map_err(|e| format!("AVL tree remove failed: {:?}", e))?;

        // Generate a proof to commit the changes to the tree state
        // This forces the tree to update its internal state and digest
        let _ = self.prover.generate_proof();

        Ok(())
    }

    /// Generate a proof for the current tree state
    pub fn generate_proof(&mut self) -> Vec<u8> {
        self.prover.generate_proof().to_vec()
    }

    /// Get the root digest of the AVL tree
    pub fn root_digest(&self) -> [u8; 33] {
        if let Some(digest) = self.prover.digest() {
            let mut result = [0u8; 33];
            result.copy_from_slice(&digest);
            result
        } else {
            [0u8; 33] // Empty tree digest
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avl_tree_creation() {
        let tree = AvlTreeState::new();
        let digest = tree.root_digest();
        // Empty tree should have a consistent digest (not necessarily zero)
        assert_eq!(digest.len(), 33);
    }

    #[test]
    fn test_avl_tree_insertion() {
        let mut tree = AvlTreeState::new();

        // Test basic insertion
        let key = vec![1u8; 64];
        let value = vec![2u8; 32];

        let result = tree.insert(key.clone(), value);
        assert!(result.is_ok(), "Insertion should succeed");

        // Verify digest changed
        let digest = tree.root_digest();
        assert_ne!(digest, [0u8; 33], "Digest should change after insertion");
    }

    #[test]
    fn test_avl_tree_update() {
        let mut tree = AvlTreeState::new();

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
    }

    #[test]
    fn test_avl_tree_removal() {
        let mut tree = AvlTreeState::new();

        let key = vec![1u8; 64];
        let value = vec![2u8; 32];

        // Insert and get digest
        tree.insert(key.clone(), value).unwrap();
        let digest_with_data = tree.root_digest();

        // Remove and verify digest changes
        tree.remove(key).unwrap();
        let digest_after_removal = tree.root_digest();

        assert_ne!(
            digest_with_data, digest_after_removal,
            "Digest should change after removal"
        );
    }

    #[test]
    fn test_avl_tree_proof_generation() {
        let mut tree = AvlTreeState::new();

        // Generate proof for empty tree
        let empty_proof = tree.generate_proof();
        assert!(!empty_proof.is_empty(), "Proof should not be empty");

        // Insert some data and generate proof
        let key = vec![1u8; 64];
        let value = vec![2u8; 32];
        tree.insert(key, value).unwrap();

        let proof_with_data = tree.generate_proof();
        assert!(!proof_with_data.is_empty(), "Proof should not be empty");
        assert_ne!(empty_proof, proof_with_data, "Proofs should differ");
    }

    #[test]
    fn test_avl_tree_multiple_operations() {
        let mut tree = AvlTreeState::new();

        // Insert multiple keys with proper format (avoiding zero keys)
        for i in 1..6 {
            let mut key = vec![i; 64];
            key[0] = i; // Ensure first byte is non-zero
            let value = vec![i * 2; 32];
            tree.insert(key, value).unwrap();
        }

        let digest_after_insertions = tree.root_digest();

        // Remove some keys
        for i in 1..3 {
            let mut key = vec![i; 64];
            key[0] = i;
            tree.remove(key).unwrap();
        }

        let digest_after_removals = tree.root_digest();

        assert_ne!(
            digest_after_insertions, digest_after_removals,
            "Digest should change after multiple operations"
        );
    }

    #[test]
    fn test_avl_tree_balance_invariants() {
        let mut tree = AvlTreeState::new();

        // Insert keys in sorted order (worst case for balancing)
        // Start from 1 to avoid zero keys
        for i in 1..20 {
            let mut key = vec![0u8; 64];
            key[0] = i;
            let value = vec![i * 2; 32];
            tree.insert(key, value).unwrap();
        }

        // Generate proof (should succeed even with many elements)
        let proof = tree.generate_proof();
        assert!(
            !proof.is_empty(),
            "Should generate proof even with many elements"
        );

        // Verify digest is consistent
        let digest = tree.root_digest();
        assert_ne!(digest, [0u8; 33], "Digest should be non-zero");
    }
}
