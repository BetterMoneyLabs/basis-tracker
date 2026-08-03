use crate::authenticated_tree_ops::*;
use crate::batch_node::*;
use crate::operation::*;
use alloc::vec::Vec;
use anyhow::*;
use byteorder::{BigEndian, ByteOrder};
use bytes::Bytes;

///
/// Implements the batch AVL verifier from https://eprint.iacr.org/2016/994
///
/// @param keyLength        - length of keys in tree
/// @param valueLengthOpt   - length of values in tree. None if it is not fixed
/// @param maxNumOperations - option the maximum number of operations that this proof
///                         can be for, to limit running time in case of malicious proofs.
///                         If None, running time limits will not be enforced.
/// @param maxDeletes       - at most, how many of maxNumOperations can be deletions;
///                         for a tighter running time bound and better attack protection.
///                         If None, defaults to maxNumOperations.
///
pub struct BatchAVLVerifier {
    proof: SerializedAdProof,
    max_num_operations: Option<usize>,
    max_deletes: Option<usize>,
    base: AuthenticatedTreeOpsBase,
    // Keeps track of where we are in the
    //  "directions" part of the proof
    directions_index: usize,
    // Keeps track of the last time we took a right step
    // when going down the tree; needed for deletions
    last_right_step: usize,
    // Keeps track of where we are when replaying directions a second time; needed for deletions
    replay_index: usize,
}

impl BatchAVLVerifier {
    pub fn new(
        starting_digest: &ADDigest,
        proof: &SerializedAdProof,
        tree: AVLTree,
        max_num_operations: Option<usize>,
        max_deletes: Option<usize>,
    ) -> Result<BatchAVLVerifier> {
        let mut verifier = BatchAVLVerifier {
            proof: proof.clone(),
            max_num_operations,
            max_deletes,
            base: AuthenticatedTreeOpsBase::new(tree, false),
            directions_index: 0,
            last_right_step: 0,
            replay_index: 0,
        };
        verifier.reconstruct_tree(starting_digest)?;
        Ok(verifier)
    }

    // Will be None if the proof is not correct and thus a tree cannot be reconstructed
    fn reconstruct_tree(&mut self, starting_digest: &ADDigest) -> Result<()> {
        ensure!(self.base.tree.key_length > 0);
        ensure!(starting_digest.len() == DIGEST_LENGTH + 1);
        self.base.tree.height = (starting_digest.last().unwrap() & 0xffu8) as usize;

        let max_nodes = if self.max_num_operations.is_some() {
            // compute the maximum number of nodes the proof can contain according to
            // https://eprint.iacr.org/2016/994 Appendix B last paragraph

            // First compute log (number of operations), rounded up
            let mut log_num_ops = 0;
            let mut temp = 1;
            let real_num_operations = self.max_num_operations.unwrap_or(0);
            while temp < real_num_operations {
                temp = temp * 2;
                log_num_ops += 1
            }

            // compute maximum height that the tree can be before an operation
            temp = 1 + core::cmp::max(self.base.tree.height, log_num_ops);
            let hnew = temp + temp / 2; // this will replace 1.4405 from the paper with 1.5 and will round down, which is safe, because hnew is an integer
            let real_max_deletes = self.max_deletes.unwrap_or(real_num_operations);
            // Note: this is quite likely a lot more than there will really be nodes
            (real_num_operations + real_max_deletes) * (2 * self.base.tree.height + 1)
                + real_max_deletes * hnew
                + 1 // +1 needed in case numOperations == 0
        } else {
            0
        };

        // Now reconstruct the tree from the proof, which has the post order traversal
        // of the tree
        let mut num_nodes = 0;
        let mut i: usize = 0;
        let mut previous_leaf: Option<NodeId> = None;
        let mut stack: Vec<NodeId> = Vec::new();
        let key_length = self.base.tree.key_length;
        while self.proof[i] != END_OF_TREE_IN_PACKAGED_PROOF {
            let n = self.proof[i];
            i += 1;
            num_nodes += 1;
            ensure!(self.max_num_operations.is_none() || num_nodes <= max_nodes);
            match n {
                LABEL_IN_PACKAGED_PROOF => {
                    let mut label: Digest32 = Default::default();
                    label.copy_from_slice(&self.proof[i..i + DIGEST_LENGTH]);
                    i += DIGEST_LENGTH;
                    stack.push(Node::new_label(&label));
                    previous_leaf = None;
                }
                LEAF_IN_PACKAGED_PROOF => {
                    let key = if let Some(prev) = previous_leaf {
                        Bytes::copy_from_slice(&self.base.tree.next_node_key(&prev))
                    } else {
                        let start = i;
                        i += self.base.tree.key_length;
                        Bytes::copy_from_slice(&self.proof[start..i])
                    };
                    let next_leaf_key = Bytes::copy_from_slice(&self.proof[i..i + key_length]);
                    i += key_length;
                    let value_length = self.base.tree.value_length.unwrap_or_else(|| {
                        let vl = BigEndian::read_u32(&self.proof[i..i + 4]) as usize;
                        i += 4;
                        vl
                    });
                    let value = Bytes::copy_from_slice(&self.proof[i..i + value_length]);
                    i += value_length;
                    let leaf = LeafNode::new(&key, &value, &next_leaf_key);
                    stack.push(leaf.clone());
                    previous_leaf = Some(leaf);
                }
                _ => {
                    let right = stack.pop().unwrap();
                    let left = stack.pop().unwrap();
                    stack.push(InternalNode::new(None, &left, &right, n as Balance));
                }
            }
        }

        ensure!(stack.len() == 1);
        let root = stack.pop().unwrap();
        ensure!(starting_digest.starts_with(&self.base.tree.label(&root)));
        self.base.tree.root = Some(root);
        self.directions_index = (i + 1) * 8; // Directions start right after the packed tree, which we just finished
        Ok(())
    }

    ///
    /// If operation.key exists in the tree and the operation succeeds,
    /// returns Success(Some(v)), where v is the value associated with operation.key
    /// before the operation.
    /// If operation.key does not exists in the tree and the operation succeeds, returns Success(None).
    /// Returns Failure if the operation fails or the proof does not verify.
    /// After one failure, all subsequent operations will fail and digest
    /// is None.
    ///
    /// @param operation
    /// @return - Success(Some(old value)), Success(None), or Failure
    ///
    pub fn perform_one_operation(&mut self, operation: &Operation) -> Result<Option<ADValue>> {
        self.replay_index = self.directions_index;
        let root = self
            .base
            .tree
            .root
            .as_ref()
            .ok_or(anyhow!("Empty tree"))?
            .clone();
        let res = self.return_result_of_one_operation(operation, &root);
        if res.is_err() {
            self.base.tree.root = None;
            self.base.tree.height = 0;
        }
        res
    }
}

impl AuthenticatedTreeOps for BatchAVLVerifier {
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
    fn next_direction_is_left(&mut self, _key: &ADKey, _r: &InternalNode) -> bool {
        // Decode bits of the proof as Booleans
        let ret =
            if self.proof[self.directions_index >> 3] & (1 << (self.directions_index & 7)) != 0 {
                true
            } else {
                self.last_right_step = self.directions_index;
                false
            };
        self.directions_index += 1;
        ret
    }

    ///
    /// Determines if the leaf r contains the key or if r.key < r < r.nextLeafKey
    /// If neither of those holds, causes an exception.
    ///
    /// @param key
    /// @param r_node
    /// @return
    ///
    fn key_matches_leaf(&mut self, key: &ADKey, leaf: &LeafNode) -> Result<bool> {
        // keyMatchesLeaf for the verifier is different than for the prover:
        // since the verifier doesn't have keys in internal nodes, keyMatchesLeaf
        // checks that the key is either equal to the leaf's key
        // or is between the leaf's key and its nextLeafKey
        // See https://eprint.iacr.org/2016/994 Appendix B paragraph "Our Algorithms"
        let leaf_key = leaf.hdr.key.as_ref().unwrap();
        if *key == *leaf_key {
            Ok(true)
        } else {
            ensure!(*key > *leaf_key);
            ensure!(*key < leaf.next_node_key);
            Ok(false)
        }
    }

    ///
    /// Deletions go down the tree twice -- once to find the leaf and realize
    /// that it needs to be deleted, and the second time to actually perform the deletion.
    /// This method will re-create comparison results using directions in the proof and lastRightStep
    /// variable. Each time it's called, it will give the next comparison result of
    /// key and node.key, where node starts at the root and progresses down the tree
    /// according to the comparison results.
    ///
    /// @return - result of previous comparison of key and relevant node's key
    ///
    fn replay_comparison(&mut self) -> i32 {
        let ret = if self.replay_index == self.last_right_step {
            0
        } else if (self.proof[self.replay_index >> 3] & (1 << (self.replay_index & 7))) == 0
            && self.replay_index < self.last_right_step
        {
            1
        } else {
            -1
        };
        self.replay_index += 1;
        ret
    }
}
