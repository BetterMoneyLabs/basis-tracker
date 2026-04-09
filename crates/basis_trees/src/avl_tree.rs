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
    panic!("Tree resolver called - this should not happen with in-memory trees");
}

impl BasisAvlTree {
    /// Create a new in-memory AVL tree
    pub fn new() -> Result<Self, TreeError> {
        // Create an AVL tree with variable length values
        // Key length: 64 bytes (issuer_hash + recipient_hash)
        // Value length: None for variable length values
        let tree = AVLTree::new(tree_resolver, 64, None);
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
            .as_secs();
    }


}

