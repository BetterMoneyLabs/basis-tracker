# Confirmed-chain reconciliation

Tracker publication is an external effect. A successful HTTP response, an
unspent output, or a transaction lookup alone is not confirmation and must not
change redeemable accounting.

## Scope

This implementation applies only to tracker-box publications. Reserve
settlement is intentionally outside this path: it requires a complete signed
claim, payout, token-order, R4-R9, and contract-generation manifest before an
accounting effect can be defined. Enabling the incomplete v2 path fails at
startup.

## Acceptance invariant

A tracker publication may be applied only when one private validated ticket
binds all of the following:

- the locally journaled signed transaction and its locally derived transaction
  id;
- the exact canonical predecessor box consumed once by that transaction;
- the unique signed successor carrying the protocol NFT as a singleton at
  token index zero;
- unchanged value, ErgoTree, ordered assets, R4, and R6-R9;
- an exact R5 AVL constant encoded as `0x64 || 33-byte-root || 0x03 0x20 0x00`;
- a full block whose canonical header is the selected header at the reported
  inclusion height and whose transaction list contains the exact transaction;
- the header `transactionsRoot`, recomputed with the Ergo block-version rule;
- every canonical header and parent link from the inclusion block to one
  coherent node tip observed identically before and after collection; and
- a configurable minimum successor depth, maximum evidence age, and explicit
  reorg-monitoring horizon.

For block version 1, each transaction Merkle leaf is the transaction id,
derived as the Blake2b-256 hash of `bytes_to_sign`. For later versions, the raw
leaf order is every transaction id followed by every witness serialized id.
Each witness id is `Blake2b256(concat(input spending proofs)).tail`, a 31-byte
leaf. Transaction and witness leaves are grouped, not interleaved; raw proofs,
full 32-byte proof hashes, and `transaction-id || proof` leaves are rejected.
This follows Ergo node v6.0.3 commit `28ebb184`.

Node transaction metadata is used only to locate evidence. The block
association is established by the selected header, full block, exact
transaction membership, and transactions root.

## Durable ordering and recovery

The single-writer journal stores one checksummed, synchronously persisted state
record. Its separate checksummed manifest is bound to the exact tracker NFT,
protocol generation 1, and the derived BNS1 state identity. A missing manifest
may be created only with one-shot explicit approval for a history-free BNS1
generation. Existing confirmed metadata or a pending publication requires the
exact existing manifest; a missing, orphaned, or differently bound journal is
rejected before replacement state is written.

Every accepted effect records the complete finality policy snapshot: policy id
and version, acceptance depth, evidence lifetime, reorg horizon, named network,
and a digest of the configured node endpoint. It also records a digest of the
exact canonical evidence and a domain-separated decision digest covering the
effect and policy. Rollback and retirement tickets carry and revalidate the
same snapshot. A restart under a different policy, horizon, network, or source
is rejected; changing those values requires an explicit versioned migration or
fresh acceptance rule.

BNS1 also stores one checksummed global projection receipt containing the exact
transaction, successor box, block, inclusion height, accepted depth, intent,
and AVL root. At startup that receipt must join an identical private journal
effect in both directions: BNS1 history without an effect, and an applied or
retired journal effect without BNS1 history, are rejected before node I/O. The
only absent-projection exceptions are an `AcceptanceReady` effect with the
exact still-armed transaction/root receipt, which can be replayed, and a
durable rollback, whose safe outcome is local demotion.

The historical AVL root is reconstructed in note insertion order from every
persisted confirmed value and must reproduce the global receipt. Revalidation
restores only values that still equal their historical confirmed value. It
never overwrites a newer live AVL root; changed and later values remain
`LocalOnly` or `Pending`.

A new publication uses this order:

1. obtain an actor fence for one local AVL root;
2. construct and sign the transaction;
3. durably record the exact transaction id in the actor state;
4. persist the full signed intent as `Prepared`;
5. persist `SubmissionArmed` immediately before the request may cross the node
   boundary;
6. broadcast the exact journaled bytes;
7. collect and validate active-chain evidence;
8. persist the validated effect as `AcceptanceReady`;
9. idempotently apply the private ticket to the actor; and
10. persist `Applied` and clear the advisory pending cache.

Recovery from `Prepared` may submit the exact stored bytes. Recovery from
`SubmissionArmed` only queries the exact derived transaction id, because a
crash or timeout leaves the broadcast outcome unknown. A 404 remains pending;
timeouts and malformed or incoherent evidence never release the fence. This
can remain an availability wait indefinitely if the configured node loses the
transaction: the implementation never releases the fence or constructs a
competing successor, and does not yet schedule exact-byte rebroadcast retries.
Malformed successful responses and other integrity failures terminate and
quarantine the publisher. A transport failure while an already accepted anchor
is being revalidated does the same: all tracker-state and confirmation
consumers share that one-way health gate, so a stale `Confirmed` value cannot
remain readable after the sole reorg watcher stops.

If the actor receipt exists at `AcceptanceReady`, its transaction id and root
must exactly match the journal, while the journal independently revalidates
every effect field against the retained signed intent, policy/evidence digest,
decision digest, and transition-event history. An absent receipt is permitted only because the
actor may already have completed the idempotent apply before the journal moved
to `Applied`; the actor then verifies the complete persisted provenance before
accepting replay. Any other join fails closed.

Durable local actions are consumed before any node request. In particular, a
crash after recording `AcceptanceReady` resumes the exact actor apply, and a
crash after recording a rollback demotes the removed projection even while the
node is unavailable.

## Reorganizations and bounded retirement

Before the configured horizon, every applied anchor is revalidated through the
same coherent selected-chain proof used for acceptance. If the selected block
at the original inclusion height changes, a rollback ticket is derived from
that proof, persisted, and applied idempotently. Rollback clears the removed
confirmation provenance and makes the affected value non-redeemable.

At the exact monitoring horizon, the node is sampled before and after an
inclusive selected-chain window from `inclusion` through
`inclusion + horizon`. The window is bounded to at most 4096 canonical linked
headers. A changed first block produces the same typed rollback. An unchanged
first block produces a private durable retirement ticket. Retired anchors are
restored locally from that ticket after restart and are never polled again;
this is the application's explicit bounded reorg assumption, not a consensus
finality statement. Consequently, anchors older than the horizon require
constant bounded I/O rather than a chain slice proportional to their age.

A validated rollback has recovery priority over a newer pending publication.
The older projection is demoted first; the newer signed ticket remains durable
and resumes its prior `Prepared` or `SubmissionArmed` rule afterward.

## Policy configuration

- minimum successor depth: 6;
- maximum evidence age: 60 seconds;
- per-request timeout: 15 seconds; and
- v2 reconciliation activation: disabled and fail-closed.

`confirmed_chain_reorg_monitor_depth` has no runtime default. It must be
configured explicitly, must be at least the acceptance depth, and must be at
most 4095. The example configuration leaves the illustrative `720` commented
out pending maintainer review; omitting it disables publication fail-closed.
Journal creation likewise requires the separate one-shot
`allow_fresh_reconciliation_journal` approval.

These are application-finality controls, not consensus finality claims.

## Bounded node and actor integration

This integrated branch contains the bounded-node-request work rooted at exact
commit `248929c5dbb923e4ce7e3530374f4fe66be13fbc`. Every reconciler evidence
request (`/info`, `/blocks/chainSlice`, `/blockchain/transaction/byId`,
`/blocks/{id}`, and `/blockchain/box/byId`) and every tracker transaction
request consumes the same process-wide client, concurrency budget, total
deadline, and two-mebibyte buffered-body cap. `get_node_bytes` may impose a
shorter reconciliation deadline around that shared executor but cannot bypass
its admission or body limit.

The actor exchanges in `begin_publication`, `abort_publication`,
`record_publication_attempt`, `confirm_publication`, and
`rollback_publication` use the same bounded queue-admission and response-deadline
helper as public handlers. A queue, worker, or reply failure therefore returns a
fail-closed result to the updater; callers retain or quarantine the publication
fence according to the durable phase instead of waiting indefinitely.

These joins close the reconciler's resource-bound dependency. They do not, by
themselves, establish the separate tracker signing/witness boundary or make the
configured finality horizon a maintainer-approved deployment policy.
