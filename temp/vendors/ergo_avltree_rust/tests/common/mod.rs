use anyhow::*;
use blake2::digest::Digest as _;
use bytes::Bytes;
use ergo_avltree_rust::authenticated_tree_ops::*;
use ergo_avltree_rust::batch_avl_prover::*;
use ergo_avltree_rust::batch_avl_verifier::*;
use ergo_avltree_rust::batch_node::*;
use ergo_avltree_rust::operation::*;
use ergo_avltree_rust::versioned_avl_storage::*;
use rand::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const INITIAL_TREE_SIZE: usize = 1000;
pub const KEY_LENGTH: usize = 32;
pub const VALUE_LENGTH: usize = 8;
pub const MAX_LIST_SIZE: usize = 1000;
pub const TEST_ITERATIONS: usize = 10;
pub const MIN_KEY: [u8; KEY_LENGTH] = [0u8; KEY_LENGTH];
pub const MAX_KEY: [u8; KEY_LENGTH] = [0xFFu8; KEY_LENGTH];

pub fn random_key_with_len(len: usize) -> ADKey {
    let key = (0..len).map(|_| rand::random::<u8>()).collect();
    if key < Bytes::from(MIN_KEY.to_vec()) || key > Bytes::from(MAX_KEY.to_vec()) {
        random_key_with_len(len)
    } else {
        key
    }
}

pub fn random_key() -> ADKey {
    random_key_with_len(KEY_LENGTH)
}

pub fn random_value() -> ADValue {
    Bytes::copy_from_slice(&rand::random::<[u8; VALUE_LENGTH]>())
}

pub fn random_kv() -> KeyValue {
    loop {
        let key = random_key();
        if key != Bytes::copy_from_slice(&MIN_KEY) && key != Bytes::copy_from_slice(&MAX_KEY) {
            let value = random_value();
            return KeyValue { key, value };
        }
    }
}

pub fn random_kv_list(max_size: usize) -> Vec<KeyValue> {
    let mut rnd = rand::thread_rng();
    let len: usize = rnd.gen_range(1..=max_size);
    let mut list = Vec::new();
    for _ in 0..len {
        list.push(random_kv());
    }
    list
}

pub fn generate_kv_list(size: usize) -> Vec<KeyValue> {
    (0..size)
        .map(|i| {
            let mut hasher = Blake2b256::new();
            hasher.update(&i.to_string());
            let key = Bytes::copy_from_slice(&hasher.finalize());
            let value = key.clone();
            KeyValue { key, value }
        })
        .collect()
}

pub fn i64_from_bytes(bytes: &Bytes) -> i64 {
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes[..]);
    i64::from_be_bytes(arr)
}

fn dummy_resolver(digest: &Digest32) -> Node {
    Node::LabelOnly(NodeHeader::new(Some(digest.clone()), None))
}

pub fn generate_verifier(
    initial_digest: &ADDigest,
    proof: &SerializedAdProof,
    key_length: usize,
    value_length: Option<usize>,
    max_num_operations: Option<usize>,
    max_deletes: Option<usize>,
) -> BatchAVLVerifier {
    BatchAVLVerifier::new(
        initial_digest,
        proof,
        generate_tree(key_length, value_length),
        max_num_operations,
        max_deletes,
    )
    .unwrap()
}

pub fn generate_tree(key_length: usize, value_length: Option<usize>) -> AVLTree {
    AVLTree::new(dummy_resolver, key_length, value_length)
}

pub fn generate_prover(key_length: usize, value_length: Option<usize>) -> BatchAVLProver {
    BatchAVLProver::new(generate_tree(key_length, value_length), true)
}

pub fn generate_and_populate_prover(size: usize) -> (BatchAVLProver, Vec<KeyValue>) {
    let mut prover = generate_prover(KEY_LENGTH, None);
    let mut initial_elements: Vec<KeyValue> = Vec::new();
    for i in 0..size {
        let mut hasher = Blake2b256::new();
        hasher.update(&i.to_string());
        let key = Bytes::copy_from_slice(&hasher.finalize());
        let value = Bytes::from(i.to_string());
        let kv = KeyValue { key, value };
        initial_elements.push(kv.clone());
        assert!(prover.perform_one_operation(&Operation::Insert(kv)).is_ok());
    }
    prover.generate_proof();
    (prover, initial_elements)
}

fn check_removed(prover: &mut BatchAVLProver, node: &NodeId, removed_nodes: &Vec<NodeId>) -> usize {
    let contains = prover.contains(node);
    let mut removed: usize = 0;
    if !contains {
        removed = 1;
    }
    assert!(
        contains
            || removed_nodes
                .iter()
                .find(|rn| { rn.borrow().get_label() == node.borrow().get_label() })
                .is_some()
    );

    let n = node.borrow().clone();
    match n {
        Node::Internal(i) => {
            removed += check_removed(prover, &i.left, removed_nodes);
            removed += check_removed(prover, &i.right, removed_nodes);
        }
        _ => {}
    }
    removed
}

///
/// check, that removedNodes contains all nodes, that are where removed, and do not contain nodes, that are still in the tree
///
pub fn check_tree(prover: &mut BatchAVLProver, old_top: &NodeId, removed_nodes: &Vec<NodeId>) {
    // check that there are no nodes in removedNodes, that are still in the tree
    for r in removed_nodes {
        assert!(!prover.contains(r));
    }

    let removed = check_removed(prover, old_top, removed_nodes);
    assert_eq!(removed, removed_nodes.len());
}

pub struct VersionedAVLStorageMock {
    saved_nodes: HashMap<ADDigest, (NodeId, usize)>,
    v: Option<ADDigest>,
}

impl VersionedAVLStorageMock {
    pub fn new() -> VersionedAVLStorageMock {
        VersionedAVLStorageMock {
            saved_nodes: HashMap::new(),
            v: None,
        }
    }
}

impl VersionedAVLStorage for VersionedAVLStorageMock {
    fn rollback(&mut self, version: &ADDigest) -> Result<(NodeId, usize)> {
        if let Some(pair) = self.saved_nodes.get(version) {
            Ok(pair.clone())
        } else {
            Err(anyhow!("Version not found"))
        }
    }
    ///
    /// Synchronize storage with prover's state
    ///
    /// @param prover - prover to synchronize storage with
    /// @return
    ///
    /// ignoring additionalData
    fn update(
        &mut self,
        prover: &mut BatchAVLProver,
        _additional_data: Vec<(ADKey, ADValue)>,
    ) -> Result<()> {
        let new_digest = prover.digest().unwrap();
        assert!(self
            .v
            .as_ref()
            .filter(|v| v.len() != new_digest.len())
            .is_none()); // Incorrect digest length
        self.v = Some(new_digest.clone());
        self.saved_nodes
            .insert(new_digest, (prover.top_node(), prover.get_tree().height));
        Ok(())
    }
    fn version(&self) -> Option<ADDigest> {
        self.v.clone()
    }
    fn rollback_versions<'a>(&'a self) -> Box<dyn Iterator<Item = ADDigest> + 'a> {
        Box::new(RollbackVersionIterator {
            version: self.v.clone(),
        })
    }
}

struct RollbackVersionIterator {
    version: Option<ADDigest>,
}

impl Iterator for RollbackVersionIterator {
    type Item = ADDigest;
    fn next(&mut self) -> Option<Self::Item> {
        let cur = self.version.clone();
        self.version = None;
        cur
    }
}

pub fn sha256(data: &str) -> Bytes {
    let mut hasher = Sha256::new();
    hasher.update(data);
    Bytes::copy_from_slice(&hasher.finalize())
}
