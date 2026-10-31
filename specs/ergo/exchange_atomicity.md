# Exchange Atomicity: Staged To-Edge Application (Issue #17)

## Problem

A cross-denomination exchange (`POST /notes/exchange`) applies **both** edges
immediately to the tracker's live AVL tree: the from-edge decrease and the
to-edge increase. The next tracker box update commits both together.

The on-chain contract checks a redemption against the **committed** tracker
tree, not the live one, and the tracker signs redemption messages for the
**stored** note total. This creates a double-pay window for a malicious
creditor:

1. The creditor pre-builds a redemption transaction for the **old** from-edge
   total (tracker signature + committed lookup proof obtained before the
   exchange; both are over the old state and remain valid until the decrease
   is committed).
2. The creditor submits the exchange. The tracker now holds: from-edge
   decreased (live), to-edge increased (live).
3. The creditor broadcasts the pre-built redemption. The reserve pays the old
   from-edge total on-chain.
4. The next tracker box commit publishes **both** the decrease and the
   increase. The creditor can now also redeem the to-edge increase.

Net result: the creditor redeems the pre-exchange from-edge total **and** the
full to-edge increase. The issuer pays both.

The pre-built transaction cannot be revoked: the creditor can always broadcast
it directly (local-sign flow), so request-time rechecks alone cannot close the
window.

## Design: two-phase exchange

The to-edge increase is **staged**, not applied. Only the from-edge decrease is
applied at request time.

### Phase 1 — request time (`exchange_debt`)

All existing validation is unchanged (four Schnorr signatures, timestamp
monotonicity, contract range, below-redeemed floor, acceptance policy against
pre-mutation state). Then:

1. The **from-edge decrease is applied immediately** (AVL update + note store
   + confirmation recompute). It will be committed by the next tracker box
   update.
2. A `PendingExchangeRecord` is persisted in a new `pending_exchanges`
   partition, keyed by the from-edge note key, containing:
   - both parties' pubkeys and both denominations,
   - `from_old_total` / `from_new_total`,
   - the **signed to-edge note** (`amount_collected = to_new_total`),
   - `to_base_collected` (the to-edge's pre-exchange total, `None` if new),
   - the box id of the issuer's canonical from-denomination reserve at
     exchange time,
   - `created_at`, `status` (`staged` / `cancelled { reason }`).
3. The to-edge AVL entry and note are **left untouched** until phase 2.
4. At most one staged exchange may exist per (issuer, recipient) pair;
   a second request is rejected until the first resolves. (Two staged
   exchanges on overlapping edges would compute their totals against
   inconsistent pre-mutation state.)

The request requires exactly one tracked reserve for the issuer in the
**from** denomination (409 otherwise, same rule as redemption building). The
reserve box id is captured for the phase-2 integrity check.

### Phase 2 — apply, driven by tracker box confirmation

The tracker box updater runs the pending-exchange pass after every
confirmation event (both the `ConfirmPendingNotes` path and the
`ReconcileWithConfirmedDigest` path):

1. `ListStagedExchanges` — the store returns staged records (key + reserve box
   id).
2. The updater fetches each stored reserve box from the node's UTXO set
   (`GET /utxo/byId/{box_id}`) and extracts its R5 digest. A node/transport
   error **skips** the exchange for this cycle (wait); a 404 (box no longer
   unspent) becomes a `None` observation.
3. `ProcessStagedExchanges { observations }` — the store decides per record:

   **Apply** only when all of the following hold:
   - The from-edge's confirmation record is `Confirmed` with
     `confirmed_total_debt == from_new_total` — the decrease is exactly what
     got committed, so the old from-edge note (signed for `from_old_total`) is
     dead on-chain (`trackerTotalDebt != totalDebt`).
   - The observed reserve R5 digest equals the tracker's reserve-tree digest
     for `(issuer, from_denomination)` — no unreported on-chain redemption
     happened against that reserve (reported redemptions advance both sides in
     lockstep; unreported ones advance only the chain).
   - The reserve box is still unspent — a redemption, top-up, or refund
     completion spends the reserve box, so a missing box is conclusive.
   - The to-edge base is unchanged (`amount_collected == to_base_collected`),
     so applying the staged note cannot lower an edge that grew meanwhile. The
     applied note carries the to-edge's **current** `amount_redeemed` forward.
   - The staged to-note's signature re-verifies.

   On apply: to-edge AVL update + note store + confirmation recompute, staged
   record deleted, shared AVL digest refreshed so the updater commits the
   to-edge increase in the next update.

   **Cancel** (record kept with `status = cancelled { reason }`, loud log)
   when:
   - the from-edge confirmed to a value other than `from_new_total`
     (the state moved past the exchange — e.g. the issuer issued new debt on
     the edge before the commit),
   - the reserve box is gone or its R5 digest diverged,
   - the to-edge base changed,
   - the staged signature fails re-verification.

   **Wait** when the from-edge is not confirmed yet or no observation is
   available.

### Why this closes the attack

The to-edge increase can only be applied after the from-edge decrease is
committed. If the creditor broadcast a pre-built old-total redemption before
that, the reserve box was spent and its R5 advanced — the phase-2 check fails
and the increase is cancelled. The creditor ends up with exactly what the old
note paid (the pre-exchange state), never old-total **plus** the increase.
If the creditor did not redeem, the reserve is intact and the exchange
completes normally.

### Residual risk (documented, not fixable off-chain)

A creditor who pre-built an old-total redemption can still redeem it during
the staging window, cancelling the trade ("free option" on the exchange rate).
Eliminating that requires an on-chain exchange action so both edges move in
one transaction; out of scope here.

### Defense in depth: stale committed proof rejection

`generate_committed_tracker_lookup_proof` now rejects
(`NoteError::StaleCommittedDebt`) when the note's confirmed total is **greater
than** its current local total — the only way that happens is an uncommitted
consented decrease. Redemption building against the stale committed value is
refused until the decrease commits, instead of producing a transaction that
would over-redeem and trip the phase-2 cancellation.

## API surface

- `POST /notes/exchange` response gains `to_edge_status: "pending"`; the
  to-edge note only appears (and only becomes redeemable) after phase 2.
- New `GET /exchanges/pending?issuer_pubkey=&recipient_pubkey=` lists staged
  and cancelled exchanges so the creditor can observe progress or cancellation
  reasons. Applied records are removed (the to-edge note's existence is the
  completion signal).

## Restart behavior

Staged records are persisted; after a restart the next confirmation cycle
re-evaluates them (confirmation metadata is rebuilt from storage and
reconciled against the on-chain digest first). A staged exchange whose
from-edge was already committed before the restart is applied on the first
post-restart confirmation pass.

## AVL+ tree ordering

The AVL+ digest depends on **first-insertion order** (updates preserve tree
shape). Both rebuild paths — `rebuild_avl_tree` after a restart and the
committed-tree rebuild behind lookup proofs — insert notes in
timestamp-sorted order. An exchange moves the from-edge's note timestamp
forward, which can reorder it relative to other notes, so the live tree's
shape would no longer match what every rebuild produces: committed lookup
proofs (redemption builds) would fail their digest consistency check until a
restart.

Fix: `exchange_debt` (phase 1) and `apply_staged_exchange` (phase 2) recreate
the live tree from storage in the canonical timestamp-sorted order
(`rebuild_tree_canonically`) after applying their mutations. The resulting
digest change is committed by the next tracker box update like any other
state change; confirmation metadata and rebuilds stay consistent because
every tree now has the same canonical shape.

Known related hazard, out of scope here: any `add_note` on an existing edge
whose new timestamp crosses another note's timestamp has the same effect
(pre-existing). Exchanges are fixed because they are the only mutation this
spec introduces; the general case deserves its own issue.
