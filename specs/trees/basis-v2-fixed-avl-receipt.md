# Basis v2 fixed AVL receipt

## Scope and pins

The v2 runtime exposes two concrete authenticated-tree types:

- `TrackerAvlTree`: 32-byte keys and fixed 8-byte values;
- `ReserveAvlTree`: 32-byte keys and fixed 24-byte values.

Both shapes allow insert and update and exclude remove. No public const-generic
constructor remains, so another value width cannot be selected by a caller.

The cross-runtime fixture is derived from
`BetterMoneyLabs/chaincash@04031626f09c6590a20ad20d5583c6eccc14412d`
with Scala 2.12.17, plasma-toolkit 1.1.0 and Ergo Appkit 6.0.0. The complete
inputs, roots and proof bytes are stored in
`crates/basis_trees/tests/fixtures/basis_v2_avl_scala_0403162.json`.

## Closed invariants

| Invariant | Producer and consumer | Failure if relaxed | Isolated evidence |
| --- | --- | --- | --- |
| Only 32/8 and 32/24 shapes are callable | concrete Rust types; v2 tracker and reserve R5 builders | an off-chain proof uses metadata rejected by the contract | public API compile-fail doctest and exact shape test |
| Replay preserves first-insertion order | ordered entry vector; prover rebuild | restart can derive a different authenticated root | deterministic replay control and reversed-order negative |
| Lookup always carries membership or non-membership evidence | prover; v2 proof consumer | an absent or mismatched record is accepted without authenticated evidence | membership, absence, key, value, root and proof single-fault negatives |
| Transition witnesses bind the predecessor root and exact operation | temporary prover; successor builder | an update or new-key insert advertises the wrong successor root | independent update and new-key insert transition tests |
| Malformed proof bytes fail as errors | vendored verifier; untrusted proof ingress | input-derived indexing or stack underflow can abort the process | truncation/structure matrix and an abort-mode executable harness |
| Rust output matches the pinned Scala model | Rust prover and ChainCash PlasmaMap | cross-runtime proof or root bytes drift | byte-for-byte 32/8 lookup/update and 32/24 absence/insert golden test |

The pinned Scala proof identities are:

- 32/8 lookup and update: 143 bytes, SHA-256
  `5b3e3451d8137978f6b86ea2021bdc0d8e31a80ecbbe8ed48f9d67e9c1f3ca54`;
- 32/24 absence and insert: 125 bytes, SHA-256
  `80f33ed16eee7c83ba7d316d07a5bb988a2e5c8f83a50487e4698829dfe02d76`.

## Validation commands

```text
cargo test --locked -p basis_trees fixed_avl --lib
cargo test --locked -p basis_trees --doc
RUSTFLAGS="-C panic=abort" cargo run --locked -p basis_trees --example malformed_proof_abort_harness
cargo clippy --locked -p basis_trees --lib --examples -- -D warnings
```

## Cost and evidence boundary

`transition_witness` reconstructs a temporary prover from the authoritative
ordered entries before applying one operation. It therefore performs O(n)
replay operations and uses O(n) temporary memory; with per-entry AVL insertion,
the conservative time bound is O(n log n). Reopening from an ordered snapshot
has the same replay count. This is deliberate for deterministic isolation and
is not a scalability claim. Ordinary in-memory key lookup uses the position
index, while authenticated proof work retains the AVL implementation's
tree-height behavior.

These checks establish source-model and Rust/Scala byte parity for the frozen
fixtures. They do not establish node reduction cost, persistence/reorg safety,
deployment identity or production capacity.
