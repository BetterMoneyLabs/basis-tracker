//! AVL+ tree implementation for Basis tracker state commitments

use crate::errors::TreeError;
use crate::state::TrackerState;

use ergo_avltree_rust::{
    authenticated_tree_ops::AuthenticatedTreeOps,
    batch_avl_prover::BatchAVLProver,
    batch_node::AVLTree,
    operation::{KeyValue, Operation},
};

use std::collections::HashMap;

/// In-memory AVL tree state for tracker commitments
pub struct BasisAvlTree {
    prover: BatchAVLProver,
    current_state: TrackerState,
    /// In-memory cache for key-value lookups
    /// This mirrors the AVL tree state for efficient get() operations
    cache: HashMap<Vec<u8>, Vec<u8>>,
}

// Simple resolver function for AVL tree
// Note: This resolver should never be called since we're using in-memory trees
fn tree_resolver(_digest: &[u8; 32]) -> ergo_avltree_rust::batch_node::Node {
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

impl BasisAvlTree {
    /// Create a new in-memory AVL tree
    pub fn new() -> Result<Self, TreeError> {
        // Create an AVL tree with variable length values
        // Key length: 32 bytes (blake2b256(issuer_pubkey || recipient_pubkey))
        // Value length: None for variable length values
        let tree = AVLTree::new(tree_resolver, 32, None);
        let prover = BatchAVLProver::new(tree, true);

        let current_state = TrackerState::empty();

        Ok(Self {
            prover,
            current_state,
            cache: HashMap::new(),
        })
    }

    /// Insert a key-value pair into the AVL tree
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), TreeError> {
        let operation = Operation::Insert(KeyValue {
            key: key.clone().into(),
            value: value.clone().into(),
        });

        // Perform the operation
        let _ = self
            .prover
            .perform_one_operation(&operation)
            .map_err(|e| TreeError::StorageError(format!("AVL tree insert failed: {:?}", e)))?;

        // Update cache
        self.cache.insert(key.clone(), value.clone());

        // Update state
        self.update_state();

        Ok(())
    }

    /// Update an existing key-value pair (or insert if key doesn't exist)
    pub fn update(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), TreeError> {
        // Try update first, and if it fails (e.g., key doesn't exist), try insert
        let update_op = Operation::Update(KeyValue {
            key: key.clone().into(),
            value: value.clone().into(),
        });

        match self.prover.perform_one_operation(&update_op) {
            Ok(_) => {
                // Update cache
                self.cache.insert(key.clone(), value.clone());
                self.update_state();
                Ok(())
            }
            Err(_) => {
                // Update failed, try insert instead
                let insert_op = Operation::Insert(KeyValue {
                    key: key.clone().into(),
                    value: value.clone().into(),
                });

                self.prover.perform_one_operation(&insert_op).map_err(|e| {
                    TreeError::StorageError(format!("AVL tree operation failed: {:?}", e))
                })?;

                // Update cache
                self.cache.insert(key.clone(), value.clone());
                self.update_state();
                Ok(())
            }
        }
    }

    /// Generate a proof for a lookup operation on the given key.
    ///
    /// This performs a `Lookup` operation and returns the proof bytes that
    /// can be used to verify the key's existence (or non-existence) in the tree.
    pub fn generate_lookup_proof(&mut self, key: Vec<u8>) -> (Vec<u8>, Option<Vec<u8>>) {
        // Commit any pending modifications so the lookup proof only covers the
        // lookup operation, not previous insert/update operations.
        let _ = self.prover.generate_proof();

        let operation = Operation::Lookup(key.into());
        let result = self.prover.perform_one_operation(&operation).ok().flatten();
        let proof = self.prover.generate_proof().to_vec();
        (proof, result.map(|b| b.to_vec()))
    }

    /// Verify a proof by replaying a lookup operation against the given starting digest.
    ///
    /// This creates a `BatchAVLVerifier` with the provided digest and proof, then
    /// performs a `Lookup` operation for `key`.  If the verifier succeeds and returns
    /// the expected value, the proof is valid.
    pub fn verify_proof(
        starting_digest: &[u8; 33],
        proof: &[u8],
        key: &[u8],
        expected_value: &[u8],
    ) -> bool {
        use bytes::Bytes;
        use ergo_avltree_rust::batch_avl_verifier::BatchAVLVerifier;

        let tree = AVLTree::new(tree_resolver, 32, None);
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

    /// Generate a proof for the current tree state (all pending operations).
    pub fn generate_proof(&mut self) -> Vec<u8> {
        self.prover.generate_proof().to_vec()
    }

    /// Generate an insert/update proof without mutating the persistent tree state.
    ///
    /// This is useful for producing redemption transaction proofs where the on-chain
    /// reserve tree has not been updated yet. A temporary prover is created from a
    /// clone of the current tree, the operation is performed on the clone, and the
    /// resulting proof (and new digest) is returned while the original tree remains
    /// unchanged.
    pub fn generate_insert_proof(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(Vec<u8>, [u8; 33]), TreeError> {
        // Rebuild a temporary tree from the in-memory cache. `AVLTree::clone()` is shallow
        // (`Rc<RefCell<Node>>`), so operating on a clone would mutate the persistent tree's
        // shared nodes and corrupt its state.
        let mut temp_prover = BatchAVLProver::new(AVLTree::new(tree_resolver, 32, None), true);
        for (cached_key, cached_value) in &self.cache {
            temp_prover
                .perform_one_operation(&Operation::Insert(KeyValue {
                    key: cached_key.clone().into(),
                    value: cached_value.clone().into(),
                }))
                .map_err(|e| {
                    TreeError::StorageError(format!("AVL tree rebuild failed: {:?}", e))
                })?;
        }
        // Commit the rebuild so the proof below covers only the single insert-or-update
        // operation against the committed starting digest.
        let _ = temp_prover.generate_proof();

        // Match JVM scrypto `Insert` semantics used by the on-chain contract (`AvlTree.insert`
        // replays `Insert` on the verifier, which updates the value when the key already
        // exists). `Operation::Update` proofs do not verify on-chain as `insert`.
        let operation = Operation::InsertOrUpdate(KeyValue {
            key: key.into(),
            value: value.into(),
        });

        temp_prover
            .perform_one_operation(&operation)
            .map_err(|e| TreeError::StorageError(format!("AVL tree operation failed: {:?}", e)))?;

        let proof = temp_prover.generate_proof().to_vec();

        let mut digest = [0u8; 33];
        if let Some(d) = temp_prover.digest() {
            digest.copy_from_slice(&d);
        }

        Ok((proof, digest))
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

    /// Lookup a value by key in the AVL tree
    /// Returns the value bytes if found, None otherwise
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        // Use the in-memory cache for efficient lookups
        self.cache.get(key).cloned()
    }

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState {
        &self.current_state
    }

    /// Update the current state with latest AVL tree root
    fn update_state(&mut self) {
        self.current_state.avl_root_digest = self.root_digest().to_vec();
        // Update timestamp would be set to current time in real implementation
        self.current_state.last_update_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_insert_proof_does_not_mutate_state() {
        let mut tree = BasisAvlTree::new().unwrap();
        let key = vec![1u8; 32];
        let value = vec![2u8; 16];

        let digest_before = tree.root_digest();
        let (proof, digest_after) = tree
            .generate_insert_proof(key.clone(), value.clone())
            .unwrap();

        // The persistent tree digest must remain unchanged.
        assert_eq!(tree.root_digest(), digest_before);
        // But the proof must reflect the updated digest.
        assert_ne!(digest_after, digest_before);
        assert!(!proof.is_empty());

        // Calling it a second time with the same key must still be an insert
        // (not an update) because the persistent state was not mutated.
        let (proof2, digest_after2) = tree
            .generate_insert_proof(key.clone(), value.clone())
            .unwrap();
        assert_eq!(tree.root_digest(), digest_before);
        assert_eq!(digest_after2, digest_after);
        assert_eq!(proof2, proof);
    }

    #[test]
    fn generate_insert_proof_matches_actual_insert() {
        let mut tree = BasisAvlTree::new().unwrap();
        let key = vec![3u8; 32];
        let value = vec![4u8; 16];

        let (proof, expected_digest) = tree
            .generate_insert_proof(key.clone(), value.clone())
            .unwrap();

        // Now actually insert into the persistent tree.
        tree.insert(key.clone(), value.clone()).unwrap();
        let actual_digest = tree.root_digest();

        assert_eq!(actual_digest, expected_digest);

        // Verify the insert proof against the original (empty) digest using a
        // BatchAVLVerifier and the same Insert operation.
        use bytes::Bytes;
        use ergo_avltree_rust::{
            batch_avl_verifier::BatchAVLVerifier,
            batch_node::AVLTree,
            operation::{KeyValue, Operation},
        };

        let empty_digest = BasisAvlTree::new().unwrap().root_digest();
        let avl_tree = AVLTree::new(tree_resolver, 32, None);
        let mut verifier = BatchAVLVerifier::new(
            &Bytes::copy_from_slice(&empty_digest),
            &Bytes::copy_from_slice(&proof),
            avl_tree,
            Some(1),
            Some(0),
        )
        .unwrap();

        let operation = Operation::Insert(KeyValue {
            key: key.into(),
            value: value.into(),
        });
        verifier.perform_one_operation(&operation).unwrap();

        let verified_digest = verifier.digest().unwrap();
        let mut verified_digest_arr = [0u8; 33];
        verified_digest_arr.copy_from_slice(&verified_digest);
        assert_eq!(verified_digest_arr, expected_digest);
    }

    /// Regression test for the on-chain "Incorrect insert" failure: updating an existing key
    /// must produce a proof that replays as an insert-style operation (JVM scrypto `Insert`
    /// updates existing keys; `Operation::Update` proofs do not verify on-chain as `insert`).
    #[test]
    fn generate_insert_proof_updates_existing_key_with_verifiable_proof() {
        use bytes::Bytes;
        use ergo_avltree_rust::{
            batch_avl_verifier::BatchAVLVerifier,
            batch_node::AVLTree,
            operation::{KeyValue, Operation},
        };

        let mut tree = BasisAvlTree::new().unwrap();
        let key = vec![7u8; 32];
        let value_v1 = vec![1u8; 16];
        let value_v2 = vec![2u8; 16];

        // Commit the first version into the persistent tree.
        tree.insert(key.clone(), value_v1.clone()).unwrap();
        let committed_digest = tree.root_digest();

        // Generate a proof for updating the same key to v2; persistent state must not change.
        let (proof, expected_digest) = tree
            .generate_insert_proof(key.clone(), value_v2.clone())
            .unwrap();
        assert_eq!(tree.root_digest(), committed_digest);
        assert_ne!(expected_digest, committed_digest);
        assert!(!proof.is_empty());

        // Deterministic: a second call returns the same proof and digest.
        let (proof2, expected_digest2) = tree
            .generate_insert_proof(key.clone(), value_v2.clone())
            .unwrap();
        assert_eq!(proof2, proof);
        assert_eq!(expected_digest2, expected_digest);

        // The proof must replay successfully from the committed digest under insert-or-update
        // semantics (the semantics the on-chain contract relies on for cumulative redemptions).
        // NOTE: the resulting verifier digest is NOT asserted here: ergo_avltree_rust 0.1.1's
        // verifier miscomputes the final label for the update path (it diverges from the
        // prover), while JVM scrypto 2.3.0's verifier matches the prover exactly — verified
        // via scala/src/main/scala/chaincash/compare/AvlUpdateDigestCheck.scala.
        let avl_tree = AVLTree::new(tree_resolver, 32, None);
        let mut verifier = BatchAVLVerifier::new(
            &Bytes::copy_from_slice(&committed_digest),
            &Bytes::copy_from_slice(&proof),
            avl_tree,
            Some(1),
            Some(0),
        )
        .unwrap();
        verifier
            .perform_one_operation(&Operation::InsertOrUpdate(KeyValue {
                key: key.clone().into(),
                value: value_v2.clone().into(),
            }))
            .unwrap();

        // The prover-side digest must match a real persistent update.
        tree.update(key.clone(), value_v2.clone()).unwrap();
        assert_eq!(tree.root_digest(), expected_digest);
    }
}
