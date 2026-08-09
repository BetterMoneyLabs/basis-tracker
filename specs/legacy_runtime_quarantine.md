# Legacy v1 runtime quarantine

The pre-v2 redemption runtime is structurally absent from production APIs.
Historical specifications remain lineage material only.

## Enforced boundary

- `basis_store` exports no v1 redemption manager, request, builder, proof
  response, or `TrackerStateManager` proof method.
- `TrackerStateManager` has no global reserve AVL mirror and no
  lookup/insert/update/digest API for redeemed state.
- The server actor has no note-proof, tracker-proof, reserve-proof, reserve
  update-proof, or reserve-digest command variant.
- No `REPAIR_RESERVE_*` startup environment path exists.
- The CLI has no transaction-generation module; its remaining v1 client and
  note-redemption methods are unconditional pre-network tombstones.
- The TUI has no redemption or transaction screen, menu item, or navigation
  variant.
- The ignored local-sign v1 fixture is removed.

Crate-level `compile_fail` examples and source-search integration tests guard
these absences. Generic note, signature, tracker-state, and fixed-tree tests
remain active; removing v1 redemption does not remove their coverage.

## HTTP compatibility

Exactly nine legacy endpoints remain visible so stale clients fail closed:

- `POST /redeem`
- `POST /redeem/complete`
- `GET /proof/redemption`
- `GET /tracker/proof`
- `GET /reserve/proof`
- `POST /tracker/signature`
- `POST /redemption/prepare`
- `POST /redemption/build`
- `POST /redemption/submit`

Every handler returns `410 Gone` without parsing request bodies or query
parameters and without access to application state. `GET /proof` is not a
route. The OpenAPI document marks all nine operations deprecated and documents
only their error response.

## V2 dependency

V2 state is reserve-scoped BRS2 state, not one process-global redeemed tree.
There is no automatic conversion from a v1 reserve mirror. V2 remains dormant
until a sealed confirmed-chain authority supplies the exact observation used
by manifest admission and a private prover/signer consumes only the validated
manifest and its exact reserve and funding boxes.

Current v2 admission does not implement proving, signing, submission,
broadcast, reconciliation, migration, or deployment.
