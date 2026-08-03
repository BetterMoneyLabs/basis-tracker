use crate::authenticated_tree_ops::*;
use crate::batch_node::*;
use crate::operation::*;
use alloc::vec;
use alloc::vec::Vec;
use anyhow::Result;
use bytes::{BufMut, Bytes, BytesMut};
use core::cmp::Ordering;
use rand::prelude::*;
use rand::RngCore;

///
/// Implements the batch AVL prover from https://eprint.iacr.org/2016/994
/// Not thread safe if you use with ThreadUnsafeHash
///
/// @param keyLength           - length of keys in tree
/// @param valueLengthOpt      - length of values in tree. None if it is not fixed
/// @param oldRootAndHeight    - option root node and height of old tree. Tree should contain new nodes only
///                            WARNING if you pass it, all isNew and visited flags should be set correctly and height should be correct
/// @param collectChangedNodes - changed nodes will be collected to a separate buffer during tree modifications if `true`
/// @param hf                  - hash function
///
pub struct BatchAVLProver {
    pub base: AuthenticatedTreeOpsBase,

    // Directions are just a bit string representing booleans
    directions: Vec<u8>,
    directions_bit_length: usize,

    // Keeps track of where we are when replaying directions
    // a second time; needed for deletions
    replay_index: usize,

    // Keeps track of the last time we took a right step
    // when going down the tree; needed for deletions
    last_right_step: usize,

    old_top_node: Option<NodeId>,

    // operation has already been found in the tree
    // (if so, we know how to get to the leaf without
    //  any further comparisons)
    found: bool, // keeps track of whether the key for the current
}

impl BatchAVLProver {
    pub fn new(tree: AVLTree, collect_changed_nodes: bool) -> BatchAVLProver {
        let mut prover = BatchAVLProver {
            base: AuthenticatedTreeOpsBase::new(tree, collect_changed_nodes),
            directions: Vec::new(),
            directions_bit_length: 0,
            replay_index: 0,
            last_right_step: 0,
            old_top_node: None,
            found: false,
        };
        if prover.base.tree.root.is_none() {
            let t = LeafNode::new(
                &prover.base.tree.negative_infinity_key(),
                &Bytes::from(vec![0u8; prover.base.tree.value_length.unwrap_or(0)]),
                &prover.base.tree.positive_infinity_key(),
            );
            prover.base.tree.root = Some(t);
            prover.base.tree.reset();
        }
        prover.old_top_node = prover.base.tree.root.clone();
        prover
    }

    ///
    /// If operation.key exists in the tree and the operation succeeds,
    /// returns Success(Some(v)), where v is the value associated with operation.key
    /// before the operation.
    /// If operation.key does not exists in the tree and the operation succeeds, returns Success(None).
    /// Returns Failure if the operation fails.
    /// Does not modify the tree or the proof in case return is Failure.
    ///
    /// @param operation
    /// @return - Success(Some(old value)), Success(None), or Failure
    ////
    pub fn perform_one_operation(&mut self, operation: &Operation) -> Result<Option<ADValue>> {
        self.replay_index = self.directions_bit_length;
        let res = self.return_result_of_one_operation(operation, &self.top_node());
        if res.is_err() {
            // take the bit length before fail and divide by 8 with rounding up
            let old_directions_byte_length = (self.replay_index + 7) / 8;
            // undo the changes to the directions array
            self.directions.truncate(old_directions_byte_length);
            self.directions_bit_length = self.replay_index;
            if (self.directions_bit_length & 7) > 0 {
                // 0 out the bits of the last element of the directions array
                // that are above directionsBitLength
                let mask = (1u8 << (self.directions_bit_length & 7)) - 1;
                *self.directions.last_mut().unwrap() &= mask;
            }
        }
        res
    }

    ///
    /// @return nodes, that where presented in old tree (starting form oldTopNode, but are not presented in new tree
    ///
    pub fn removed_nodes(&mut self) -> Vec<NodeId> {
        for cn in &self.base.changed_nodes_buffer_to_check {
            if !self.contains(cn) {
                self.base.changed_nodes_buffer.push(cn.clone())
            }
        }
        self.base.changed_nodes_buffer.clone()
    }

    ///
    /// Generates the proof for all the operations in the list.
    /// Does NOT modify the tree
    ////
    pub fn generate_proof_for_operations(
        &self,
        operations: &Vec<Operation>,
    ) -> Result<(SerializedAdProof, ADDigest)> {
        let mut new_prover = BatchAVLProver::new(self.base.tree.clone(), false);
        for op in operations.iter() {
            new_prover.perform_one_operation(op)?;
        }
        Ok((new_prover.generate_proof(), new_prover.digest().unwrap()))
    }

    /* TODO Possible optimizations:
     * - Don't put in the key if it's in the modification stream somewhere
     *   (savings ~32 bytes per proof for transactions with existing key; 0 for insert)
     *   (problem is that then verifier logic has to change --
     *   can't verify tree immediately)
     * - Condense a sequence of balances and other non-full-byte info using
     *   bit-level stuff and maybe even "changing base without losing space"
     *   by Dodis-Patrascu-Thorup STOC 2010 (expected savings: 5-15 bytes
     *   per proof for depth 20, based on experiments with gzipping the array
     *   that contains only this info)
     * - Condense the sequence of values if they are mostly not randomly distributed
     */
    fn pack_tree(
        &self,
        r_node: &NodeId,
        packaged_tree: &mut BytesMut,
        previous_leaf_available: &mut bool,
    ) {
        // Post order traversal to pack up the tree
        if !self.base.tree.visited(r_node) {
            packaged_tree.put_u8(LABEL_IN_PACKAGED_PROOF);
            let label = self.base.tree.label(r_node);
            packaged_tree.extend_from_slice(&label);
            assert!(label.len() == DIGEST_LENGTH);
            *previous_leaf_available = false;
        } else {
            self.base.tree.mark_visited(r_node, false);
            match self.base.tree.copy(r_node) {
                Node::Leaf(leaf) => {
                    packaged_tree.put_u8(LEAF_IN_PACKAGED_PROOF);
                    if !*previous_leaf_available {
                        packaged_tree.extend_from_slice(&leaf.hdr.key.unwrap());
                    }
                    packaged_tree.extend_from_slice(&leaf.next_node_key);
                    if self.base.tree.value_length.is_none() {
                        packaged_tree.put_u32(leaf.value.len() as u32);
                    }
                    packaged_tree.extend_from_slice(&leaf.value);
                    *previous_leaf_available = true;
                }
                Node::Internal(node) => {
                    self.pack_tree(&node.left, packaged_tree, previous_leaf_available);
                    self.pack_tree(&node.right, packaged_tree, previous_leaf_available);
                    packaged_tree.put_u8(node.balance as u8);
                }
                _ => {
                    panic!("Node is not resolved");
                }
            }
        }
    }

    ///
    /// Generates the proof for all the operations performed (except the ones that failed)
    /// since the last generateProof call
    ///
    /// @return - the proof
    ///
    pub fn generate_proof(&mut self) -> SerializedAdProof {
        self.base.changed_nodes_buffer.clear();
        self.base.changed_nodes_buffer_to_check.clear();
        let mut packaged_tree = BytesMut::new();
        let mut previous_leaf_available = false;
        self.pack_tree(
            &self.old_top_node.as_ref().unwrap().clone(),
            &mut packaged_tree,
            &mut previous_leaf_available,
        );
        packaged_tree.put_u8(END_OF_TREE_IN_PACKAGED_PROOF);
        packaged_tree.extend_from_slice(&self.directions);

        // prepare for the next time proof
        self.base.tree.reset();
        self.directions = Vec::new();
        self.directions_bit_length = 0;
        self.old_top_node = self.base.tree.root.clone();

        packaged_tree.freeze()
    }

    fn walk<IR, LR>(
        &self,
        r_node: &NodeId,
        ir: IR,
        internal_node_fn: &mut dyn FnMut(&InternalNode, IR) -> (NodeId, IR),
        leaf_fn: &mut dyn FnMut(&LeafNode, IR) -> LR,
    ) -> LR {
        match self.base.tree.copy(r_node) {
            Node::Leaf(leaf) => leaf_fn(&leaf, ir),
            Node::Internal(r) => {
                let i = internal_node_fn(&r, ir);
                self.walk(&i.0, i.1, internal_node_fn, leaf_fn)
            }
            _ => {
                panic!("Node is not resolved");
            }
        }
    }

    ///
    /// Walk from tree to a leaf.
    ///
    /// @param internalNodeFn - function applied to internal nodes. Takes current internal node and current IR, returns
    ///                       new internal nod and new IR
    /// @param leafFn         - function applied to leafss. Takes current leaf and current IR, returns result of walk LR
    /// @param initial        - initial value of IR
    /// @tparam IR - result of applying internalNodeFn to internal node. E.g. some accumutalor of previous results
    /// @tparam LR - result of applying leafFn to a leaf. Result of all walk application
    /// @return
    ///
    pub fn tree_walk<IR, LR>(
        &self,
        internal_node_fn: &mut dyn FnMut(&InternalNode, IR) -> (NodeId, IR),
        leaf_fn: &mut dyn FnMut(&LeafNode, IR) -> LR,
        initial: IR,
    ) -> LR {
        self.walk(&self.top_node(), initial, internal_node_fn, leaf_fn)
    }

    ///
    ///
    /// @param rand - source of randomness
    /// @return Random leaf from the tree that is not positive or negative infinity
    ////
    pub fn random_walk(&self, rand: &mut dyn RngCore) -> Option<KeyValue> {
        let mut internal_node_fn = |r: &InternalNode, _dummy: ()| -> (NodeId, ()) {
            if rand.gen::<bool>() {
                (r.right.clone(), ())
            } else {
                (r.left.clone(), ())
            }
        };
        let mut leaf_fn = |leaf: &LeafNode, _dummy: ()| -> Option<KeyValue> {
            let key = leaf.hdr.key.as_ref().unwrap().clone();
            if key == self.base.tree.positive_infinity_key() {
                None
            } else if key == self.base.tree.negative_infinity_key() {
                None
            } else {
                let value = leaf.value.clone();
                Some(KeyValue { key, value })
            }
        };

        self.tree_walk(&mut internal_node_fn, &mut leaf_fn, ())
    }

    ///
    /// A simple non-modifying non-proof-generating lookup.
    /// Does not mutate the data structure
    ///
    /// @return Some(value) for value associated with the given key if key is in the tree, and None otherwise
    ///
    pub fn unauthenticated_lookup(&self, key: &ADKey) -> Option<ADValue> {
        let mut internal_node_fn = |r: &InternalNode, found: bool| {
            if found {
                // left all the way to the leaf
                (r.left.clone(), true)
            } else {
                match (*key).cmp(r.hdr.key.as_ref().unwrap()) {
                    Ordering::Equal =>
                    // found in the tree -- go one step right, then left to the leaf
                    {
                        (r.right.clone(), true)
                    }
                    Ordering::Less =>
                    // going left, not yet found
                    {
                        (r.left.clone(), false)
                    }
                    Ordering::Greater =>
                    // going right, not yet found
                    {
                        (r.right.clone(), false)
                    }
                }
            }
        };

        let mut leaf_fn = |leaf: &LeafNode, found: bool| -> Option<ADValue> {
            if found {
                Some(leaf.value.clone())
            } else {
                None
            }
        };

        self.tree_walk(&mut internal_node_fn, &mut leaf_fn, false)
    }

    fn check_tree_helper(&self, r_node: &NodeId, post_proof: bool) -> (NodeId, NodeId, usize) {
        let node = self.base.tree.copy(r_node);
        assert!(!post_proof || (!node.visited() && !node.is_new()));
        match node {
            Node::Internal(r) => {
                let key = r.hdr.key.unwrap();
                if let Node::Internal(rl) = &*r.left.borrow() {
                    assert!(*rl.hdr.key.as_ref().unwrap() < key);
                }
                if let Node::Internal(rr) = &*r.right.borrow() {
                    assert!(*rr.hdr.key.as_ref().unwrap() > key);
                }
                let (min_left, max_left, left_height) = self.check_tree_helper(&r.left, post_proof);
                let (min_right, max_right, right_height) =
                    self.check_tree_helper(&r.right, post_proof);
                assert_eq!(max_left.borrow().next_node_key(), min_right.borrow().key());
                assert_eq!(min_right.borrow().key(), key);
                assert!(
                    r.balance >= -1
                        && r.balance <= 1
                        && r.balance == (right_height as i8 - left_height as i8)
                );
                let height = core::cmp::max(left_height, right_height) + 1;
                (min_left, max_right, height)
            }
            _ => (r_node.clone(), r_node.clone(), 0),
        }
    }

    ///
    /// Is for debug only
    ///
    /// Checks the BST order, AVL balance, correctness of leaf positions, correctness of first and last
    /// leaf, correctness of nextLeafKey fields
    /// If postProof, then also checks for visited and isNew fields being false
    /// Warning: slow -- takes linear time in tree size
    /// Throws exception if something is wrong
    ///
    pub fn check_tree(&self, post_proof: bool) {
        let (min_tree, max_tree, tree_height) =
            self.check_tree_helper(&self.top_node(), post_proof);
        assert_eq!(
            min_tree.borrow().key(),
            self.base.tree.negative_infinity_key()
        );
        assert_eq!(
            max_tree.borrow().next_node_key(),
            self.base.tree.positive_infinity_key()
        );
        assert_eq!(tree_height, self.base.tree.height);
    }
}

impl AuthenticatedTreeOps for BatchAVLProver {
    fn get_state<'a>(&'a self) -> &'a AuthenticatedTreeOpsBase {
        return &self.base;
    }

    fn state<'a>(&'a mut self) -> &'a mut AuthenticatedTreeOpsBase {
        return &mut self.base;
    }

    ///
    /// Figures out whether to go left or right when from node r when searching for the key,
    /// using the appropriate bit in the directions bit string from the proof
    ///
    /// @param key
    /// @param r
    /// @return - true if to go left, false if to go right in the search
    ///
    fn next_direction_is_left(&mut self, key: &ADKey, r: &InternalNode) -> bool {
        let ret = if self.found {
            true
        } else {
            match (*key).cmp(r.hdr.key.as_ref().unwrap()) {
                Ordering::Equal => {
                    // found in the tree -- go one step right, then left to the leaf
                    self.found = true;
                    self.last_right_step = self.directions_bit_length;
                    false
                }
                Ordering::Less =>
                // going left
                {
                    true
                }
                Ordering::Greater =>
                // going right
                {
                    false
                }
            }
        };

        // encode Booleans as bits
        if (self.directions_bit_length & 7) == 0 {
            // new byte needed
            self.directions.push(if ret { 1u8 } else { 0u8 });
        } else {
            if ret {
                let i = self.directions_bit_length >> 3;
                self.directions[i] |= 1 << (self.directions_bit_length & 7);
                // change last byte
            }
        }
        self.directions_bit_length += 1;
        ret
    }

    ///
    /// Determines if the leaf r contains the key
    ///
    /// @param key
    /// @param r
    /// @return
    ////
    fn key_matches_leaf(&mut self, _key: &ADKey, _leaf: &LeafNode) -> Result<bool> {
        // The prover doesn't actually need to look at the leaf key,
        // because the prover would have already seen this key on the way
        // down the to leaf if and only if the leaf matches the key that is being sought
        let ret = self.found;
        self.found = false; // reset for next time
        Ok(ret)
    }

    ///
    /// Deletions go down the tree twice -- once to find the leaf and realize
    /// that it needs to be deleted, and the second time to actually perform the deletion.
    /// This method will re-create comparison results using directions array and lastRightStep
    /// variable. Each time it's called, it will give the next comparison result of
    /// key and node.key, where node starts at the root and progresses down the tree
    /// according to the comparison results.
    ///
    /// @return - result of previous comparison of key and relevant node's key
    ///
    fn replay_comparison(&mut self) -> i32 {
        let ret = if self.replay_index == self.last_right_step {
            0
        } else if (self.directions[self.replay_index >> 3] & (1 << (self.replay_index & 7))) == 0 {
            1
        } else {
            -1
        };
        self.replay_index += 1;
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_digest_hash_is_correct() {
        let prover = BatchAVLProver::new(
            AVLTree::new(
                |digest| Node::LabelOnly(NodeHeader::new(Some(*digest), None)),
                32,
                None,
            ),
            true,
        );
        let actual = base16::encode_lower(&prover.digest().unwrap());
        let expected = "4ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900";

        assert_eq!(actual, expected)
    }
}
