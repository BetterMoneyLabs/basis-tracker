use crate::batch_node::*;
use crate::operation::*;
use alloc::vec::Vec;
use bytes::{BufMut, BytesMut};

use anyhow::*;

type ChangeHappened = bool;
type HeightIncreased = bool;
type ToDelete = bool;

pub struct AuthenticatedTreeOpsBase {
    pub collect_changed_nodes: bool,
    pub changed_nodes_buffer: Vec<NodeId>,
    pub changed_nodes_buffer_to_check: Vec<NodeId>,
    pub tree: AVLTree,
}

impl AuthenticatedTreeOpsBase {
    pub fn new(tree: AVLTree, collect_changed_nodes: bool) -> AuthenticatedTreeOpsBase {
        AuthenticatedTreeOpsBase {
            collect_changed_nodes,
            changed_nodes_buffer: Vec::new(),
            changed_nodes_buffer_to_check: Vec::new(),
            tree,
        }
    }
}

pub trait AuthenticatedTreeOps {
    fn get_state<'a>(&'a self) -> &'a AuthenticatedTreeOpsBase;
    fn state<'a>(&'a mut self) -> &'a mut AuthenticatedTreeOpsBase;
    fn tree<'a>(&'a mut self) -> &'a mut AVLTree {
        &mut self.state().tree
    }
    fn get_tree<'a>(&'a self) -> &'a AVLTree {
        &self.get_state().tree
    }

    fn top_node(&self) -> NodeId {
        self.get_tree().root.as_ref().unwrap().clone()
    }

    fn extract_nodes(&self, extractor: &mut dyn FnMut(&mut Node) -> bool) -> Option<Vec<NodeId>> {
        self.get_tree().extract_nodes(extractor)
    }

    fn extract_first_node(&self, extractor: &mut dyn FnMut(&mut Node) -> bool) -> Option<NodeId> {
        self.get_tree().extract_first_node(extractor)
    }

    ///
    /// @return `true` if this tree has an element that has the same label, as `node.label`, `false` otherwise.
    ///
    fn contains(&self, node: &NodeId) -> bool {
        self.get_tree().contains(node)
    }

    // The following four methods differ for the prover and verifier, but are used in the code below'

    ///
    /// @return - whether we found the correct leaf and the key contains it
    ///
    fn key_matches_leaf(&mut self, key: &ADKey, leaf: &LeafNode) -> Result<bool>;

    ///
    /// @return - whether to go left or right when searching for key and standing at r
    ///
    fn next_direction_is_left(&mut self, key: &ADKey, r: &InternalNode) -> bool;

    ///
    /// Deletions go down the tree twice -- once to find the leaf and realize
    /// that it needs to be deleted, and the second time to actually perform the deletion.
    /// This method will re-create comparison results. Each time it's called, it will give
    /// the next comparison result of
    /// key and node.key, where node starts at the root and progresses down the tree
    /// according to the comparison results.
    ///
    /// @return - result of previous comparison of key and relevant node's key
    ///
    fn replay_comparison(&mut self) -> i32;

    fn on_node_visit(&mut self, node: &NodeId, operation: &Operation, is_rotate: bool) {
        let this = self.state();
        if this.collect_changed_nodes && !this.tree.visited(node) && !this.tree.is_new(node) {
            if is_rotate {
                // during rotate operation node may stay in the tree in a different position
                this.changed_nodes_buffer_to_check.push(node.clone());
            } else {
                match operation {
                    Operation::Insert(_) | Operation::Remove(_) | Operation::InsertOrUpdate(_) =>
                    // during non-rotate insert and remove operations nodes on the path should not be presented in a new tree
                    {
                        this.changed_nodes_buffer.push(node.clone())
                    }
                    Operation::Lookup(_) => {}
                    _ =>
                    // during other non-lookup operations we don't know, whether node will stay in thee tree or not
                    {
                        this.changed_nodes_buffer_to_check.push(node.clone())
                    }
                }
            }
        }
        this.tree.mark_visited(node, true);
    }

    ///
    /// The digest consists of the label of the root node followed by its height,
    /// expressed as a single (unsigned) byte
    ///
    fn digest(&self) -> Option<ADDigest> {
        let this = self.get_state();
        assert!(this.tree.height < 256);
        // rootNodeHeight should never be more than 255, so the toByte conversion is safe (though it may cause an incorrect
        // sign on the signed byte if rootHeight>127, but we handle that case correctly on decoding the byte back to int in the
        // verifier, by adding 256 if it's negative).
        // The reason rootNodeHeight should never be more than 255 is that if height is more than 255,
        // then the AVL tree has at least  2^{255/1.4405} = 2^177 leaves, which is more than the number of atoms on planet Earth.
        if let Some(root) = &this.tree.root {
            let mut buf = BytesMut::new();
            buf.extend_from_slice(&this.tree.label(root));
            buf.put_u8(this.tree.height as u8);
            Some(buf.freeze())
        } else {
            None
        }
    }

    ///
    /// Assumes the conditions for the double left rotation have already been established
    /// and rightChild.left.visited = true
    /// neither child needs to be attached to currentRoot
    ///
    fn double_left_rotate(
        &mut self,
        current_root: &NodeId,
        left_child: &NodeId,
        right_child: &NodeId,
    ) -> NodeId {
        let this = self.state();
        let new_root = this.tree.left(right_child);
        let (new_left_balance, new_right_balance) = match this.tree.balance(&new_root) {
            a if a == 0 => (0i8, 0i8),
            a if a == -1 => (0i8, 1i8),
            a if a == 1 => (-1i8, 0i8),
            a => panic!("Invalid balance {}", a),
        };
        let new_left_child = InternalNode::update(
            current_root,
            left_child,
            &this.tree.left(&new_root),
            new_left_balance,
        );
        let new_right_child = InternalNode::update(
            right_child,
            &this.tree.right(&new_root),
            &this.tree.right(right_child),
            new_right_balance,
        );
        let root = InternalNode::update(&new_root, &new_left_child, &new_right_child, 0i8);
        this.tree.root = Some(root.clone());
        root
    }

    ///
    /// Assumes the conditions for the double right rotation have already been established
    /// and leftChild.right.visited = true
    /// neither child needs to be attached to currentRoot
    ///
    fn double_right_rotate(
        &mut self,
        current_root: &NodeId,
        left_child: &NodeId,
        right_child: &NodeId,
    ) -> NodeId {
        let this = self.state();
        let new_root = this.tree.right(left_child);
        let (new_left_balance, new_right_balance) = match this.tree.balance(&new_root) {
            a if a == 0 => (0i8, 0i8),
            a if a == -1 => (0i8, 1i8),
            a if a == 1 => (-1i8, 0i8),
            a => panic!("Invalid balance {}", a),
        };
        let new_right_child = InternalNode::update(
            current_root,
            &this.tree.right(&new_root),
            right_child,
            new_right_balance,
        );
        let new_left_child = InternalNode::update(
            left_child,
            &this.tree.left(&left_child),
            &this.tree.left(&new_root),
            new_left_balance,
        );
        let root = InternalNode::update(&new_root, &new_left_child, &new_right_child, 0i8);
        this.tree.root = Some(root.clone());
        root
    }

    ///
    /// @return - a new node with two leaves: r on the left and a new leaf containing key and value on the right
    ///
    fn add_node(&self, r_node: &NodeId, key: &ADKey, v: &ADValue) -> NodeId {
        let this = self.get_state();
        let n = this.tree.next_node_key(r_node);
        InternalNode::new(
            Some(key.clone()),
            &LeafNode::update(
                r_node,
                &this.tree.key(r_node),
                &this.tree.value(r_node),
                key,
            ),
            &LeafNode::new(key, v, &n),
            0,
        )
    }

    fn return_result_of_one_operation(
        &mut self,
        operation: &Operation,
        root_node: &NodeId,
    ) -> Result<Option<ADValue>> {
        let key = operation.key();
        ensure!(key > self.tree().negative_infinity_key()); // Key is more than -inf
        ensure!(key < self.tree().positive_infinity_key()); // Key is less than +inf
        ensure!(key.len() == self.tree().key_length);
        let mut saved_node: Option<NodeId> = None;

        let (new_root_node, _, height_increased, to_delete, old_value) =
            self.modify_helper(root_node, &key, operation)?;
        if to_delete {
            let (post_delete_root_node, height_decreased) =
                self.delete_helper(&new_root_node, false, operation, &mut saved_node);
            if height_decreased {
                self.tree().height -= 1;
            }
            self.tree().root = Some(post_delete_root_node);
        } else {
            if height_increased {
                self.tree().height += 1;
            }
            self.tree().root = Some(new_root_node);
        }
        Ok(old_value)
    }

    ///
    /// returns the new root, an indicator whether tree has been modified at r or below,
    /// an indicator whether the height has increased,
    /// an indicator whether we need to go delete the leaf that was just reached,
    /// and the old value associated with key
    ///
    /// Handles binary tree search and AVL rebalancing
    ///
    /// Deletions are not handled here in order not to complicate the code even more -- in case of deletion,
    /// we don't change the tree, but simply return toDelete = true.
    /// We then go in and delete using deleteHelper
    ///
    fn modify_helper(
        &mut self,
        r_node: &NodeId,
        key: &ADKey,
        operation: &Operation,
    ) -> Result<(
        NodeId,
        ChangeHappened,
        HeightIncreased,
        ToDelete,
        Option<ADValue>,
    )> {
        // Do not set the visited flag on the way down -- set it only after you know the operation did not fail,
        // because if the operation failed, there is no need to put nodes in the proof.
        let res = match self.tree().copy(r_node) {
			Node::Leaf(r) => {
                if self.key_matches_leaf(key, &r)? {
                    match operation {
                        Operation::Lookup(_) => {
                            self.on_node_visit(r_node, operation, false);
                            (r_node.clone(), false, false, false, Some(r.value))
                        }
                        _ => { // on modification
                            match operation.update_fn(Some(r.value.clone()))? {
                                None =>  { // delete key
                                    self.on_node_visit(r_node, operation, false);
                                    (r_node.clone(), false, false, true, Some(r.value))
                                }
                                Some(v) => { // update value
                                    assert!(self.tree().value_length.filter(|len|v.len() != *len).is_none());
                                    let old_value = Some(r.value);
                                    let r_new = LeafNode::update(r_node, &r.hdr.key.unwrap(), &v, &r.next_node_key);
                                    self.on_node_visit(r_node, operation, false);
                                    (r_new, true, false, false, old_value)
                                }
                            }
                        } // do nothing
                    }
                } else {
                    // x > r.key
                    match operation {
                        Operation::Lookup(_) => {
                            self.on_node_visit(r_node, operation, false);
                            (r_node.clone(), false, false, false, None)
                        }
                        _ => { // on modification
                            match operation.update_fn(None)? {
                                None => { // don't change anything, just lookup
                                    self.on_node_visit(r_node, operation, false);
                                    (r_node.clone(), false, false, false, None)
                                }
                                Some(v) => { // insert new value
                                    assert!(self.tree().value_length.filter(|len|v.len() != *len).is_none());
                                    self.on_node_visit(r_node, operation, false);
                                    (self.add_node(r_node, &key, &v), true, true, false, None)
                                }
                            }
                        }
                    }
                }
            }
            Node::Internal(r) => {
                // Go recursively in the correct direction
                // Get a new node
                // See if a single or double rotation is needed for AVL tree balancing
                if self.next_direction_is_left(key, &r) {
                    let (new_leftm, change_happened, child_height_increased, to_delete, old_value) = self.modify_helper(&r.left, key, operation)?;
                    self.on_node_visit(r_node, operation, false);

                    // balance = -1 if left higher, +1 if left lower
                    if change_happened {
                        if child_height_increased && r.balance < 0 {
                            // need to rotate
                            // at this point we know newLeftM must be an internal node and not a leaf -- because height increased
                            if self.tree().balance(&new_leftm) < 0 {
                                // single right rotate
                                let new_r = InternalNode::update(r_node, &self.tree().right(&new_leftm), &r.right, 0);
                                (InternalNode::update(&new_leftm, &self.tree().left(&new_leftm), &new_r, 0), true, false, false, old_value)
                            } else {
                                (self.double_right_rotate(r_node, &new_leftm, &r.right), true, false, false, old_value)
                            }
                        } else {
                            // no need to rotate
                            let my_height_increased = child_height_increased && r.balance == 0;
                            let r_balance = if child_height_increased { r.balance - 1 } else { r.balance };
                            (InternalNode::update(r_node, &new_leftm, &r.right, r_balance), true, my_height_increased, false, old_value)
                        }
                    } else {
                        // no change happened
                        (r_node.clone(), false, false, to_delete, old_value)
                    }
                } else {
                    let (new_rightm, change_happened, child_height_increased, to_delete, old_value) = self.modify_helper(&r.right, key, operation)?;
                    self.on_node_visit(r_node, operation, false);

                    // balance = -1 if left higher, +1 if left lower
                    if change_happened {
                        if child_height_increased && r.balance > 0 {
                            // need to rotate
                            // at this point we know newRightM must be an internal node and not a leaf -- because height increased
                            if self.tree().balance(&new_rightm) > 0 {
                                // single left rotate
                                let new_r = InternalNode::update(r_node, &r.left, &self.tree().left(&new_rightm), 0);
                                (InternalNode::update(&new_rightm, &new_r, &self.tree().right(&new_rightm), 0), true, false, false, old_value)
                            } else {
                                (self.double_left_rotate(r_node, &r.left, &new_rightm), true, false, false, old_value)
                            }
                        } else {
                            // no need to rotate
                            let my_height_increased = child_height_increased && r.balance == 0;
                            let r_balance = if child_height_increased { r.balance + 1 } else { r.balance };
                            (InternalNode::update(r_node, &r.left, &new_rightm, r_balance), true, my_height_increased, false, old_value)
                        }
                    } else {
                        // no change happened
                        (r_node.clone(), false, false, to_delete, old_value)
                    }
                }
            }
            _ =>
                bail!("Should never reach this point. If in prover, this is a bug. If in verifier, this proof is wrong.")
        };
        Ok(res)
    }

    // Overall strategy: if key is found in the node that has only a leaf as either
    // of the two children, we can just delete the node. If it has a leaf as the right child,
    // we can also delete the right child, update the nextLeafKey in the rightmost leaf of the left subtree,
    // and we are done. Else, it has a leaf as the left child,
    // so we copy the information from this left child leaf to the leftmost leaf in the right subtree,
    // and delete the left child.
    //
    // Things get more complicated key is found in a node that has two non-leaf children.
    // In that case, we perform a deleteMax operation on the left subtree
    // (recursively call ourselves on the left child with
    // with deleteMax = true), and copy the information from that deleted leaf into the node where the
    // key was found and into the leftmost leaf of its right subtree

    fn change_next_leaf_key_of_max_node(
        &mut self,
        r_node: &NodeId,
        next_leaf_key: &ADKey,
        operation: &Operation,
    ) -> NodeId {
        self.on_node_visit(r_node, operation, false);
        match self.tree().copy(r_node) {
            Node::Leaf(node) =>
                LeafNode::update(r_node, &node.hdr.key.unwrap(), &node.value, &next_leaf_key),
            Node::Internal(node) =>
                InternalNode::update(r_node, &node.left, &self.change_next_leaf_key_of_max_node(&node.right, next_leaf_key, operation), node.balance),
            _ =>
                panic!("Should never reach this point. If in prover, this is a bug. In in verifier, this proof is wrong.")
        }
    }

    fn change_key_and_value_of_min_node(
        &mut self,
        r_node: &NodeId,
        new_key: &ADKey,
        new_value: &ADValue,
        operation: &Operation,
    ) -> NodeId {
        self.on_node_visit(r_node, operation, false);
        match self.tree().copy(r_node) {
            Node::Leaf(node) =>
                LeafNode::update(r_node, new_key, new_value, &node.next_node_key),
            Node::Internal(node) =>
                InternalNode::update(r_node, &self.change_key_and_value_of_min_node(&node.left, new_key, new_value, operation), &node.right, node.balance),
            _ =>
                panic!("Should never reach this point. If in prover, this is a bug. In in verifier, this proof is wrong.")
        }
    }

    /// Deletes the node in the subtree rooted at r and its corresponding leaf
    /// as indicated by replayComparison or deleteMax. Performs AVL balancing.
    ///
    /// If deleteMax == false: deletes the first node for which replayComparison returns 0
    /// and the leaf that is the leftmost descendant of this node's child
    ///
    /// If deleteMax == true: deletes the right leaf and its parent, replacing the parent
    /// with the parent's left child
    ///
    /// Returns the new root and an indicator whether the tree height decreased
    ///
    fn delete_helper(
        &mut self,
        r_node: &NodeId,
        delete_max: bool,
        operation: &Operation,
        saved_node: &mut Option<NodeId>,
    ) -> (NodeId, bool) {
        self.on_node_visit(r_node, operation, false);

        let direction = if delete_max {
            1
        } else {
            self.replay_comparison()
        };
        if let Node::Internal(r) = self.tree().copy(r_node) {
            assert!(!(direction < 0 && r.left.borrow().is_leaf()));

            // If direction<0, this means we are not in deleteMax mode and we still haven't found
            // the value we are trying to delete
            // If the next step -- which is to the left -- is a leaf, then the value
            // we are looking for is not a key of any internal node in the tree,
            // which is impossible

            if direction >= 0 {
                if let Node::Leaf(right_child) = self.tree().copy(&r.right) {
                    // we delete this node and its right child (leaf)
                    // we return the left subtree
                    self.on_node_visit(&r.right, operation, false);
                    if delete_max {
                        // If we are in deleteMax mode,
                        // we should save the info of leaf we are deleting,
                        // because it will be copied over to its successor
                        *saved_node = Some(r.right);
                        return (r.left, true);
                    } else {
                        // Otherwise, we really are deleting the leaf, and therefore
                        // we need to change the nextLeafKey of its predecessor
                        assert!(direction == 0);
                        return (
                            self.change_next_leaf_key_of_max_node(
                                &r.left,
                                &right_child.next_node_key,
                                operation,
                            ),
                            true,
                        );
                    }
                }
            }
            if direction == 0 {
                if let Node::Leaf(left_child) = self.tree().copy(&r.left) {
                    // we know (r.left.isInstanceOf[Leaf])
                    // we delete the node and its left child (leaf); we return the right
                    // subtree, after changing the key and value stored in its leftmost leaf
                    self.on_node_visit(&r.left, operation, false);
                    return (
                        self.change_key_and_value_of_min_node(
                            &r.right,
                            &left_child.hdr.key.unwrap(),
                            &left_child.value,
                            operation,
                        ),
                        true,
                    );
                }
            }
            // Potential hard deletion cases:
            if direction <= 0 {
                // going left; know left child is not a leaf; deleteMax if and only if direction == 0
                let (new_left, child_height_decreased) =
                    self.delete_helper(&r.left, direction == 0, operation, saved_node);

                let new_root = if direction == 0 {
                    // this is the case where we needed to delete the min of the right
                    // subtree, but, because we had two non-leaf children,
                    // we instead deleted the node that was the max of the left subtree
                    // and are copying its info
                    let s = saved_node.take().unwrap();
                    let r_with_changed_key = InternalNode::update_key(r_node, &self.tree().key(&s));
                    let left = self.tree().left(&r_with_changed_key);
                    let right = self.tree().right(&r_with_changed_key);
                    let key = self.tree().key(&s);
                    let value = self.tree().value(&s);
                    InternalNode::update(
                        &r_with_changed_key,
                        &left,
                        &self.change_key_and_value_of_min_node(&right, &key, &value, operation),
                        self.tree().balance(&r_with_changed_key),
                    )
                } else {
                    r_node.clone()
                };
                let root_balance = self.tree().balance(&new_root);
                let root_right = self.tree().right(&new_root);
                if child_height_decreased && root_balance > 0 {
                    // new to rotate because my left subtree is shorter than my right
                    self.on_node_visit(&root_right, operation, true);

                    // I know my right child is not a leaf, because it is taller than my left
                    if let Node::Internal(right_child) = self.tree().copy(&root_right) {
                        if right_child.balance < 0 {
                            // double left rotate
                            // I know rightChild.left is not a leaf, because rightChild has a higher subtree on the left
                            self.on_node_visit(&right_child.left, operation, true);
                            (
                                self.double_left_rotate(&new_root, &new_left, &root_right),
                                true,
                            )
                        } else {
                            // single left rotate
                            let new_left_child = InternalNode::update(
                                &new_root,
                                &new_left,
                                &right_child.left,
                                1 - right_child.balance,
                            );
                            let new_rbalance = right_child.balance - 1;
                            let new_r = InternalNode::update(
                                &root_right,
                                &new_left_child,
                                &right_child.right,
                                new_rbalance,
                            );
                            (new_r, new_rbalance == 0)
                        }
                    } else {
                        panic!("Not internal node");
                    }
                } else {
                    // no rotation, just recalculate newRoot.balance and childHeightDecreased
                    let new_balance = if child_height_decreased {
                        root_balance + 1
                    } else {
                        root_balance
                    };
                    (
                        InternalNode::update(&new_root, &new_left, &root_right, new_balance),
                        child_height_decreased && new_balance == 0,
                    )
                }
            } else {
                // going right; know right child is not a leaf
                let (new_right, child_height_decreased) =
                    self.delete_helper(&r.right, delete_max, operation, saved_node);
                if child_height_decreased && r.balance < 0 {
                    // new to rotate because my right subtree is shorter than my left
                    self.on_node_visit(&r.left, operation, true);

                    // I know my left child is not a leaf, because it is taller than my right
                    if let Node::Internal(left_child) = self.tree().copy(&r.left) {
                        if left_child.balance > 0 {
                            // double right rotate
                            // I know leftChild.right is not a leaf, because leftChild has a higher subtree on the right
                            self.on_node_visit(&left_child.right, operation, true);
                            (self.double_right_rotate(r_node, &r.left, &new_right), true)
                        } else {
                            // single right rotate
                            let new_right_child = InternalNode::update(
                                r_node,
                                &left_child.right,
                                &new_right,
                                -left_child.balance - 1,
                            );
                            let new_rbalance = 1 + left_child.balance;
                            let new_r = InternalNode::update(
                                &r.left,
                                &left_child.left,
                                &new_right_child,
                                new_rbalance,
                            );
                            (new_r, new_rbalance == 0)
                        }
                    } else {
                        panic!("Not internal node");
                    }
                } else {
                    // no rotation, just recalculate r.balance and childHeightDecreased
                    let new_balance = if child_height_decreased {
                        r.balance - 1
                    } else {
                        r.balance
                    };
                    (
                        InternalNode::update(r_node, &r.left, &new_right, new_balance),
                        child_height_decreased && new_balance == 0,
                    )
                }
            }
        } else {
            panic!("Not internal node");
        }
    }
}
