# Basis Server Crate Specification

## Current runtime boundary

`basis_server` exposes note, reserve-observation, acceptance-policy, event, and
tracker-state APIs. It does not expose an active redemption builder, proof
service, tracker signer, wallet proxy, transaction submitter, or settlement
completion command.

The server keeps an actor for active note and confirmation-state operations.
The actor does not contain the retired v1 proof commands and
`TrackerStateManager` does not own a global reserve redeemed-state tree.
Reserve-scoped v2 redeemed state belongs to BRS2 and is not synthesized from
legacy process memory or environment variables.

## Active components

1. Axum HTTP routing for active note, reserve-observation, status, event, and
   acceptance-policy endpoints.
2. A tracker actor for note storage and confirmation records.
3. Scanner and reserve-observation state.
4. Tracker-box observation/update support, subject to its separately reviewed
   confirmation and publication boundaries.
5. Configuration with explicit generation-sensitive reserve contract identity.

V2 reserve construction and redemption stay disabled when confirmed scanner
authority, BNS2/BRS2 state, or the exact v2 builder dependency is absent.

## Retired redemption compatibility surface

Exactly nine routes remain as unconditional compatibility tombstones:

| Method | Route |
| --- | --- |
| `POST` | `/redeem` |
| `POST` | `/redeem/complete` |
| `GET` | `/proof/redemption` |
| `GET` | `/tracker/proof` |
| `GET` | `/reserve/proof` |
| `POST` | `/tracker/signature` |
| `POST` | `/redemption/prepare` |
| `POST` | `/redemption/build` |
| `POST` | `/redemption/submit` |

Every handler returns `410 Gone` before request-body or query parsing. The
tombstone router is constructible without `AppState`, so it cannot reach the
actor, storage, scanner, node, signer, or broadcast effects. `GET /proof` is
absent.

The v1 request and success-response schemas are not part of the server models
or OpenAPI components. The OpenAPI document marks all nine routes deprecated
and documents only the standard error envelope.

## Active tracker commands

The production actor accepts only note and confirmation-state commands:

- add and query notes;
- read per-note or aggregate confirmation records;
- mark notes pending, confirm them, revert pending state, and reconcile a
  confirmed tracker digest.

It has no generic note-proof, tracker lookup-proof, reserve lookup-proof,
reserve update-proof, or reserve-root command.

## Configuration and storage

- `server.data_dir` is the base directory for persistent server state.
- `ergo.node.node_url` must be configured for runtime node access.
- Reserve contract identity is generation-sensitive; missing, legacy, or
  unknown construction identity fails closed.
- `REPAIR_RESERVE_*` variables are unsupported and ignored because no global
  v1 reserve AVL state exists.

## V2 admission non-claims

The separately versioned v2 manifest validator is dormant client admission.
It does not provide a prover, signer, wallet, node submission, broadcast,
confirmed-chain reconciliation, deployment, or migration path. Production
activation requires an opaque sealed chain observation and a private signing
primitive that accepts only the validated manifest and the exact boxes it
commits.
