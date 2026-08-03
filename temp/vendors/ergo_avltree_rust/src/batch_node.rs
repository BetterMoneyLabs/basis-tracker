use crate::operation::*;
use alloc::vec;
use alloc::vec::Vec;
use blake2::digest::Digest;
use blake2::Blake2b;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use core::cell::RefCell;
//use debug_cell::RefCell;
use alloc::rc::Rc;
use core::cmp::Ordering;
use core::fmt;

// Do not use bytes -1, 0, or 1 -- these are for balance
pub(crate) const LEAF_IN_PACKAGED_PROOF: u8 = 2;
pub(crate) const LABEL_IN_PACKAGED_PROOF: u8 = 3;
pub(crate) const END_OF_TREE_IN_PACKAGED_PROOF: u8 = 4;

pub type Balance = i8;
pub type SerializedAdProof = Bytes;
pub type NodeId = Rc<RefCell<Node>>;
pub type Resolver = fn(&Digest32) -> Node;
pub type Blake2b256 = Blake2b<blake2::digest::typenum::U32>;

#[derive(Debug, Clone)]
pub struct NodeHeader {
    pub visited: bool,
    pub is_new: bool,
    pub label: Option<Digest32>,
    pub key: Option<ADKey>,
}

#[derive(Debug, Clone)]
pub struct InternalNode {
    pub hdr: NodeHeader,
    pub balance: Balance,
    pub left: NodeId,
    pub right: NodeId,
}

#[derive(Debug, Clone)]
pub struct LeafNode {
    pub hdr: NodeHeader,
    pub value: ADValue,
    pub next_node_key: ADKey,
}

#[derive(Debug, Clone)]
pub enum Node {
    LabelOnly(NodeHeader),
    Internal(InternalNode),
    Leaf(LeafNode),
}

const INTERNAL_NODE_PREFIX: u8 = 0;
const LEAF_NODE_PREFIX: u8 = 1;

impl Node {
    pub fn visited(&self) -> bool {
        self.hdr().visited
    }

    pub fn reset(&mut self) -> bool {
        let hdr = self.hdr_mut();
        let was_new = hdr.is_new;
        hdr.is_new = false;
        hdr.visited = false;
        was_new
    }

    pub fn mark_visited(&mut self, visited: bool) {
        let hdr = self.hdr_mut();
        hdr.visited = visited;
    }

    pub fn is_new(&self) -> bool {
        self.hdr().is_new
    }

    pub fn get_label(&self) -> Digest32 {
        self.hdr().label.unwrap()
    }

    pub fn label(&mut self) -> Digest32 {
        if let Some(label) = self.hdr().label {
            return label;
        }
        match self {
            Node::LabelOnly(hdr) => hdr.label.unwrap(),
            Node::Leaf(node) => {
                let mut hasher = Blake2b256::new();
                hasher.update(&[0u8; 1]);
                hasher.update(&node.hdr.key.as_ref().unwrap()[..]);
                hasher.update(&node.value[..]);
                hasher.update(&node.next_node_key[..]);
                let mut label: Digest32 = Default::default();
                label.copy_from_slice(&hasher.finalize());
                node.hdr.label = Some(label);
                label
            }
            Node::Internal(node) => {
                let mut hasher = Blake2b256::new();
                hasher.update(&[1u8; 1]);
                hasher.update(&[node.balance as u8; 1]);
                hasher.update(node.left.borrow_mut().label());
                hasher.update(node.right.borrow_mut().label());
                let mut label: Digest32 = Default::default();
                label.copy_from_slice(&hasher.finalize());
                node.hdr.label = Some(label);
                label
            }
        }
    }

    pub fn left(&self) -> NodeId {
        if let Node::Internal(node) = self {
            node.left.clone()
        } else {
            panic!("not internal node");
        }
    }

    pub fn right(&self) -> NodeId {
        if let Node::Internal(node) = self {
            node.right.clone()
        } else {
            panic!("not internal node");
        }
    }
    pub fn balance(&self) -> Balance {
        if let Node::Internal(node) = self {
            node.balance
        } else {
            panic!("not internal node");
        }
    }
    pub fn value(&self) -> ADValue {
        if let Node::Leaf(node) = self {
            node.value.clone()
        } else {
            panic!("not leaf node");
        }
    }
    pub fn key(&self) -> ADValue {
        let hdr = self.hdr();
        hdr.key.as_ref().unwrap().clone()
    }
    pub fn next_node_key(&self) -> ADKey {
        if let Node::Leaf(node) = self {
            node.next_node_key.clone()
        } else {
            panic!("not leaf node");
        }
    }
    pub fn is_leaf(&self) -> bool {
        match self {
            Node::Leaf(_) => true,
            _ => false,
        }
    }
    pub fn is_internal(&self) -> bool {
        match self {
            Node::Internal(_) => true,
            _ => false,
        }
    }
    pub fn new_label(label: &Digest32) -> NodeId {
        Rc::new(RefCell::new(Node::LabelOnly(NodeHeader::new(
            Some(*label),
            None,
        ))))
    }

    // Private methods
    fn hdr(&'_ self) -> &'_ NodeHeader {
        match self {
            Node::LabelOnly(hdr) => &hdr,
            Node::Internal(node) => &node.hdr,
            Node::Leaf(node) => &node.hdr,
        }
    }

    fn hdr_mut(&'_ mut self) -> &'_ mut NodeHeader {
        match self {
            Node::LabelOnly(ref mut hdr) => hdr,
            Node::Internal(ref mut node) => &mut node.hdr,
            Node::Leaf(ref mut node) => &mut node.hdr,
        }
    }

    fn reset_recursive(node: &NodeId) {
        if node.borrow_mut().reset() {
            if let Node::Internal(r) = &*node.borrow() {
                Self::reset_recursive(&r.left);
                Self::reset_recursive(&r.right);
            }
        }
    }
}

impl NodeHeader {
    pub fn new(label: Option<Digest32>, key: Option<ADKey>) -> NodeHeader {
        NodeHeader {
            visited: false,
            is_new: true,
            key,
            label,
        }
    }
}

impl InternalNode {
    pub fn new(key: Option<ADKey>, left: &NodeId, right: &NodeId, balance: Balance) -> NodeId {
        Rc::new(RefCell::new(Node::Internal(InternalNode {
            hdr: NodeHeader::new(None, key),
            left: left.clone(),
            right: right.clone(),
            balance,
        })))
    }

    pub fn update_key(node: &NodeId, key: &ADKey) -> NodeId {
        if let Node::Internal(this) = &mut *node.borrow_mut() {
            if this.hdr.is_new {
                this.hdr.key = Some(key.clone());
            } else {
                return Self::new(Some(key.clone()), &this.left, &this.right, this.balance);
            }
        } else {
            panic!("Not internal node");
        }
        node.clone()
    }

    pub fn update(node: &NodeId, left: &NodeId, right: &NodeId, balance: Balance) -> NodeId {
        if let Node::Internal(this) = &mut *node.borrow_mut() {
            if this.hdr.is_new {
                this.left = left.clone();
                this.right = right.clone();
                this.balance = balance;
                this.hdr.label = None;
            } else {
                return Self::new(this.hdr.key.clone(), left, right, balance);
            }
        } else {
            panic!("Not internal node");
        }
        node.clone()
    }
}

impl LeafNode {
    pub fn update(node: &NodeId, key: &ADKey, value: &ADValue, next_node_key: &ADKey) -> NodeId {
        if let Node::Leaf(this) = &mut *node.borrow_mut() {
            if this.hdr.is_new {
                this.hdr.key = Some(key.clone());
                this.value = value.clone();
                this.next_node_key = next_node_key.clone();
                this.hdr.label = None;
            } else {
                return Self::new(key, value, next_node_key);
            }
        } else {
            panic!("Not leaf node");
        }
        node.clone()
    }

    pub fn new(key: &ADKey, value: &ADValue, next_node_key: &ADKey) -> NodeId {
        Rc::new(RefCell::new(Node::Leaf(LeafNode {
            hdr: NodeHeader::new(None, Some(key.clone())),
            value: value.clone(),
            next_node_key: next_node_key.clone(),
        })))
    }
}

#[derive(Clone)]
pub struct AVLTree {
    pub root: Option<NodeId>,
    pub height: usize,
    pub key_length: usize,
    pub value_length: Option<usize>,
    pub resolver: Resolver,
}

impl AVLTree {
    pub fn new(resolver: Resolver, key_length: usize, value_length: Option<usize>) -> AVLTree {
        AVLTree {
            key_length,
            value_length,
            resolver,
            height: 0,
            root: None,
        }
    }

    pub fn left(&self, node: &NodeId) -> NodeId {
        if let Node::Internal(r) = &mut *node.borrow_mut() {
            self.resolve(&mut r.left)
        } else {
            panic!("Not internal node");
        }
    }

    pub fn right(&self, node: &NodeId) -> NodeId {
        if let Node::Internal(r) = &mut *node.borrow_mut() {
            self.resolve(&mut r.right)
        } else {
            panic!("Not internal node");
        }
    }

    pub fn balance(&self, node: &NodeId) -> Balance {
        node.borrow().balance()
    }

    pub fn label(&self, node: &NodeId) -> Digest32 {
        node.borrow_mut().label()
    }

    pub fn key(&self, node: &NodeId) -> ADKey {
        node.borrow().key()
    }

    pub fn value(&self, node: &NodeId) -> ADValue {
        node.borrow().value()
    }

    pub fn next_node_key(&self, node: &NodeId) -> ADKey {
        node.borrow().next_node_key()
    }

    pub fn visited(&self, node: &NodeId) -> bool {
        node.borrow().visited()
    }

    pub fn is_new(&self, node: &NodeId) -> bool {
        node.borrow().is_new()
    }

    pub fn mark_visited(&self, node: &NodeId, visited: bool) {
        node.borrow_mut().mark_visited(visited)
    }

    pub fn resolve(&self, child: &mut NodeId) -> NodeId {
        let mut resolved_node: Option<NodeId> = None;
        if let Node::LabelOnly(hdr) = &*child.borrow() {
            resolved_node = Some(Rc::new(RefCell::new((self.resolver)(&hdr.label.unwrap()))));
        }
        if let Some(node) = resolved_node {
            *child = node
        }
        child.clone()
    }

    pub fn copy(&self, node: &NodeId) -> Node {
        let n = &mut *node.borrow_mut();
        if let Node::Internal(r) = n {
            let _ = self.resolve(&mut r.left);
            let _ = self.resolve(&mut r.right);
        }
        n.clone()
    }

    pub fn extract_nodes(
        &self,
        extractor: &mut dyn FnMut(&mut Node) -> bool,
    ) -> Option<Vec<NodeId>> {
        if let Some(root) = &self.root {
            let mut set = Vec::new();
            self.extract_nodes_recursive(extractor, root, &mut set);
            Some(set)
        } else {
            None
        }
    }

    pub fn extract_first_node(
        &self,
        extractor: &mut dyn FnMut(&mut Node) -> bool,
    ) -> Option<NodeId> {
        if let Some(root) = &self.root {
            self.extract_first_node_recursive(extractor, root)
        } else {
            None
        }
    }

    fn extract_first_node_recursive(
        &self,
        extractor: &mut dyn FnMut(&mut Node) -> bool,
        node: &NodeId,
    ) -> Option<NodeId> {
        let nr = &mut *node.borrow_mut();
        if let Node::Internal(r) = nr {
            self.extract_first_node_recursive(extractor, &self.resolve(&mut r.left))
                .or(self.extract_first_node_recursive(extractor, &self.resolve(&mut r.right)))
        } else if extractor(nr) {
            Some(node.clone())
        } else {
            None
        }
    }

    pub fn contains(&self, node: &NodeId) -> bool {
        if let Some(root) = &self.root {
            self.contains_recursive(root, &self.key(node), &self.label(node), false)
        } else {
            false
        }
    }

    pub fn contains_key(&self, key: &ADKey, label: &Digest32) -> bool {
        if let Some(root) = &self.root {
            self.contains_recursive(root, key, label, false)
        } else {
            false
        }
    }

    fn contains_recursive(
        &self,
        node: &NodeId,
        key: &ADKey,
        label: &Digest32,
        key_found: bool,
    ) -> bool {
        if &self.label(node) == label {
            true
        } else {
            if let Node::Internal(r) = &mut *node.borrow_mut() {
                if key_found {
                    self.contains_recursive(&self.resolve(&mut r.left), key, label, true)
                } else {
                    match (*key).cmp(r.hdr.key.as_ref().unwrap()) {
                        Ordering::Equal =>
                        // found in the tree -- go one step right, then left to the leaf
                        {
                            self.contains_recursive(&self.resolve(&mut r.right), key, label, true)
                        }
                        Ordering::Less =>
                        // going left, not yet found
                        {
                            self.contains_recursive(&self.resolve(&mut r.left), key, label, false)
                        }
                        Ordering::Greater => {
                            self.contains_recursive(&self.resolve(&mut r.right), key, label, false)
                        }
                    }
                }
            } else {
                false
            }
        }
    }

    fn extract_nodes_recursive(
        &self,
        extractor: &mut dyn FnMut(&mut Node) -> bool,
        node: &NodeId,
        set: &mut Vec<NodeId>,
    ) {
        let nr = &mut *node.borrow_mut();
        if let Node::Internal(r) = nr {
            self.extract_nodes_recursive(extractor, &self.resolve(&mut r.left), set);
            self.extract_nodes_recursive(extractor, &self.resolve(&mut r.right), set);
        } else if extractor(nr) {
            set.push(node.clone())
        }
    }

    fn fmt_recursive(&self, f: &mut fmt::Formatter, node: &NodeId, depth: usize) -> fmt::Result {
        write!(f, "{:1$}", "  ", depth)?;
        match &*node.borrow() {
            Node::Leaf(leaf) => {
                writeln!(
                    f,
                    "At leaf label={:?}, key={:?} next_node_key={:?}, value={:?}",
                    leaf.hdr.label, leaf.hdr.key, leaf.next_node_key, leaf.value
                )
            }
            Node::Internal(r) => {
                writeln!(
                    f,
                    "Internal node label={:?}, balance={:?}",
                    r.hdr.label, r.balance
                )?;
                self.fmt_recursive(f, &r.left, depth + 1)?;
                self.fmt_recursive(f, &r.right, depth + 1)
            }
            Node::LabelOnly(hdr) => {
                writeln!(f, "Label-only node label={:?}", hdr.label)
            }
        }
    }

    pub fn reset(&self) {
        if let Some(root) = &self.root {
            Node::reset_recursive(root);
        }
    }

    pub fn pack(&self, node: NodeId) -> Bytes {
        let mut buf = BytesMut::new();
        match &*node.borrow() {
            Node::Internal(node) => {
                buf.put_u8(INTERNAL_NODE_PREFIX);
                buf.put_i8(node.balance);
                buf.extend_from_slice(&node.hdr.key.as_ref().unwrap());
                buf.extend_from_slice(&node.left.borrow_mut().label());
                buf.extend_from_slice(&node.right.borrow_mut().label());
            }
            Node::Leaf(leaf) => {
                buf.put_u8(LEAF_NODE_PREFIX);
                buf.extend_from_slice(&leaf.hdr.key.as_ref().unwrap());
                if let Some(value_length) = self.value_length {
                    assert!(leaf.value.len() == value_length as usize);
                    buf.extend_from_slice(&leaf.value);
                } else {
                    buf.put_u32(leaf.value.len() as u32);
                    buf.extend_from_slice(&leaf.value);
                }
                buf.extend_from_slice(&leaf.next_node_key);
            }
            Node::LabelOnly(_) => panic!("LabelOnly nodes should not be serialized"),
        }
        buf.freeze()
    }

    pub fn unpack(&self, bytes: &Bytes) -> NodeId {
        let mut buf = BytesMut::from(&bytes[..]);
        match buf.get_u8() {
            INTERNAL_NODE_PREFIX => {
                let balance = buf.get_i8();
                let key = Some(buf.copy_to_bytes(self.key_length));
                let mut left: Digest32 = Default::default();
                buf.copy_to_slice(&mut left);
                let mut right: Digest32 = Default::default();
                buf.copy_to_slice(&mut right);
                InternalNode::new(
                    key,
                    &Node::new_label(&left),
                    &Node::new_label(&right),
                    balance,
                )
            }
            LEAF_NODE_PREFIX => {
                let key = buf.copy_to_bytes(self.key_length);
                let value: Bytes;
                if let Some(value_length) = self.value_length {
                    value = buf.copy_to_bytes(value_length);
                } else {
                    let value_length = buf.get_u32() as usize;
                    value = buf.copy_to_bytes(value_length);
                }
                let next_node_key = buf.copy_to_bytes(self.key_length);
                LeafNode::new(&key, &value, &next_node_key)
            }
            _ => {
                panic!("Unexpected node prefix");
            }
        }
    }

    pub fn positive_infinity_key(&self) -> ADKey {
        Bytes::from(vec![0xFFu8; self.key_length])
    }

    pub fn negative_infinity_key(&self) -> ADKey {
        Bytes::from(vec![0u8; self.key_length])
    }
}

impl fmt::Display for AVLTree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(root) = &self.root {
            self.fmt_recursive(f, &root, 0)
        } else {
            writeln!(f, "Empty tree")
        }
    }
}
