//! AVL+ tree implementation for Basis tracker state commitments

use crate::state::TrackerState;
use crate::errors::TreeError;

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
            },
            Err(_) => {
                // Update failed, try insert instead
                let insert_op = Operation::Insert(KeyValue {
                    key: key.clone().into(),
                    value: value.clone().into(),
                });

                self.prover
                    .perform_one_operation(&insert_op)
                    .map_err(|e| TreeError::StorageError(format!("AVL tree operation failed: {:?}", e)))?;

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
        use ergo_avltree_rust::authenticated_tree_ops::AuthenticatedTreeOps;
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
        use ergo_avltree_rust::batch_avl_verifier::BatchAVLVerifier;
        use bytes::Bytes;

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

