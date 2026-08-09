//! Fixed-shape authenticated trees for the Basis v2 ABI.
//!
//! The legacy tree wrapper accepts variable-length values and rebuilds proof
//! candidates from a `HashMap`. Basis v2 instead authenticates exact 32/8 and
//! 32/24 tree shapes, and its root depends on first-insertion order. This
//! wrapper makes both constraints explicit and never exposes removal.
//!
//! Unsupported value widths are absent from the public type surface:
//!
//! ```compile_fail
//! use basis_trees::TrackerAvlTree;
//! let _ = TrackerAvlTree::<16>::new();
//! ```

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

fn tree_resolver(digest: &[u8; 32]) -> ergo_avltree_rust::batch_node::Node {
    use ergo_avltree_rust::batch_node::{Node, NodeHeader};
    // Match Sigma's verifier resolver exactly. Replacing an unresolved label
    // with a synthetic leaf changes an existing-key update digest even though
    // first-insertion fixtures may still appear to verify.
    Node::LabelOnly(NodeHeader::new(Some(*digest), None))
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

/// Internal membership or non-membership lookup evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LookupWitness<const VALUE_LEN: usize> {
    proof: Vec<u8>,
    value: Option<[u8; VALUE_LEN]>,
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
struct FixedAvlInner<const VALUE_LEN: usize> {
    prover: BatchAVLProver,
    ordered_entries: Vec<([u8; BASIS_V2_KEY_LENGTH], [u8; VALUE_LEN])>,
    positions: HashMap<[u8; BASIS_V2_KEY_LENGTH], usize>,
}

impl<const VALUE_LEN: usize> FixedAvlInner<VALUE_LEN> {
    fn new() -> Self {
        Self {
            prover: Self::empty_prover(),
            ordered_entries: Vec::new(),
            positions: HashMap::new(),
        }
    }

    fn from_ordered_entries<I>(entries: I) -> Result<Self, TreeError>
    where
        I: IntoIterator<Item = ([u8; BASIS_V2_KEY_LENGTH], [u8; VALUE_LEN])>,
    {
        let mut tree = Self::new();
        for (key, value) in entries {
            tree.insert(key, value)?;
        }
        Ok(tree)
    }

    const fn shape() -> FixedTreeShape {
        FixedTreeShape {
            key_length: BASIS_V2_KEY_LENGTH,
            value_length: VALUE_LEN,
            insert_allowed: true,
            update_allowed: true,
            remove_allowed: false,
        }
    }

    fn len(&self) -> usize {
        self.ordered_entries.len()
    }

    fn is_empty(&self) -> bool {
        self.ordered_entries.is_empty()
    }

    fn get(&self, key: &[u8; BASIS_V2_KEY_LENGTH]) -> Option<[u8; VALUE_LEN]> {
        self.positions
            .get(key)
            .map(|position| self.ordered_entries[*position].1)
    }

    fn ordered_entries(
        &self,
    ) -> impl ExactSizeIterator<Item = &([u8; BASIS_V2_KEY_LENGTH], [u8; VALUE_LEN])> {
        self.ordered_entries.iter()
    }

    fn root_digest(&self) -> Result<[u8; 33], TreeError> {
        let mut digest = [0u8; 33];
        let value = self.prover.digest().ok_or(TreeError::InvalidState)?;
        digest.copy_from_slice(&value);
        Ok(digest)
    }

    fn insert(
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

    fn update(
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
    fn lookup_witness(
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
    fn transition_witness(
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
    fn verify_lookup(
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

    /// Verify one raw insert-or-update proof and bind the resulting digest.
    ///
    /// This is the signer-side counterpart of `transition_witness`: a CLI can
    /// replay the exact context variable supplied by a remote builder without
    /// trusting the builder's claimed successor R5.
    fn verify_transition(
        starting_digest: &[u8; 33],
        key: &[u8; BASIS_V2_KEY_LENGTH],
        value: &[u8; VALUE_LEN],
        proof: &[u8],
        expected_digest: &[u8; 33],
    ) -> bool {
        if proof.is_empty() {
            return false;
        }
        let mut verifier = match BatchAVLVerifier::new(
            &Bytes::copy_from_slice(starting_digest),
            &Bytes::copy_from_slice(proof),
            AVLTree::new(tree_resolver, BASIS_V2_KEY_LENGTH, Some(VALUE_LEN)),
            Some(1),
            Some(0),
        ) {
            Ok(verifier) => verifier,
            Err(_) => return false,
        };
        if verifier
            .perform_one_operation(&Operation::InsertOrUpdate(KeyValue {
                key: key.to_vec().into(),
                value: value.to_vec().into(),
            }))
            .is_err()
        {
            return false;
        }
        verifier
            .digest()
            .map(|digest| digest.as_ref() == expected_digest)
            .unwrap_or(false)
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

macro_rules! define_fixed_tree {
    ($tree:ident, $witness:ident, $value_len:literal, $description:literal) => {
        #[doc = $description]
        pub struct $tree {
            inner: FixedAvlInner<$value_len>,
        }

        #[doc = concat!("Mandatory membership or non-membership evidence for `", stringify!($tree), "`.")]
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $witness {
            inner: LookupWitness<$value_len>,
        }

        impl $witness {
            pub fn proof(&self) -> &[u8] {
                &self.inner.proof
            }

            pub const fn value(&self) -> Option<[u8; $value_len]> {
                self.inner.value
            }
        }

        impl $tree {
            pub fn new() -> Self {
                Self {
                    inner: FixedAvlInner::new(),
                }
            }

            pub fn from_ordered_entries<I>(entries: I) -> Result<Self, TreeError>
            where
                I: IntoIterator<
                    Item = ([u8; BASIS_V2_KEY_LENGTH], [u8; $value_len]),
                >,
            {
                Ok(Self {
                    inner: FixedAvlInner::from_ordered_entries(entries)?,
                })
            }

            pub const fn shape() -> FixedTreeShape {
                FixedAvlInner::<$value_len>::shape()
            }

            pub fn len(&self) -> usize {
                self.inner.len()
            }

            pub fn is_empty(&self) -> bool {
                self.inner.is_empty()
            }

            pub fn get(
                &self,
                key: &[u8; BASIS_V2_KEY_LENGTH],
            ) -> Option<[u8; $value_len]> {
                self.inner.get(key)
            }

            pub fn ordered_entries(
                &self,
            ) -> impl ExactSizeIterator<
                Item = &([u8; BASIS_V2_KEY_LENGTH], [u8; $value_len]),
            > {
                self.inner.ordered_entries()
            }

            pub fn root_digest(&self) -> Result<[u8; 33], TreeError> {
                self.inner.root_digest()
            }

            pub fn insert(
                &mut self,
                key: [u8; BASIS_V2_KEY_LENGTH],
                value: [u8; $value_len],
            ) -> Result<(), TreeError> {
                self.inner.insert(key, value)
            }

            pub fn update(
                &mut self,
                key: [u8; BASIS_V2_KEY_LENGTH],
                value: [u8; $value_len],
            ) -> Result<(), TreeError> {
                self.inner.update(key, value)
            }

            pub fn lookup_witness(
                &mut self,
                key: [u8; BASIS_V2_KEY_LENGTH],
            ) -> Result<$witness, TreeError> {
                Ok($witness {
                    inner: self.inner.lookup_witness(key)?,
                })
            }

            pub fn transition_witness(
                &self,
                key: [u8; BASIS_V2_KEY_LENGTH],
                value: [u8; $value_len],
            ) -> Result<TransitionWitness, TreeError> {
                self.inner.transition_witness(key, value)
            }

            /// Verify exactly one lookup without unwinding on malformed proof bytes.
            pub fn verify_lookup(
                starting_digest: &[u8; 33],
                key: &[u8; BASIS_V2_KEY_LENGTH],
                witness: &$witness,
            ) -> bool {
                FixedAvlInner::<$value_len>::verify_lookup(
                    starting_digest,
                    key,
                    &witness.inner,
                )
            }

            /// Verify raw mandatory membership or non-membership evidence.
            pub fn verify_lookup_bytes(
                starting_digest: &[u8; 33],
                key: &[u8; BASIS_V2_KEY_LENGTH],
                proof: &[u8],
                value: Option<[u8; $value_len]>,
            ) -> bool {
                let witness = $witness {
                    inner: LookupWitness {
                        proof: proof.to_vec(),
                        value,
                    },
                };
                Self::verify_lookup(starting_digest, key, &witness)
            }

            /// Verify a raw insert-or-update proof and its exact successor root.
            pub fn verify_transition_bytes(
                starting_digest: &[u8; 33],
                key: &[u8; BASIS_V2_KEY_LENGTH],
                value: &[u8; $value_len],
                proof: &[u8],
                expected_digest: &[u8; 33],
            ) -> bool {
                FixedAvlInner::<$value_len>::verify_transition(
                    starting_digest,
                    key,
                    value,
                    proof,
                    expected_digest,
                )
            }
        }

        impl Default for $tree {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

define_fixed_tree!(
    TrackerAvlTree,
    TrackerLookupWitness,
    8,
    "Basis v2 tracker debt tree with exact 32-byte keys and 8-byte values."
);

define_fixed_tree!(
    ReserveAvlTree,
    ReserveLookupWitness,
    24,
    "Basis v2 reserve redemption tree with exact 32-byte keys and 24-byte values."
);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const SCALA_VECTORS: &str = include_str!("../tests/fixtures/basis_v2_avl_scala_0403162.json");

    fn decode_hex(encoded: &str) -> Vec<u8> {
        assert_eq!(encoded.len() % 2, 0);
        encoded
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).unwrap();
                u8::from_str_radix(pair, 16).unwrap()
            })
            .collect()
    }

    fn digest33(encoded: &str) -> [u8; 33] {
        decode_hex(encoded).try_into().unwrap()
    }

    #[test]
    fn exposes_the_exact_v2_shapes() {
        assert_eq!(
            TrackerAvlTree::shape(),
            FixedTreeShape {
                key_length: 32,
                value_length: 8,
                insert_allowed: true,
                update_allowed: true,
                remove_allowed: false,
            }
        );
        assert_eq!(ReserveAvlTree::shape().value_length(), 24);
    }

    #[test]
    fn insertion_order_is_explicit_and_replayable() {
        let entries = [([1u8; 32], [10u8; 8]), ([2u8; 32], [20u8; 8])];
        let tree = TrackerAvlTree::from_ordered_entries(entries).unwrap();
        let replay = TrackerAvlTree::from_ordered_entries(entries).unwrap();
        assert_eq!(tree.root_digest().unwrap(), replay.root_digest().unwrap());
        assert_eq!(tree.ordered_entries().copied().collect::<Vec<_>>(), entries);

        let ordered = [
            ([1u8; 32], [11u8; 8]),
            ([2u8; 32], [22u8; 8]),
            ([3u8; 32], [33u8; 8]),
            ([4u8; 32], [44u8; 8]),
        ];
        let reversed = [ordered[3], ordered[2], ordered[1], ordered[0]];
        let ordered_tree = TrackerAvlTree::from_ordered_entries(ordered).unwrap();
        let reversed_tree = TrackerAvlTree::from_ordered_entries(reversed).unwrap();
        assert_ne!(
            ordered_tree.root_digest().unwrap(),
            reversed_tree.root_digest().unwrap(),
            "reversing the authoritative replay order must not be treated as equivalent"
        );
    }

    #[test]
    fn insert_and_update_are_strict() {
        let mut tree = TrackerAvlTree::new();
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
        let mut tree = ReserveAvlTree::new();
        tree.insert([1u8; 32], [2u8; 24]).unwrap();
        let digest = tree.root_digest().unwrap();

        let membership = tree.lookup_witness([1u8; 32]).unwrap();
        assert_eq!(membership.value(), Some([2u8; 24]));
        assert!(!membership.proof().is_empty());
        assert!(ReserveAvlTree::verify_lookup(
            &digest,
            &[1u8; 32],
            &membership
        ));

        let non_membership = tree.lookup_witness([9u8; 32]).unwrap();
        assert_eq!(non_membership.value(), None);
        assert!(!non_membership.proof().is_empty());
        assert!(ReserveAvlTree::verify_lookup(
            &digest,
            &[9u8; 32],
            &non_membership
        ));
        assert!(ReserveAvlTree::verify_lookup_bytes(
            &digest,
            &[1u8; 32],
            membership.proof(),
            membership.value(),
        ));
        assert!(ReserveAvlTree::verify_lookup_bytes(
            &digest,
            &[9u8; 32],
            non_membership.proof(),
            None,
        ));

        let mut wrong = non_membership.clone();
        wrong.inner.value = Some([0u8; 24]);
        assert!(!ReserveAvlTree::verify_lookup(&digest, &[9u8; 32], &wrong));

        let mut wrong_proof = membership.clone();
        wrong_proof.inner.proof[0] ^= 1;
        assert!(!ReserveAvlTree::verify_lookup(
            &digest,
            &[1u8; 32],
            &wrong_proof
        ));
        assert!(!ReserveAvlTree::verify_lookup(
            &digest,
            &[3u8; 32],
            &membership
        ));
        let mut wrong_digest = digest;
        wrong_digest[1] ^= 1;
        assert!(!ReserveAvlTree::verify_lookup(
            &wrong_digest,
            &[1u8; 32],
            &membership
        ));
    }

    #[test]
    fn transition_proof_is_deterministic_and_does_not_mutate() {
        let mut tree = TrackerAvlTree::new();
        tree.insert([1u8; 32], [2u8; 8]).unwrap();
        tree.insert([3u8; 32], [4u8; 8]).unwrap();
        let before = tree.root_digest().unwrap();

        let witness = tree.transition_witness([1u8; 32], [5u8; 8]).unwrap();
        let again = tree.transition_witness([1u8; 32], [5u8; 8]).unwrap();
        assert_eq!(witness, again);
        assert_eq!(tree.root_digest().unwrap(), before);

        tree.update([1u8; 32], [5u8; 8]).unwrap();
        assert_eq!(tree.root_digest().unwrap(), witness.new_digest());
        assert!(TrackerAvlTree::verify_transition_bytes(
            &before,
            &[1u8; 32],
            &[5u8; 8],
            witness.proof(),
            &witness.new_digest(),
        ));
        let mut wrong_root = witness.new_digest();
        wrong_root[0] ^= 1;
        assert!(!TrackerAvlTree::verify_transition_bytes(
            &before,
            &[1u8; 32],
            &[5u8; 8],
            witness.proof(),
            &wrong_root,
        ));
    }

    #[test]
    fn new_key_transition_witness_matches_the_inserted_successor() {
        let mut tree = ReserveAvlTree::new();
        tree.insert([1u8; 32], [2u8; 24]).unwrap();
        let before = tree.root_digest().unwrap();

        let witness = tree.transition_witness([3u8; 32], [4u8; 24]).unwrap();
        assert_eq!(tree.root_digest().unwrap(), before);

        tree.insert([3u8; 32], [4u8; 24]).unwrap();
        assert_eq!(tree.root_digest().unwrap(), witness.new_digest());
    }

    #[test]
    fn malformed_lookup_proofs_return_false_without_unwinding() {
        let mut tree = ReserveAvlTree::new();
        tree.insert([1u8; 32], [2u8; 24]).unwrap();
        tree.insert([3u8; 32], [4u8; 24]).unwrap();
        let digest = tree.root_digest().unwrap();
        let valid = tree.lookup_witness([1u8; 32]).unwrap();

        for cut in 0..valid.proof().len() {
            let malformed = ReserveLookupWitness {
                inner: LookupWitness {
                    proof: valid.proof()[..cut].to_vec(),
                    value: valid.value(),
                },
            };
            assert!(!ReserveAvlTree::verify_lookup(
                &digest, &[1u8; 32], &malformed
            ));
        }

        let malformed = ReserveLookupWitness {
            inner: LookupWitness {
                proof: vec![0, 4],
                value: valid.value(),
            },
        };
        assert!(!ReserveAvlTree::verify_lookup(
            &digest, &[1u8; 32], &malformed
        ));

        let mut bit_mutations_exercised = 0;
        for offset in 0..valid.proof().len() {
            for bit in 0..8 {
                let mut mutated = valid.clone();
                mutated.inner.proof[offset] ^= 1 << bit;
                let _ = ReserveAvlTree::verify_lookup(&digest, &[1u8; 32], &mutated);
                bit_mutations_exercised += 1;
            }
        }
        assert_eq!(bit_mutations_exercised, valid.proof().len() * 8);
    }

    #[test]
    fn matches_chaincash_scala_golden_vectors_byte_for_byte() {
        let vectors: Value = serde_json::from_str(SCALA_VECTORS).unwrap();
        assert_eq!(
            vectors["source"]["commit"].as_str().unwrap(),
            "04031626f09c6590a20ad20d5583c6eccc14412d"
        );

        let tracker = &vectors["tracker_32_8"];
        let mut tracker_tree = TrackerAvlTree::new();
        assert_eq!(
            tracker_tree.root_digest().unwrap(),
            digest33(tracker["empty_digest_hex"].as_str().unwrap())
        );
        tracker_tree.insert([1; 32], [2; 8]).unwrap();
        tracker_tree.insert([3; 32], [4; 8]).unwrap();
        assert_eq!(
            tracker_tree.root_digest().unwrap(),
            digest33(tracker["starting_digest_hex"].as_str().unwrap())
        );
        let tracker_lookup = tracker_tree.lookup_witness([1; 32]).unwrap();
        let tracker_lookup_bytes = decode_hex(tracker["lookup_proof_hex"].as_str().unwrap());
        assert_eq!(tracker_lookup.proof(), tracker_lookup_bytes);
        assert_eq!(tracker_lookup.proof().len(), 143);
        assert_eq!(
            tracker["lookup_proof_sha256"].as_str().unwrap(),
            "5b3e3451d8137978f6b86ea2021bdc0d8e31a80ecbbe8ed48f9d67e9c1f3ca54"
        );
        let tracker_update = tracker_tree.transition_witness([1; 32], [5; 8]).unwrap();
        let tracker_update_bytes = decode_hex(tracker["update_proof_hex"].as_str().unwrap());
        assert_eq!(tracker_update.proof(), tracker_update_bytes);
        assert_eq!(
            tracker["update_proof_sha256"].as_str().unwrap(),
            "5b3e3451d8137978f6b86ea2021bdc0d8e31a80ecbbe8ed48f9d67e9c1f3ca54"
        );
        tracker_tree.update([1; 32], [5; 8]).unwrap();
        assert_eq!(
            tracker_tree.root_digest().unwrap(),
            digest33(tracker["output_digest_hex"].as_str().unwrap())
        );

        let reserve = &vectors["reserve_32_24"];
        let mut reserve_tree = ReserveAvlTree::new();
        assert_eq!(
            reserve_tree.root_digest().unwrap(),
            digest33(reserve["empty_digest_hex"].as_str().unwrap())
        );
        reserve_tree.insert([1; 32], [2; 24]).unwrap();
        assert_eq!(
            reserve_tree.root_digest().unwrap(),
            digest33(reserve["starting_digest_hex"].as_str().unwrap())
        );
        let reserve_absence = reserve_tree.lookup_witness([3; 32]).unwrap();
        let reserve_absence_bytes = decode_hex(reserve["absence_proof_hex"].as_str().unwrap());
        assert_eq!(reserve_absence.value(), None);
        assert_eq!(reserve_absence.proof(), reserve_absence_bytes);
        assert_eq!(reserve_absence.proof().len(), 125);
        assert_eq!(
            reserve["absence_proof_sha256"].as_str().unwrap(),
            "80f33ed16eee7c83ba7d316d07a5bb988a2e5c8f83a50487e4698829dfe02d76"
        );
        let reserve_insert = reserve_tree.transition_witness([3; 32], [4; 24]).unwrap();
        let reserve_insert_bytes = decode_hex(reserve["insert_proof_hex"].as_str().unwrap());
        assert_eq!(reserve_insert.proof(), reserve_insert_bytes);
        assert_eq!(
            reserve["insert_proof_sha256"].as_str().unwrap(),
            "80f33ed16eee7c83ba7d316d07a5bb988a2e5c8f83a50487e4698829dfe02d76"
        );
        reserve_tree.insert([3; 32], [4; 24]).unwrap();
        assert_eq!(
            reserve_tree.root_digest().unwrap(),
            digest33(reserve["output_digest_hex"].as_str().unwrap())
        );
    }
}
