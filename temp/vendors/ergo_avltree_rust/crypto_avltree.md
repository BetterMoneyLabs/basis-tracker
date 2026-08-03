## 1. AVL tree.

AVL tree (named after inventors Adelson-Velsky and Landis) is a self-balancing binary search tree.
In an AVL tree, the heights of the two child subtrees of any node differ by at most one; if at any time they differ by more than one, rebalancing is done to restore this property. Lookup, insertion, and deletion all take O(log n) time in both the average and worst cases, where n {\displaystyle n} n is the number of nodes in the tree prior to the operation. Insertions and deletions may require the tree to be rebalanced by one or more tree rotations.

AVL trees are often compared with red–black trees because both support the same set of operations and take `O(log ⁡n)` time for the basic operations. For lookup-intensive applications, AVL trees are faster than red–black trees because they are more strictly balanced.

### 1.1 Insert

When inserting a node into an AVL tree, you initially follow the same process as inserting into a Binary Search Tree. If the tree is empty, then the node is inserted as the root of the tree. In case the tree has not been empty then we go down the root, and recursively go down the tree searching for the location to insert the new node. This traversal is guided by the comparison function. In this case, the node always replaces a NULL reference (left or right) of an external node in the tree i.e., the node is either made a left-child or a right-child of the external node.

After this insertion if a tree becomes unbalanced, only ancestors of the newly inserted node are unbalanced. This is because only those nodes have their sub-trees altered. So it is necessary to check each of the node's ancestors for consistency with the invariants of AVL trees: this is called *retracing*. This is achieved by considering the balance factor of each node.

Since with a single insertion the height of an AVL subtree cannot increase by more than one, the temporary balance factor of a node after an insertion will be in the range `[–2,+2]`. For each node checked, if the temporary balance factor remains in the range from –1 to +1 then only an update of the balance factor and no rotation is necessary. However, if the temporary balance factor becomes less than –1 or greater than +1, the subtree rooted at this node is AVL unbalanced, and a rotation is needed. With insertion as the code below shows, the adequate rotation immediately perfectly rebalances the tree.


In order to update the balance factors of all nodes, first observe that all nodes requiring correction lie from child to parent along the path of the inserted leaf. If the above procedure is applied to nodes along this path, starting from the leaf, then every node in the tree will again have a balance factor of −1, 0, or 1.

The retracing can stop if the balance factor becomes 0 implying that the height of that subtree remains unchanged.

If the balance factor becomes `±1` then the height of the subtree increases by one and the retracing needs to continue.

If the balance factor temporarily becomes `±2`, this has to be repaired by an appropriate rotation after which the subtree has the same height as before (and its root the balance factor 0).

The time required is `O(log n)` for lookup, plus a maximum of `O(log n)` retracing levels (`O(1)` on average) on the way back to the root, so the operation can be completed in `O(log n)` time.


### 1.2 Delete

The effective deletion of the subject node or the replacement node decreases the height of the corresponding child tree either from 1 to 0 or from 2 to 1, if that node had a child.

Starting at this subtree, it is necessary to check each of the ancestors for consistency with the invariants of AVL trees. This is called *retracing*.

Since with a single deletion the height of an AVL subtree cannot decrease by more than one, the temporary balance factor of a node will be in the range from −2 to +2. If the balance factor remains in the range from −1 to +1 it can be adjusted in accord with the AVL rules. If it becomes ±2 then the subtree is unbalanced and needs to be rotated. (Unlike insertion where a rotation always balances the tree, after delete, there may be BF(Z) ≠ 0 (see figures 2 and 3), so that after the appropriate single or double rotation the height of the rebalanced subtree decreases by one meaning that the tree has to be rebalanced again on the next higher level.) The various cases of rotations are described in section Rebalancing.

The retracing can stop if the balance factor becomes ±1 (it must have been 0) meaning that the height of that subtree remains unchanged.

If the balance factor becomes 0 (it must have been `±1`) then the height of the subtree decreases by one and the retracing needs to continue.

If the balance factor temporarily becomes `±2`, this has to be repaired by an appropriate rotation. It depends on the balance factor of the sibling Z whether the height of the subtree decreases by one –and the retracing needs to continue– or does not change (if Z has the balance factor 0) and the whole tree is in AVL-shape.

The time required is `O(log n)` for lookup, plus a maximum of `O(log n)` retracing levels (`O(1)` on average) on the way back to the root, so the operation can be completed in `O(log n)` time.

### 1.3 Rebalancing

If during a modifying operation the height difference between two child subtrees changes, this may, as long as it is < 2, be reflected by an adaption of the balance information at the parent. During insert and delete operations a (temporary) height difference of 2 may arise, which means that the parent subtree has to be "rebalanced". The given repair tools are the so-called tree rotations, because they move the keys only *vertically*, so that the (*horizontal*) in-order sequence of the keys is fully preserved (which is essential for a binary-search tree).

Let X be the node that has a (temporary) balance factor of −2 or +2. Its left or right subtree was modified. Let Z be the higher child (see figures 2 and 3). Note that both children are in AVL shape by induction hypothesis.

In case of insertion this insertion has happened to one of Z's children in a way that Z's height has increased. In case of deletion this deletion has happened to the sibling t1 of Z in a way so that t1's height being already lower has decreased. (This is the only case where Z's balance factor may also be 0.)

There are four possible variants of the violation:

```
	Right Right ==> Z is a right    child of its parent X and BF(Z) ≥ 0
	Left Left   ==> Z is a left     child of its parent X and BF(Z) ≤ 0
	Right Left 	==> Z is a right    child of its parent X and BF(Z) < 0
	Left Right 	==> Z is a left     child of its parent X and BF(Z) > 0
```

And the rebalancing is performed differently:

```
	Right Right 	==> X is rebalanced with a 	simple 	rotation rotate_Left
	Left Left 	==> X is rebalanced with a 	simple 	rotation rotate_Right
	Right Left 	==> X is rebalanced with a 	double 	rotation rotate_RightLeft
	Left Right 	==> X is rebalanced with a 	double 	rotation rotate_LeftRight
```

Thereby, the situations are denoted as C B, where C (= child direction) and B (= balance) come from the set { Left, Right } with Right := −Left. The balance violation of case C == B is repaired by a simple rotation rotate_(−C), whereas the case C != B is repaired by a double rotation rotate_CB.

The cost of a rotation, either simple or double, is constant.

#### 1.3.1 Simple rotation

Code snippet of a simple left rotation:

```
Input: 	X = root of subtree to be rotated left
	Z = right child of X, Z is right-heavy
	    with height == Height(LeftSubtree(X))+2
Result: 	new root of rebalanced subtree

node rotate_Left(node X, node Z) {
    // Z is by 2 higher than its sibling
    t23 = left_child(Z); // Inner child of Z
    right_child(X) = t23;
    if (t23 != null)
        parent(t23) = X;
    left_child(Z) = X;
    parent(X) = Z;
    // 1st case, BF(Z) == 0,
    //   only happens with deletion, not insertion:
    if (BF(Z) == 0) { // t23 has been of same height as t4
        BF(X) = +1;   // t23 now higher
        BF(Z) = –1;   // t4 now lower than X
    } else
    { // 2nd case happens with insertion or deletion:
        BF(X) = 0;
        BF(Z) = 0;
    }
    return Z; // return new root of rotated subtree
}
```

#### 1.3.2 Double rotation

Code snippet of a right-left double rotation:

```
Input: 	X = root of subtree to be rotated
	Z = its right child, left-heavy
	    with height == Height(LeftSubtree(X))+2
Result: 	new root of rebalanced subtree

node rotate_RightLeft(node X, node *Z) {
    // Z is by 2 higher than its sibling
    Y = left_child(Z); // Inner child of Z
    // Y is by 1 higher than sibling
    t3 = right_child(Y);
    left_child(Z) = t3;
    if (t3 != null)
        parent(t3) = Z;
    right_child(Y) = Z;
    parent(Z) = Y;
    t2 = left_child(Y);
    right_child(X) = t2;
    if (t2 != null)
        parent(t2) = X;
    left_child(Y) = X;
    parent(X) = Y;
    // 1st case, BF(Y) == 0,
    //   only happens with deletion, not insertion:
    if (BF(Y) == 0) {
        BF(X) = 0;
        BF(Z) = 0;
    } else
    // other cases happen with insertion or deletion:
        if (BF(Y) > 0) { // t3 was higher
            BF(X) = –1;  // t1 now higher
            BF(Z) = 0;
        } else {
            // t2 was higher
            BF(X) = 0;
            BF(Z) = +1;  // t4 now higher
        }
    BF(Y) = 0;
    return Y; // return new root of rotated subtree
}
```

## 2. Merkle Tree

A hash tree or Merkle tree is a tree in which every leaf node is labelled with the cryptographic hash of a data block, and every non-leaf node is labelled with the cryptographic hash of the labels of its child nodes. Hash trees allow efficient and secure verification of the contents of large data structures. Hash trees are a generalization of hash lists and hash chains.

Demonstrating that a leaf node is a part of a given binary hash tree requires computing a number of hashes proportional to the logarithm of the number of leaf nodes of the tree;[1] this contrasts with hash lists, where the number is proportional to the number of leaf nodes itself. Merkle trees are therefore an efficient example of a cryptographic commitment scheme, in which the root of the Merkle tree is seen as a commitment and leaf nodes may be revealed and proven to be part of the original commitment.

A hash tree is a tree of hashes in which the leaves are hashes of data blocks in, for instance, a file or set of files. Nodes further up in the tree are the hashes of their respective children. For example, in the above picture hash 0 is the result of hashing the concatenation of hash 0-0 and hash 0-1. That is, `hash 0 = hash(hash(0-0) + hash(0-1))` where + denotes concatenation.

Most hash tree implementations are binary (two child nodes under each node) but they can just as well use many more child nodes under each node.

Usually, a cryptographic hash function such as SHA-2 is used for the hashing. If the hash tree only needs to protect against unintentional damage, unsecured checksums such as CRCs can be used.

In the top of a hash tree there is a top hash (or root hash or master hash). Before downloading a file on a p2p network, in most cases the top hash is acquired from a trusted source, for instance a friend or a web site that is known to have good recommendations of files to download. When the top hash is available, the hash tree can be received from any non-trusted source, like any peer in the p2p network. Then, the received hash tree is checked against the trusted top hash, and if the hash tree is damaged or fake, another hash tree from another source will be tried until the program finds one that matches the top hash.[12]


## 3. Cryptographically authenticated dictionary.

A variety of cryptocurrencies are based on a public ledger of the entire sequence
of all transactions that have ever taken place.
Transactions are verified and added to this ledger by nodes called miners. Multiple transactions are
grouped into blocks before being added, and the ledger becomes a chain of such blocks, commonly
known as a blockchain.

If a miner adds a block of transactions to the blockchain, other miners verify that every trans-
action is valid and correctly recorded before accepting the new block. (Miners also perform other
work to ensure universal agreement on the blockchain, which we do not address here.) However, not
only miners participate in a cryptocurrency; others watch the blockchain and/or perform partial
verification (e.g., so-called *light nodes*). It is de-
sirable that these other participants are able to check a blockchain with full security guarantees on
commodity hardware, both for their own benefit and because maintaining a large number of nodes
performing full validation is important for the health of the cryptocurrency. To verify each
transactions, they need to know the balance of the payer’s account.

The simple solution is to have every verifier maintain a dynamic dictionary data structure of
(key, value) pairs, where keys are account addresses (typically, public keys) and values are account
balances. Unfortunately this simple solution doesn't work for very large number of accounts which can not fit in RAM. Cryptographically authenticated data structures can make verifying transactions in the blockchain much cheaper than adding them to the blockchain.

In such a data structure, provers (who are, in our case, miners) hold the entire data structure and
modify it as transactions are processed, publishing proofs that each transaction resulted in the
correct modification of the data structure (these proofs will be included with the block that records
the transaction). In contrast, verifiers, who hold only a short digest of the data structure, verify a
proof and compute the new digest that corresponds to the new state of the data structure, without
ever having to store the structure itself.

The verifier can perform these checks and updates without trusting the prover: the verification
algorithm will reject any attempt by a malicious prover or man-in-the-middle who tries to fool
the verifier into accepting incorrect results or making incorrect modifications. In contrast to the
unauthenticated case discussed above, where the verifier must store the entire data structure, here
verifier storage is minimal: 32 bytes suffice for a digest (at 128-bit security level), while each proof
is only a few hundred bytes long and can be discarded immediately upon verification.

## 4. Our implementation of cryptographically authenticated dictionary.

Lets combine Merkle tree with AVL tree: AVL rebalancing allows to keep logarithmic complexity of operations with
binary tree and  Merkle algorithm allows to incrementally calculate tree digest.
The same tree manipulation code is shared both by prover and verifier implementations.
Internal node of tree contains only key, while leaf node also keeps value and next node key.
Label only node is used as stub, used to fetch node content from persistent storage on demand.

In addition to standard operations with tree: **Lookup**, **Insert**, **Delete**, **Update**, **Iterate**,
our tree allows to collect list of changed nodes. Updates of tree are performed using copy-on-write mechanism (*CoW*),
preserving original tree which allows to easily undo changes.

Another supported optimization is batch mode, when we apply multiple operations at once and get
single prove for them. Size of the prov in this case is eventually smaller than sum of sizes
of individual proves of each operations.

### Basic operations with AVL tree are the following:

- extract_nodes

  Get list of tree nodes matching search predicate

- extract_first_nodes

  Get first node matching search predicate

- top_node

  Returns top node of the tree

- contains

  Checks if tree contains specified node

- digest

  Recursive calculate tree digest

- return_result_of_one_operation

  Apply one operation and return its result

- perform_one_operation

  Locate operation key in the tree and perform operation

### Prover operations:

- removed_nodes

  Returns node that where presented in old tree but are not presented in new tree

- generate_proof_for_operations

  Generates the proof for all the operations

- generate_proof

  Generates the proof for all the operations performed (except the ones that failed)
  since the last generateProof call

- tree_walk

  Visit all tree nodes and apply specified visitor functions to them

- random_walk

  Random leaf from the tree that is not positive or negative infinity

- unauthenticated_lookup

  A simple non-modifying non-proof-generating lookup.

### Verifier operations

- reconstruct_tree

  Reconstruct tree from the proof

### Proof serialization format (proof)

Proof of set of operations consists of serialized visited nodes
plus labels (digests) of other nodes and bit vector directions, storing results
of key comparison operations for internal nodes.
Tree is traversed in depth-first order.
We are storing key and values only for leaf nodes, for internal nodes we store only balance.
For absent branches of the tree we are storing label-only nodes, which contains just digest of this branch.
Format of leaf node depends on whether values are fixed  or varying size. In the last case we have tot store length of the value.
For first (left-most) leaf we store its key and next node key. For all other leaf nodes we store only next key.


First leaf node with varying length value:

| magic | key | next_key | value_length | value_data |
| --- | --- | --- | --- | --- |
| 2(byte) | key_length bytes | key_length bytes | 4 bytes | value_length bytes |

First leaf node with fixed length value:

| magic | key | next_key | value_data |
| --- | --- | --- | --- |
| 2(byte) | key_length bytes | key_length bytes | value_length bytes |


Following leaf nodes with varying length value:

| magic | next_key | value length | value data |
| --- | --- | --- | --- |
| 2(byte) | key_length bytes | 4 bytes | value length bytes |

Following leaf node with fixed length value:

| magic | next_key | value_data |
| --- | --- | --- |
| 2(byte) | key_length bytes | value length bytes |

Internal node:

| balance |
| --- |
| -1,0,1 (byte) |

Label-only node:

| magic | digest |
| --- | --- |
| 3(byte) | 32 bytes |

End of tree:

| magic | directions |
| --- | --- |
| 4(byte) | bit array representing path in the tree |


On first path from top to down we initialize directions vector.
Visited flag is not set on the way down: it is set when we know that
operation is not failed.

Full state of visited nodes, labels for not visited siblings and bit vector with directions
allows verifier to replay and verify operation using only subset of the whole tree.


### Copy on write

All updates of the tree are not done in place. Instead of it, we check `is_new` flag in node header.
If flag is not set, then copy of this node is created and update with specified data. Also
`is_new` bit is set, so any subsequent updates of this node in this batch processing (`generate_proof_for_operations`)
are updated in place. After the end of batch processing `is_new` bit is cleared in all nodes, as well as `visited` flag.

List of changed nodes allows to perform batch update of persistent storage.
For efficiency we maintain two lists of changed nodes: one for nodes which needs to be rechecked
for presence in new tree and one for nodes for which we know for sure that them are not present in new tree.
First list is used for nodes participated in rotate operations and for all other non-lookup operations
other than *Insert*,*Remove* and *InsertOrUpdate*.

### Operations

Supported operations:

* Lookup(key)

  Locate key in the tree.

* UnknownModification(key)

  Locate key in the tree and preserves its old value.

* Insert(key,value)

  Insert new key-value pair in the tree or throw error if such key already exists in the tree.

* Update(key,value)

  Locate key in the tree and update associated value or throw error if key not found.

* InsertOrUpdate(key,value)

  Insert new key-value pair in the tree if there is no such key or update value associated with existed key

* UpdateLongBy(key,delta)

  If the key exists in the tree, add delta to its value, fail if
  the result is negative, and remove the key if the result is equal to 0.
  If the key does not exist in the tree, treat it as if its value is 0:
  insert the key with value delta if delta is positive,
  fail if delta is negative, and do nothing if delta is 0.

* RemoveIfExists(key)

  Remove entry from the tree if such key exists or do nothing otherwise.


### Lazy tree loading

Tree can be loaded from persistent storage lazily (when sibling nodes are loaded on demand).
Initially internal nodes stores just digests of its subtrees. When some of it siblings is accessed,
it is loaded from persistent storage and reference to it is stored in internal node.
So once node is loaded, it stays in memory until tree is destructed or node is deleted.
It means that prover should have enough memory to fit all tree.
Verifier works with subset of the tree, so can be run at nodes with limited amount of RAM.
