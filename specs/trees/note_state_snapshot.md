# Authoritative Note State Snapshot

## Persistence invariant

The tracker persists one versioned `BNS1` value in the `iou_notes` partition.
That value contains:

1. the expected 33-byte AVL root digest;
2. the ordered live set of issuer-recipient notes; and
3. each note's tracker-derived redeemed progress.

The vector position is the key's immutable first-insertion order. Updating a
signed cumulative-debt successor or settlement progress replaces the record in
place. It never appends mutation history. The live-note count is capped at
50,000, making disk use, strict reads and restart work `O(K)` in live edges.

Before a mutation is acknowledged, the tracker builds an isolated AVL
candidate, serializes the corresponding complete snapshot and expected root,
replaces the single authoritative value, and calls
`Keyspace::persist(PersistMode::SyncData)`. No Fjall multi-key batch is used for
this note-state boundary.

## Single writer and unknown outcomes

Opening note storage obtains an exclusive file lock for that database path.
The server owns one `TrackerStateManager`; scanner clones do not reopen the
note database. A second in-process or cross-process writer is rejected.

If the authoritative write or durability call fails after mutation begins, the
outcome is treated as unknown. The manager becomes quarantined and rejects
reads, mutations and publication of its root. Recovery requires dropping the
manager, reopening the store, strictly validating the durable snapshot, and
rebuilding the AVL tree before any state is exposed.

## Recovery

Startup parses the entire authoritative value with exact length and count
checks, rejects duplicate logical edges and verifies every issuer signature.
It rebuilds a fresh AVL tree in the stored order and requires the resulting
digest to equal the persisted root. Redeemed progress must not exceed signed
cumulative debt. Only after all checks pass does the candidate replace the
live tree.

Unexpected rows, a missing state value, an unsupported schema, malformed
records, invalid signatures, duplicate edges, order/root mismatch or an
out-of-range redeemed amount fail closed.

## Legacy data

A database containing legacy per-note rows is returned as
`MigrationRequired`; the runtime does not reorder, rewrite or delete it.
Operators must choose one of two separately reviewed procedures:

- export and import the original order while proving the intended tracker-box
  root; or
- preserve the old database as evidence, retire that tracker generation, start
  a fresh generation, and have issuers resubmit signed cumulative notes.

## Settlement boundary

`amount_redeemed` is local settlement state and is not covered by the issuer's
signature. The generic note-ingestion path always initializes it to zero and
preserves existing progress across signed successors. Only the internal,
checked settlement transition may advance it; signed fields remain unchanged.

This snapshot establishes local note/root consistency. It does not by itself
prove transaction inclusion, confirmation depth, active-chain lineage, reserve
successor validity or reorg rollback. Those properties belong to the confirmed
settlement reconciler and its versioned chain-evidence journal.
