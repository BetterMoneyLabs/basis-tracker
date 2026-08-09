# Authoritative Note State Snapshot

## Persistence invariant

The tracker persists one versioned `BNS2` value in the `iou_notes` partition.
That value contains:

1. the expected 33-byte AVL root digest;
2. a Blake2b-256 checksum over the complete snapshot domain, count, root and
   records;
3. the ordered live set of issuer-recipient notes; and
4. each note's tracker-derived redeemed progress.

The vector position is the key's immutable first-insertion order. Updating a
signed cumulative-debt successor or settlement progress replaces the record in
place. It never appends mutation history. The live-note count is capped at
50,000, making disk use, strict reads and restart work `O(K)` in live edges.

Before a mutation is acknowledged, the tracker builds an isolated AVL
candidate, serializes the corresponding complete snapshot and expected root,
replaces the single authoritative value, and calls
`Keyspace::persist(PersistMode::SyncData)`. No Fjall multi-key batch is used for
this note-state boundary.

Before every rewrite, the manager rereads and checksums the complete persisted
snapshot, verifies every issuer signature and redeemed bound, rebuilds every
physical issuer-recipient AVL key, and requires the rebuilt, persisted and live
roots to agree. A valid successor cannot launder a malformed predecessor.

The checksum detects accidental corruption and incomplete writes. It is not a
MAC or adversarial authentication mechanism: a party able to rewrite the data
directory can recompute it. `amount_redeemed` therefore remains trusted local
state until the confirmed-chain reconciler replaces it with replayable,
lineage-bound settlement evidence.

## Single writer and unknown outcomes

Opening note storage obtains an exclusive file lock for that database path.
The server owns one `TrackerStateManager`; scanner clones do not reopen the
note database. A second in-process or cross-process writer is rejected.

If the authoritative write or durability call fails after mutation begins, the
outcome is treated as unknown. The manager becomes quarantined and rejects
reads, mutations and publication of its root. Recovery requires dropping the
manager, reopening the store, strictly validating the durable snapshot, and
rebuilding the AVL tree before any state is exposed.

Quarantine also flips a one-way health signal shared with the tracker-box
updater. A cached pre-failure root is not publishable after the manager enters
an unknown or structurally invalid state.

## External publication receipt

Before a signed tracker-box transaction crosses the node broadcast boundary,
the actor stores a checksummed `BPA1` receipt containing the exact local root,
the 32-byte transaction id derived from the signed transaction, and the
submission height. The publication lease remains held after that write. A
success response releases nothing unless its transaction id exactly matches the
locally derived id.

An HTTP error, malformed response, mismatched id, process crash, or partial
per-note confirmation write leaves the receipt durable. Startup restores the
same fence and polls only that transaction id. Once the exact transaction is
observed, confirmation is replayed idempotently from the complete authenticated
snapshot and the receipt is cleared only after every advisory confirmation row
is durable. An orphaned or malformed receipt blocks fresh-generation
initialization without rewriting the authoritative records.

This receipt prevents a restart from authorizing a competing tracker
publication after an indeterminate broadcast. It does not establish active-chain
lineage, confirmation depth, or reorg safety; those remain gates of the
confirmed-chain reconciler.

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

## Generation binding and bootstrap

The note schema partition contains a checksummed, durable `BNG1` manifest
binding the data directory to exactly one 32-byte tracker NFT, the approved
empty bootstrap root, and (after first observation) its on-chain anchor root.
Opening an unbound data directory requires an explicit fresh-generation
approval. Existing generations open with approval denied, and corruption or a
different configured NFT is rejected.

Before any tracker-box successor may be submitted, the updater asks the state
manager to validate the observed NFT and R5. For an unanchored generation, the
first observed R5 must equal the persisted bootstrap root; otherwise the
manager and publisher are quarantined. This prevents an empty or wrong data
directory from overwriting a non-empty generation under the same configured
NFT.

## Legacy data

A database containing BNS1 or legacy per-note rows is returned as
`MigrationRequired`; the runtime does not reorder, rewrite or delete it.
Operators must choose one of two separately reviewed procedures:

- export and import the original order while proving the intended tracker-box
  root; or
- preserve the old database as evidence, retire that tracker generation, start
  a fresh generation, and have issuers resubmit signed cumulative notes.

## Settlement boundary

`amount_redeemed` is local settlement state and is not covered by the issuer's
signature. The generic note-ingestion path always initializes it to zero and
preserves existing progress across signed successors. No production API or
store method accepts raw settlement scalars; the historical direct-completion
route is a `410 Gone` tombstone and broadcast acceptance does not mutate local
accounting. A future internal transition must consume validated confirmed-chain
evidence rather than caller metadata; signed fields remain unchanged.

This snapshot establishes local note/root consistency. It does not by itself
prove transaction inclusion, confirmation depth, active-chain lineage, reserve
successor validity or reorg rollback. Those properties belong to the confirmed
settlement reconciler and its versioned chain-evidence journal.
