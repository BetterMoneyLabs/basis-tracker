# Legacy library API quarantine

The pre-v2 redemption builders are historical test fixtures, not supported
production APIs.

## Enforced boundary

- `basis_store::redemption` and `basis_store::transaction_builder` are compiled
  only for the crate's unit tests.
- `basis_offchain::transaction_builder` is compiled only for the crate's unit
  tests.
- `basis_store` does not export `RedemptionManager`, `RedemptionRequest`, or
  the v1 `RedemptionTransactionBuilder` to downstream crates.
- The server actor owns `TrackerStateManager` directly; it cannot reach the
  retired manager through a production dependency.

The crate-level `compile_fail` examples are regression guards for the public
API boundary. Historical unit and property tests remain available so the old
behavior can still be examined without making it callable by an application.

## Scope and integration dependency

This branch closes only the public Rust library surface described above. It
does not quarantine every server or client runtime path: the assisted
`/redemption/build` and `/redemption/submit` routes belong to the separate
generation-admission workstream. Before integration, those routes must either
be unconditional tombstones or accept only the exact reviewed v2 source,
ErgoTree, P2S, claim domain, register schema, and proof shapes.

There is no automatic migration from legacy reserve state. Operators must
inventory any supported lineage and apply an explicitly reviewed retirement or
migration policy.
