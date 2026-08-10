use bytes::Bytes;
use ergo_avltree_rust::{
    authenticated_tree_ops::AuthenticatedTreeOps,
    batch_avl_prover::BatchAVLProver,
    batch_avl_verifier::BatchAVLVerifier,
    batch_node::{AVLTree, LeafNode, Node, NodeHeader},
    operation::{ADKey, ADValue, KeyValue, Operation},
};

const KEY_LENGTH: usize = 32;
const VALUE_LENGTH: usize = 24;

fn resolver(_digest: &[u8; 32]) -> Node {
    Node::Leaf(LeafNode {
        hdr: NodeHeader {
            visited: false,
            is_new: false,
            label: None,
            key: Some(ADKey::from(vec![0; KEY_LENGTH])),
        },
        value: ADValue::from(vec![]),
        next_node_key: ADKey::from(vec![0; KEY_LENGTH]),
    })
}

fn tree() -> AVLTree {
    AVLTree::new(resolver, KEY_LENGTH, Some(VALUE_LENGTH))
}

fn insert(key: u8, value: u8) -> Operation {
    Operation::Insert(KeyValue {
        key: vec![key; KEY_LENGTH].into(),
        value: vec![value; VALUE_LENGTH].into(),
    })
}

fn valid_lookup_case() -> (Bytes, Bytes, ADKey) {
    let mut prover = BatchAVLProver::new(tree(), true);
    prover.perform_one_operation(&insert(1, 11)).unwrap();
    prover.perform_one_operation(&insert(3, 33)).unwrap();
    let _ = prover.generate_proof();
    let digest = prover.digest().unwrap();
    let key = ADKey::from(vec![1; KEY_LENGTH]);
    prover
        .perform_one_operation(&Operation::Lookup(key.clone()))
        .unwrap();
    let proof = prover.generate_proof();
    (digest, proof, key)
}

fn exercise(digest: &Bytes, proof: Bytes, key: &ADKey) {
    if let Ok(mut verifier) = BatchAVLVerifier::new(digest, &proof, tree(), Some(1), Some(0)) {
        let _ = verifier.perform_one_operation(&Operation::Lookup(key.clone()));
    }
}

fn main() {
    let arbitrary_digest = Bytes::from(vec![0; 33]);
    let malformed = [
        vec![],
        vec![3],
        vec![3, 0],
        vec![2],
        vec![2, 0, 0, 0],
        vec![0, 4],
        vec![2; 128],
    ];
    for proof in malformed {
        assert!(BatchAVLVerifier::new(
            &arbitrary_digest,
            &Bytes::from(proof),
            tree(),
            Some(1),
            Some(0),
        )
        .is_err());
    }

    let (digest, valid_proof, key) = valid_lookup_case();
    assert!(valid_proof.len() > 1);
    for cut in 0..valid_proof.len() {
        exercise(&digest, valid_proof.slice(..cut), &key);
    }
    for offset in 0..valid_proof.len() {
        for bit in 0..8 {
            let mut mutated = valid_proof.to_vec();
            mutated[offset] ^= 1 << bit;
            exercise(&digest, Bytes::from(mutated), &key);
        }
    }
}
