# Tracker Security Boundaries

This specification records the current fail-closed wallet, signing,
construction, and settlement boundaries.

## Enforced invariants

1. The HTTP service never submits reserve creation through a tracker-owned
   wallet. The reserve owner must review, sign, and submit any future enabled
   payload with owner authority.
2. All nine legacy redemption routes are unconditional `410 Gone` tombstones
   before body/query parsing and without application state.
3. The server actor has no proof-generation or global reserve-digest command.
   `TrackerStateManager` has no process-global redeemed AVL state and no
   `REPAIR_RESERVE_*` recovery path.
4. The CLI has no transaction-generation module. `note redeem`, the MCP
   compatibility tool, and legacy client methods fail before network, proof,
   signing, submission, broadcast, or persistence effects.
5. The TUI exposes no redemption or transaction screens or navigation.
6. Node acceptance is not confirmed settlement. Only a future confirmed-chain
   reconciler may advance authoritative local settlement state.
7. Exact Basis v2 contract identities can be recognized while runtime
   construction remains disabled. Recognition does not activate a scanner,
   builder, signer, or migration path.

## HTTP compatibility changes

| Surface | Current behavior |
| --- | --- |
| `POST /reserves/submit` | `410 Gone`; no node-wallet request. |
| `reserve create --submit` | Rejected; no tracker wallet proxy. |
| Nine v1 redemption/proof/signature/build/submit routes | Deprecated `410 Gone` tombstones. |
| Generic `GET /proof` | Not routed. |
| V1 success request/response models | Removed from server models and OpenAPI. |
| V2 manifest admission | Dormant Rust-only validator; no prover/sign/submit/broadcast. |

## Settlement hand-off

A future reconciler must authenticate the expected reserve successor, selected
chain inclusion, confirmation policy, and rollback before advancing BRS2 and
note state. A future private prover/signer must accept only the validated v2
manifest and the same exact reserve/funding boxes committed by it.

Until those components are integrated, the repository provides no active
redemption flow and makes no target-node, deployment, migration, confirmation,
reorg-safety, or production-readiness claim.
