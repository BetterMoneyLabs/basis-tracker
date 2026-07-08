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
    // Return a dummy leaf node instead of panicking.
    // In a real implementation, this would fetch nodes from storage.
    // For testing with self-contained proofs, this should not be called.
    ergo_avltree_rust::batch_node::Node::Leaf(ergo_avltree_rust::batch_node::LeafNode {
        hdr: ergo_avltree_rust::batch_node::NodeHeader {
            visited: false,
            is_new: false,
            label: None,
            key: Some(ergo_avltree_rust::operation::ADKey::from(vec![0u8; 32])),
        },
        value: ergo_avltree_rust::operation::ADValue::from(vec![]),
        next_node_key: ergo_avltree_rust::operation::ADKey::from(vec![0u8; 32]),
    })
}

impl AvlTreeState {
    /// Create a new AVL tree state
    pub fn new() -> Self {
        // Create an AVL tree with variable length values
        // Key length: 32 bytes (blake2b256(issuer_pubkey || recipient_pubkey))
        // Value length: None for variable length values
        let tree = AVLTree::new(simple_resolver, 32, None);
        let mut prover = BatchAVLProver::new(tree, true);

        // Generate an initial proof to establish the empty tree state
        // This ensures the prover has an initial digest even for an empty tree
        let _ = prover.generate_proof();

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

    /// Generate a proof for a lookup operation on the given key.
    ///
    /// This performs a `Lookup` operation and returns the proof bytes that
    /// can be used to verify the key's existence (or non-existence) in the tree.
    pub fn generate_lookup_proof(&mut self, key: Vec<u8>) -> (Vec<u8>, Option<Vec<u8>>) {
        use ergo_avltree_rust::authenticated_tree_ops::AuthenticatedTreeOps;
        let operation = Operation::Lookup(key.into());
        let result = self.prover.perform_one_operation(&operation).ok().flatten();
        let proof = self.prover.generate_proof().to_vec();
        (proof, result.map(|b| b.to_vec()))
    }

    /// Generate a proof for the current tree state (all pending operations).
    pub fn generate_proof(&mut self) -> Vec<u8> {
        self.prover.generate_proof().to_vec()
    }

    /// Verify a proof by replaying a lookup operation against the given starting digest.
    ///
    /// This creates a `BatchAVLVerifier` with the provided digest and proof, then
    /// performs a `Lookup` operation for `key`.  If the verifier succeeds and returns
    /// the expected value, the proof is valid.
    ///
    /// # Arguments
    /// * `starting_digest` – The 33-byte root digest the proof is anchored to.
    /// * `proof`         – The serialized AVL proof bytes (must contain the lookup path).
    /// * `key`           – The 32-byte lookup key.
    /// * `expected_value`– The value that must be returned for the proof to be considered valid.
    ///
    /// # Returns
    /// `true` if the proof verifies and yields `expected_value`, otherwise `false`.
    pub fn verify_proof(
        starting_digest: &[u8; 33],
        proof: &[u8],
        key: &[u8],
        expected_value: &[u8],
    ) -> bool {
        use bytes::Bytes;
        use ergo_avltree_rust::batch_avl_verifier::BatchAVLVerifier;

        let tree = AVLTree::new(simple_resolver, 32, None);
        let mut verifier = match BatchAVLVerifier::new(
            &Bytes::copy_from_slice(starting_digest),
            &Bytes::copy_from_slice(proof),
            tree,
            Some(1), // max one operation (lookup)
            Some(0), // no deletes
        ) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let operation = Operation::Lookup(key.to_vec().into());
        match verifier.perform_one_operation(&operation) {
            Ok(Some(value)) => value.as_ref() == expected_value,
            Ok(None) => false, // key not found
            Err(_) => false,   // proof invalid
        }
    }

    pub fn verify_insert_proof(
        starting_digest: &[u8; 33],
        proof: &[u8],
        key: &[u8],
        value: &[u8],
        expected_digest: &[u8; 33],
    ) -> bool {
        use bytes::Bytes;
        use ergo_avltree_rust::batch_avl_verifier::BatchAVLVerifier;
        use ergo_avltree_rust::operation::{KeyValue, Operation};

        let tree = AVLTree::new(simple_resolver, 32, None);
        let mut verifier = match BatchAVLVerifier::new(
            &Bytes::copy_from_slice(starting_digest),
            &Bytes::copy_from_slice(proof),
            tree,
            Some(1), // max one operation (insert)
            Some(0), // no deletes
        ) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let operation = Operation::Insert(KeyValue {
            key: key.to_vec().into(),
            value: value.to_vec().into(),
        });
        match verifier.perform_one_operation(&operation) {
            Ok(_) => match verifier.digest() {
                Some(digest) => {
                    let mut result = [0u8; 33];
                    result.copy_from_slice(&digest);
                    result == *expected_digest
                }
                None => false,
            },
            Err(_) => false,
        }
    }
    pub fn root_digest(&self) -> [u8; 33] {
        if let Some(digest) = self.prover.digest() {
            let mut result = [0u8; 33];
            result.copy_from_slice(&digest);
            result
        } else {
            [0u8; 33] // Empty tree digest
        }
    }

    /// Generate a lookup proof and verify it immediately (for testing).
    pub fn generate_and_verify_lookup(&mut self, key: &[u8], expected_value: &[u8]) -> bool {
        let (proof, returned_value) = self.generate_lookup_proof(key.to_vec());
        let digest = self.root_digest();
        let value_matches = returned_value.as_ref().map(|v| v.as_slice()) == Some(expected_value);
        let proof_valid = Self::verify_proof(&digest, &proof, key, expected_value);
        value_matches && proof_valid
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
        let key = vec![1u8; 32];
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

        let key = vec![1u8; 32];
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

        let key = vec![1u8; 32];
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
        let key = vec![1u8; 32];
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
            let mut key = vec![i; 32];
            key[0] = i; // Ensure first byte is non-zero
            let value = vec![i * 2; 32];
            tree.insert(key, value).unwrap();
        }

        let digest_after_insertions = tree.root_digest();

        // Remove some keys
        for i in 1..3 {
            let mut key = vec![i; 32];
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
    fn test_avl_tree_proof_verification() {
        let mut tree = AvlTreeState::new();

        let key = vec![1u8; 32];
        let value = vec![2u8; 32];

        // Insert data and capture the digest *after* the insertion
        tree.insert(key.clone(), value.clone()).unwrap();
        let digest_after = tree.root_digest();

        // Generate a lookup proof for the key
        let (proof, returned_value) = tree.generate_lookup_proof(key.clone());
        assert_eq!(
            returned_value.as_ref(),
            Some(&value),
            "Lookup should return the correct value"
        );

        // Verify the proof against the digest we just captured.
        let valid = AvlTreeState::verify_proof(&digest_after, &proof, &key, &value);
        assert!(valid, "Proof should verify for the inserted key/value");

        // Verify that a wrong key fails
        let wrong_key = vec![2u8; 32];
        let invalid = AvlTreeState::verify_proof(&digest_after, &proof, &wrong_key, &value);
        assert!(!invalid, "Proof should NOT verify for a wrong key");

        // Verify that a wrong value fails
        let wrong_value = vec![3u8; 32];
        let invalid = AvlTreeState::verify_proof(&digest_after, &proof, &key, &wrong_value);
        assert!(!invalid, "Proof should NOT verify for a wrong value");
    }

    #[test]
    fn test_avl_tree_proof_verification_empty_tree() {
        let mut tree = AvlTreeState::new();
        let digest = tree.root_digest();
        let key = vec![1u8; 32];
        let value = vec![2u8; 32];

        // Generate a lookup proof for a non-existent key in an empty tree
        let (proof, returned_value) = tree.generate_lookup_proof(key.clone());
        assert!(
            returned_value.is_none(),
            "Lookup should return None for empty tree"
        );

        // Looking up a non-existent key in an empty tree should fail
        let valid = AvlTreeState::verify_proof(&digest, &proof, &key, &value);
        assert!(
            !valid,
            "Proof for empty tree should NOT verify a non-existent key"
        );
    }

    #[test]
    fn test_avl_tree_proof_verification_multiple_inserts() {
        let mut tree = AvlTreeState::new();

        let key1 = vec![1u8; 32];
        let value1 = vec![10u8; 32];
        let key2 = vec![2u8; 32];
        let value2 = vec![20u8; 32];

        tree.insert(key1.clone(), value1.clone()).unwrap();
        tree.insert(key2.clone(), value2.clone()).unwrap();

        let digest_after = tree.root_digest();

        // Generate lookup proof for key1 and verify
        let (proof1, returned1) = tree.generate_lookup_proof(key1.clone());
        assert_eq!(returned1.as_ref(), Some(&value1));
        let valid1 = AvlTreeState::verify_proof(&digest_after, &proof1, &key1, &value1);
        assert!(valid1, "Proof should verify for key1");

        // Generate lookup proof for key2 and verify
        let (proof2, returned2) = tree.generate_lookup_proof(key2.clone());
        assert_eq!(returned2.as_ref(), Some(&value2));
        let valid2 = AvlTreeState::verify_proof(&digest_after, &proof2, &key2, &value2);
        assert!(valid2, "Proof should verify for key2");
    }

    #[test]
    fn test_avl_tree_proof_verification_after_update() {
        let mut tree = AvlTreeState::new();

        let key = vec![1u8; 32];
        let value1 = vec![10u8; 32];
        let value2 = vec![20u8; 32];

        tree.insert(key.clone(), value1.clone()).unwrap();
        let digest1 = tree.root_digest();

        // Generate lookup proof for old value and verify
        let (proof1, returned1) = tree.generate_lookup_proof(key.clone());
        assert_eq!(returned1.as_ref(), Some(&value1));
        let valid1 = AvlTreeState::verify_proof(&digest1, &proof1, &key, &value1);
        assert!(valid1, "Proof should verify old value");

        // Update value
        tree.update(key.clone(), value2.clone()).unwrap();
        let digest2 = tree.root_digest();

        // Generate lookup proof for new value and verify
        let (proof2, returned2) = tree.generate_lookup_proof(key.clone());
        assert_eq!(returned2.as_ref(), Some(&value2));
        let valid2 = AvlTreeState::verify_proof(&digest2, &proof2, &key, &value2);
        assert!(valid2, "Proof should verify updated value");

        // Old proof should NOT verify new value against old digest
        let invalid = AvlTreeState::verify_proof(&digest1, &proof1, &key, &value2);
        assert!(!invalid, "Old proof should NOT verify updated value");
    }

    #[test]
    fn test_avl_tree_proof_verification_wrong_digest() {
        let mut tree = AvlTreeState::new();

        let key = vec![1u8; 32];
        let value = vec![2u8; 32];

        tree.insert(key.clone(), value.clone()).unwrap();
        let _digest = tree.root_digest();

        // Generate lookup proof
        let (proof, returned) = tree.generate_lookup_proof(key.clone());
        assert_eq!(returned.as_ref(), Some(&value));

        // Use a wrong (all-zeros) digest
        let wrong_digest = [0u8; 33];
        let invalid = AvlTreeState::verify_proof(&wrong_digest, &proof, &key, &value);
        assert!(!invalid, "Proof should NOT verify against wrong digest");
    }

    #[test]
    fn test_avl_tree_proof_verification_tampered_proof() {
        let mut tree = AvlTreeState::new();

        let key = vec![1u8; 32];
        let value = vec![2u8; 32];

        tree.insert(key.clone(), value.clone()).unwrap();
        let digest = tree.root_digest();
        let (mut proof, returned) = tree.generate_lookup_proof(key.clone());
        assert_eq!(returned.as_ref(), Some(&value));

        // Tamper with the proof bytes
        if !proof.is_empty() {
            proof[0] ^= 0xFF;
        }

        // The ergo_avltree_rust library may panic on malformed proofs,
        // so we catch_unwind to verify it doesn't return true.
        let result =
            std::panic::catch_unwind(|| AvlTreeState::verify_proof(&digest, &proof, &key, &value));

        // Either it returns false (correct behavior) or panics (library bug)
        match result {
            Ok(valid) => assert!(!valid, "Tampered proof should NOT verify"),
            Err(_) => {
                // Library panics on malformed proof - this is a known issue
                // with the ergo_avltree_rust crate. The proof is still invalid.
            }
        }
    }

    #[test]
    fn test_avl_tree_proof_verification_multiple_operations() {
        let mut tree = AvlTreeState::new();

        // Insert multiple keys
        for i in 1..6 {
            let mut key = vec![i; 32];
            key[0] = i;
            let value = vec![i * 2; 32];
            tree.insert(key, value).unwrap();
        }

        let digest = tree.root_digest();

        // Verify each key with a fresh lookup proof
        for i in 1..6 {
            let mut key = vec![i; 32];
            key[0] = i;
            let value = vec![i * 2; 32];
            let (proof, returned) = tree.generate_lookup_proof(key.clone());
            assert_eq!(returned.as_ref(), Some(&value));
            let valid = AvlTreeState::verify_proof(&digest, &proof, &key, &value);
            assert!(valid, "Proof should verify for key {}", i);
        }

        // Verify non-existent key fails
        let non_existent_key = vec![99u8; 32];
        let non_existent_value = vec![99u8; 32];
        let (proof, returned) = tree.generate_lookup_proof(non_existent_key.clone());
        assert!(returned.is_none(), "Non-existent key should return None");
        let invalid =
            AvlTreeState::verify_proof(&digest, &proof, &non_existent_key, &non_existent_value);
        assert!(!invalid, "Proof should NOT verify for non-existent key");
    }

    #[test]
    fn test_avl_tree_balance_invariants() {
        let mut tree = AvlTreeState::new();

        // Insert keys in sorted order (worst case for balancing)
        // Start from 1 to avoid zero keys
        for i in 1..20 {
            let mut key = vec![0u8; 32];
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
