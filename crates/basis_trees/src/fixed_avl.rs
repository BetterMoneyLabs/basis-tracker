//! Fixed-shape authenticated trees for the Basis v2 ABI.
//!
//! The legacy tree wrapper accepts variable-length values and rebuilds proof
//! candidates from a `HashMap`. Basis v2 instead authenticates exact 32/8 and
//! 32/24 tree shapes, and its root depends on first-insertion order. This
//! wrapper makes both constraints explicit and never exposes removal.

use crate::TreeError;
use bytes::Bytes;
use ergo_avltree_rust::{
    authenticated_tree_ops::AuthenticatedTreeOps,
    batch_avl_prover::BatchAVLProver,
    batch_avl_verifier::BatchAVLVerifier,
    batch_node::AVLTree,
    operation::{KeyValue, Operation},
};
use std::collections::HashMap;

pub const BASIS_V2_KEY_LENGTH: usize = 32;

fn tree_resolver(_digest: &[u8; 32]) -> ergo_avltree_rust::batch_node::Node {
    ergo_avltree_rust::batch_node::Node::Leaf(ergo_avltree_rust::batch_node::LeafNode {
        hdr: ergo_avltree_rust::batch_node::NodeHeader {
            visited: false,
            is_new: false,
            label: None,
            key: Some(ergo_avltree_rust::operation::ADKey::from(vec![
                0u8;
                BASIS_V2_KEY_LENGTH
            ])),
        },
        value: ergo_avltree_rust::operation::ADValue::from(vec![]),
        next_node_key: ergo_avltree_rust::operation::ADKey::from(vec![0u8; BASIS_V2_KEY_LENGTH]),
    })
}

/// ErgoScript metadata authenticated by both Basis v2 reserve families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedTreeShape {
    key_length: usize,
    value_length: usize,
    insert_allowed: bool,
    update_allowed: bool,
    remove_allowed: bool,
}

impl FixedTreeShape {
    pub const fn key_length(&self) -> usize {
        self.key_length
    }

    pub const fn value_length(&self) -> usize {
        self.value_length
    }

    pub const fn insert_allowed(&self) -> bool {
        self.insert_allowed
    }

    pub const fn update_allowed(&self) -> bool {
        self.update_allowed
    }

    pub const fn remove_allowed(&self) -> bool {
        self.remove_allowed
    }
}

/// Mandatory membership or non-membership lookup evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupWitness<const VALUE_LEN: usize> {
    proof: Vec<u8>,
    value: Option<[u8; VALUE_LEN]>,
}

impl<const VALUE_LEN: usize> LookupWitness<VALUE_LEN> {
    pub fn proof(&self) -> &[u8] {
        &self.proof
    }

    pub const fn value(&self) -> Option<[u8; VALUE_LEN]> {
        self.value
    }
}

/// An insert-or-update proof and the root it produces without mutating state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionWitness {
    proof: Vec<u8>,
    new_digest: [u8; 33],
}

impl TransitionWitness {
    pub fn proof(&self) -> &[u8] {
        &self.proof
    }

    pub const fn new_digest(&self) -> [u8; 33] {
        self.new_digest
    }
}

/// In-memory fixed-width AVL prover with stable first-insertion ordering.
///
/// Persistence owns the ordered entry vector. Reopening the tree must replay
/// exactly that vector; sorting a snapshot or iterating a hash map is not an
/// equivalent reconstruction.
pub struct FixedAvlTree<const VALUE_LEN: usize> {
    prover: BatchAVLProver,
    ordered_entries: Vec<([u8; BASIS_V2_KEY_LENGTH], [u8; VALUE_LEN])>,
    positions: HashMap<[u8; BASIS_V2_KEY_LENGTH], usize>,
}

impl<const VALUE_LEN: usize> FixedAvlTree<VALUE_LEN> {
    pub fn new() -> Result<Self, TreeError> {
        if VALUE_LEN != 8 && VALUE_LEN != 24 {
            return Err(TreeError::InvalidState);
        }
        Ok(Self {
            prover: Self::empty_prover(),
            ordered_entries: Vec::new(),
            positions: HashMap::new(),
        })
    }

    pub fn from_ordered_entries<I>(entries: I) -> Result<Self, TreeError>
    where
        I: IntoIterator<Item = ([u8; BASIS_V2_KEY_LENGTH], [u8; VALUE_LEN])>,
    {
        let mut tree = Self::new()?;
        for (key, value) in entries {
            tree.insert(key, value)?;
        }
        Ok(tree)
    }

    pub const fn shape() -> FixedTreeShape {
        FixedTreeShape {
            key_length: BASIS_V2_KEY_LENGTH,
            value_length: VALUE_LEN,
            insert_allowed: true,
            update_allowed: true,
            remove_allowed: false,
        }
    }

    pub fn len(&self) -> usize {
        self.ordered_entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ordered_entries.is_empty()
    }

    pub fn get(&self, key: &[u8; BASIS_V2_KEY_LENGTH]) -> Option<[u8; VALUE_LEN]> {
        self.positions
            .get(key)
            .map(|position| self.ordered_entries[*position].1)
    }

    pub fn ordered_entries(
        &self,
    ) -> impl ExactSizeIterator<Item = &([u8; BASIS_V2_KEY_LENGTH], [u8; VALUE_LEN])> {
        self.ordered_entries.iter()
    }

    pub fn root_digest(&self) -> Result<[u8; 33], TreeError> {
        let mut digest = [0u8; 33];
        let value = self.prover.digest().ok_or(TreeError::InvalidState)?;
        digest.copy_from_slice(&value);
        Ok(digest)
    }

    pub fn insert(
        &mut self,
        key: [u8; BASIS_V2_KEY_LENGTH],
        value: [u8; VALUE_LEN],
    ) -> Result<(), TreeError> {
        if self.positions.contains_key(&key) {
            return Err(TreeError::DuplicateKey);
        }
        self.perform_insert_or_update(key, value)?;
        let position = self.ordered_entries.len();
        self.ordered_entries.push((key, value));
        self.positions.insert(key, position);
        Ok(())
    }

    pub fn update(
        &mut self,
        key: [u8; BASIS_V2_KEY_LENGTH],
        value: [u8; VALUE_LEN],
    ) -> Result<(), TreeError> {
        let position = *self.positions.get(&key).ok_or(TreeError::KeyNotFound)?;
        self.perform_insert_or_update(key, value)?;
        self.ordered_entries[position].1 = value;
        Ok(())
    }

    /// Generate mandatory lookup evidence for either an existing or absent key.
    pub fn lookup_witness(
        &mut self,
        key: [u8; BASIS_V2_KEY_LENGTH],
    ) -> Result<LookupWitness<VALUE_LEN>, TreeError> {
        // Flush prior modifications so the returned proof contains one lookup.
        let _ = self.prover.generate_proof();
        let result = self
            .prover
            .perform_one_operation(&Operation::Lookup(key.to_vec().into()))
            .map_err(|error| TreeError::StorageError(format!("AVL lookup failed: {error:?}")))?;
        let proof = self.prover.generate_proof().to_vec();
        if proof.is_empty() {
            return Err(TreeError::InvalidState);
        }
        let value = match result {
            Some(bytes) => Some(
                bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| TreeError::TreeCorruption)?,
            ),
            None => None,
        };
        Ok(LookupWitness { proof, value })
    }

    /// Generate the v2 `insertOrUpdate` proof against the current root.
    ///
    /// The real tree is not mutated. Rebuilding the temporary prover uses the
    /// authoritative first-insertion order, never hash-map iteration order.
    pub fn transition_witness(
        &self,
        key: [u8; BASIS_V2_KEY_LENGTH],
        value: [u8; VALUE_LEN],
    ) -> Result<TransitionWitness, TreeError> {
        let mut prover = Self::empty_prover();
        for (existing_key, existing_value) in &self.ordered_entries {
            prover
                .perform_one_operation(&Operation::Insert(KeyValue {
                    key: existing_key.to_vec().into(),
                    value: existing_value.to_vec().into(),
                }))
                .map_err(|error| {
                    TreeError::StorageError(format!("AVL rebuild failed: {error:?}"))
                })?;
        }
        let _ = prover.generate_proof();
        prover
            .perform_one_operation(&Operation::InsertOrUpdate(KeyValue {
                key: key.to_vec().into(),
                value: value.to_vec().into(),
            }))
            .map_err(|error| {
                TreeError::StorageError(format!("AVL transition failed: {error:?}"))
            })?;
        let proof = prover.generate_proof().to_vec();
        if proof.is_empty() {
            return Err(TreeError::InvalidState);
        }
        let mut new_digest = [0u8; 33];
        let digest = prover.digest().ok_or(TreeError::InvalidState)?;
        new_digest.copy_from_slice(&digest);
        Ok(TransitionWitness { proof, new_digest })
    }

    /// Verify exactly one membership or non-membership lookup operation.
    pub fn verify_lookup(
        starting_digest: &[u8; 33],
        key: &[u8; BASIS_V2_KEY_LENGTH],
        witness: &LookupWitness<VALUE_LEN>,
    ) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::verify_lookup_inner(starting_digest, key, witness)
        }))
        .unwrap_or(false)
    }

    fn verify_lookup_inner(
        starting_digest: &[u8; 33],
        key: &[u8; BASIS_V2_KEY_LENGTH],
        witness: &LookupWitness<VALUE_LEN>,
    ) -> bool {
        let mut verifier = match BatchAVLVerifier::new(
            &Bytes::copy_from_slice(starting_digest),
            &Bytes::copy_from_slice(&witness.proof),
            AVLTree::new(tree_resolver, BASIS_V2_KEY_LENGTH, Some(VALUE_LEN)),
            Some(1),
            Some(0),
        ) {
            Ok(verifier) => verifier,
            Err(_) => return false,
        };
        let actual = match verifier.perform_one_operation(&Operation::Lookup(key.to_vec().into())) {
            Ok(value) => value,
            Err(_) => return false,
        };
        match (actual, witness.value) {
            (None, None) => true,
            (Some(actual), Some(expected)) => actual.as_ref() == expected,
            _ => false,
        }
    }

    fn empty_prover() -> BatchAVLProver {
        BatchAVLProver::new(
            AVLTree::new(tree_resolver, BASIS_V2_KEY_LENGTH, Some(VALUE_LEN)),
            true,
        )
    }

    fn perform_insert_or_update(
        &mut self,
        key: [u8; BASIS_V2_KEY_LENGTH],
        value: [u8; VALUE_LEN],
    ) -> Result<(), TreeError> {
        self.prover
            .perform_one_operation(&Operation::InsertOrUpdate(KeyValue {
                key: key.to_vec().into(),
                value: value.to_vec().into(),
            }))
            .map_err(|error| {
                TreeError::StorageError(format!("AVL transition failed: {error:?}"))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_exact_v2_shapes() {
        assert_eq!(
            FixedAvlTree::<8>::shape(),
            FixedTreeShape {
                key_length: 32,
                value_length: 8,
                insert_allowed: true,
                update_allowed: true,
                remove_allowed: false,
            }
        );
        assert_eq!(FixedAvlTree::<24>::shape().value_length(), 24);
        assert!(matches!(
            FixedAvlTree::<0>::new(),
            Err(TreeError::InvalidState)
        ));
        assert!(matches!(
            FixedAvlTree::<16>::new(),
            Err(TreeError::InvalidState)
        ));
    }

    #[test]
    fn insertion_order_is_explicit_and_replayable() {
        let entries = [([1u8; 32], [10u8; 8]), ([2u8; 32], [20u8; 8])];
        let tree = FixedAvlTree::<8>::from_ordered_entries(entries).unwrap();
        let replay = FixedAvlTree::<8>::from_ordered_entries(entries).unwrap();
        assert_eq!(tree.root_digest().unwrap(), replay.root_digest().unwrap());
        assert_eq!(tree.ordered_entries().copied().collect::<Vec<_>>(), entries);
    }

    #[test]
    fn insert_and_update_are_strict() {
        let mut tree = FixedAvlTree::<8>::new().unwrap();
        tree.insert([1u8; 32], [2u8; 8]).unwrap();
        assert!(matches!(
            tree.insert([1u8; 32], [3u8; 8]),
            Err(TreeError::DuplicateKey)
        ));
        assert!(matches!(
            tree.update([9u8; 32], [3u8; 8]),
            Err(TreeError::KeyNotFound)
        ));
        tree.update([1u8; 32], [4u8; 8]).unwrap();
        assert_eq!(tree.get(&[1u8; 32]), Some([4u8; 8]));
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn membership_and_non_membership_both_have_verifiable_proofs() {
        let mut tree = FixedAvlTree::<24>::new().unwrap();
        tree.insert([1u8; 32], [2u8; 24]).unwrap();
        let digest = tree.root_digest().unwrap();

        let membership = tree.lookup_witness([1u8; 32]).unwrap();
        assert_eq!(membership.value, Some([2u8; 24]));
        assert!(!membership.proof.is_empty());
        assert!(FixedAvlTree::<24>::verify_lookup(
            &digest,
            &[1u8; 32],
            &membership
        ));

        let non_membership = tree.lookup_witness([9u8; 32]).unwrap();
        assert_eq!(non_membership.value, None);
        assert!(!non_membership.proof.is_empty());
        assert!(FixedAvlTree::<24>::verify_lookup(
            &digest,
            &[9u8; 32],
            &non_membership
        ));

        let mut wrong = non_membership.clone();
        wrong.value = Some([0u8; 24]);
        assert!(!FixedAvlTree::<24>::verify_lookup(
            &digest, &[9u8; 32], &wrong
        ));

        let mut wrong_proof = membership.clone();
        wrong_proof.proof[0] ^= 1;
        assert!(!FixedAvlTree::<24>::verify_lookup(
            &digest,
            &[1u8; 32],
            &wrong_proof
        ));
        assert!(!FixedAvlTree::<24>::verify_lookup(
            &digest,
            &[3u8; 32],
            &membership
        ));
        let mut wrong_digest = digest;
        wrong_digest[1] ^= 1;
        assert!(!FixedAvlTree::<24>::verify_lookup(
            &wrong_digest,
            &[1u8; 32],
            &membership
        ));
    }

    #[test]
    fn transition_proof_is_deterministic_and_does_not_mutate() {
        let mut tree = FixedAvlTree::<8>::new().unwrap();
        tree.insert([1u8; 32], [2u8; 8]).unwrap();
        tree.insert([3u8; 32], [4u8; 8]).unwrap();
        let before = tree.root_digest().unwrap();

        let witness = tree.transition_witness([1u8; 32], [5u8; 8]).unwrap();
        let again = tree.transition_witness([1u8; 32], [5u8; 8]).unwrap();
        assert_eq!(witness, again);
        assert_eq!(tree.root_digest().unwrap(), before);

        tree.update([1u8; 32], [5u8; 8]).unwrap();
        assert_eq!(tree.root_digest().unwrap(), witness.new_digest());
    }
}
