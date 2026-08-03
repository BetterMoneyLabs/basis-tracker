# Scrypto [![Build Status](https://travis-ci.org/input-output-hk/scrypto.svg?branch=master)](https://travis-ci.org/input-output-hk/scrypto)

Scrypto is an open source cryptographic toolkit designed to make it easier and safer for developers to use cryptography in their applications.

It was extracted from [Scorex](https://github.com/ScorexProject/Scorex-Lagonaki), open-source modular blockchain & cryptocurrency framework.

Public Domain.

scorex_crypto_avltree is Rust port of AVL tree from scrypto package.

## Authenticated data structures

Scrypto supports two-party authenticated AVL+ trees with the batching compression support and guaranteed verifier efficiency, as described in http://eprint.iacr.org/2016/994. 
The implementation can be found in the `scorex.crypto.authds.avltree.batch` package. 


The overall approach is as follows. The prover has a data structure of (key, value) pairs
and can perform operations on it using `performOneOperation` method. An operation (see `scorex.crypto.authds.avltree.batch.Operation`) is either a lookup or a modification.
 We provide sample modifications (such as insertions, removals, and additions/subtractions from the value of a given key), but users of this code may define their own (such as subtractions that allow negative values, unlike our subtractions). A modification may be defined to fail under certain conditions (e.g., a deletion of a key that is not there, or a subtraction that results in a negative value), in which case the tree is not modified. If the operation succeeds, it returns the value associated with the key before the operation was performed. The prover can compute the digest of the current state of the data structure via the `digest` method. At any point the prover may use `generateProof`, which will produce a proof covering the batch of operations (except the ones that failed) since the last `generateProof`. 

The verifier is constructed from the digest that preceeded the latest batch of operations and the proof for the latest batch. The verifier can also be given optional parameters for the maximum number of operations (and at most how many of those are deletions) in order to guarantee a bound on the verifier running time in case of a malicious proof, thus mitigating denial of service attacks. Once constructed, the verifier can replay the same sequence of operations to compute the new digest and to be assured that the operations do not fail and their return values are correct. Note that the verifier is not assured that the sequence of operations is the same as the one the prover performed---it is assumed that the prover and verifier agree on the sequence of operations (two-party authenticated data structures are useful when the prover and verifier agree on the sequence of operations). However, if the verifier digest matches the prover digest after the sequence of operations, then the verifier is assured that the state of the data structure is the same, regardless of what sequence of operations led to this state.

We also provide `unauthenticatedLookup` for the prover, in order to allow the prover to look up values in the data structure without affecting the proof. 

Here are code examples for generating proofs and checking them. In this example we demonstrate two batches of operations, starting with the empty tree. In the first batch, a prover inserts three values into the tree; in the second batch, the prover changes the first value, attempts to subtract too much from the second one, which fails, looks up the third value, and attempts to delete a nonexisting value, which also fails. We use 1-byte keys for simplicity; in a real deployment, keys would be longer.

* First, we create a prover and get an initial digest from it (in a real application, this value is a public constant because anyone, including verifiers, can compute it by using the same two lines of code)

```rust
  use scorex_crypto_avltree::authenticated_tree_ops::*;
  use scorex_crypto_avltree::batch_node::*;
  use scorex_crypto_avltree::operation::*;
  use scorex_crypto_avltree::batch_avl_verifier::BatchAVLVerifier;
  use scorex_crypto_avltree::persistent_batch_avl_prover::*;

  let prover = BatchAVLProver::new(AVLTree::new(dummy_resolver, key_length=1, value_length=Some(8)));
  let initial_digest = prover.digest();
```

* Second, we create the first batch of tree modifications, inserting keys 1, 2, and 3 with values 10, 20, and 30. We use `com.google.common.primitives.Longs.toByteArray` to get 8-byte values out of longs.

```rust
  let key1 = [1u8; 1];
  let key2 = [2u8; 1];
  let key3 = [3u8; 1];
  let op1 = Operation::Insert(KeyValue {key: key1.clone(), value: 10u64.to_be_bytes()};
  let op2 = Operation::Insert(KeyValue {key: key2.clone(), value: 20u64.to_be_bytes()};
  let op3 = Operation::Insert(KeyValue {key: key3.clone(), value: 30u64.to_be_bytes()};
```

* The prover applies the three modifications to the empty tree, obtains the first batch proof, and announces the next digest `digest1`.

```rust
  prover.perform_one_operation(&op1).unwrap();
  prover.perform_one_operation(&op2).unwrap();
  prover.perform_one_operation(&op3).unwrap();
  let proof1 = prover.generate_proof();
  let digest1 = prover.digest();
```

* A proof is just an array of bytes, so you can immediately send it over a wire or save it to a disk. 

* Next, the prover attempts to perform five more modifications: changing the first value to 50, subtracting 40 from the second value (which will fail, because our UpDateLongBy operation is designed to fail on negative values), looking up the third value, deleting the key 5 (which will also fail, because key 5 does not exist), and deleting the third value. After the four operations, the prover obtains a second proof, and announces the new digest `digest2` 

```rust
  let op4 = Operation::Update(KeyValue {key: key1.clone(), value: 50u64.to_be_bytes()});
  let op5 = Operation::UpdateLongBy(KeyDelta {key: key2.clone(), delta: -40});
  let op6 = Operation::Lookup(key3.clone());
  let op7 = Operation::Remove([5u8; 1]);
  let op8 = Operation::Remove(key3.clone());
  prover.perform_one_operation(&op4).unwrap();
  // Here we can, for example, perform prover.unauthenticated_lookup(&key1) to get 50
  // without affecting the proof or anything else
  prover.perform_one_operation(&op5).unwrap();
  prover.perform_one_operation(&op6).unwrap();
  prover.perform_one_operation(&op7).unwrap();
  prover.perform_one_operation(&op8).unwrap();
  let proof2 = prover.generate_proof(); // Proof only for op4 and op6
  let digest2 = prover.digest();
```

* We now verify the proofs. For each batch, we first construct a verifier using the digest that preceded the batch and the proof of the batch; we also supply an upper bound on the number of operations in the batch and an upper bound on how many of those operations are deletions. Note that the number of operations can be None, in which case there is no guaranteed running time bound; furthermore, the number of deletions can be None, in which case the guaranteed running time bound is not as small as it can be if a good upper bound on the number of deletion is supplied. 

* Once the verifier for a particular batch is constructed, we perform the same operations as the prover, one by one (but not the ones that failed for the prover). If verification fails at any point (at construction time or during an operation), the verifier digest will equal None from that point forward, and no further verifier operations will change the digest.  Else, the verifier's new digest is the correct one for the tree as modified by the verifier. Furthermore, if the verifier performed the same modifications as the prover, then the verifier and prover digests will match.

```rust
  let verifier1 = BatchAVLVerifier::new(
        &initial_digest, &proof1,
		AVLTree::new(dummy_resolver, key_length=1, value_length=Some(8)),
		Some(2),
		Some(0));
  verifier1.perform_one_operation(&op1).unwrap();
  verifier1.perform_one_operation(&op2).unwrap();
  verifier1.perform_one_operation(&op3).unwrap();
  match verifier1.digest() {
    Some(d1) if d1 == digest1 => {
      // If digest1 from the prover is already trusted, then verification of the second batch can simply start here
      let verifier2 = BatchAVLVerifier::new(
        &d1, &proof2,
		AVLTree::new(dummy_resolver, key_length=1, value_length=Some(8)),
		Some(3),
		Some(1));

      verifier2.perform_one_operation(&op4).unwrap();
      verifier2.perform_one_operation(&op6).unwrap();
      verifier2.perform_one_operation(&op8).unwrap();
	  match verifier2.digest() {
        Some(d2) if d2 == digest2 => println!("first and second digest value and proofs are valid"),
		_ => println!("second proof or announced digest NOT valid"),
      }
	}
    case _ =>
      println!("first proof or announced digest NOT valid")
  }
```

# Tests
Run `cargo test` from a folder containing the framework to launch tests.

# License

The code is under Public Domain CC0 license means you can do anything with it. Full license text is in [COPYING file](https://github.com/ScorexProject/scrypto/blob/master/COPYING)

# Contributing

Your contributions are always welcome! Please submit a pull request or create an issue to add a new cryptographic primitives or better implementations.
