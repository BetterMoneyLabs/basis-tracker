# Reserve admission control (issue #19)

## Problem

R4 of a reserve box is not authenticated on-chain: anyone can create a box at the
reserve contract address with a victim's public key in R4. Before this change, a dust
duplicate of a victim's reserve was tracked alongside the real one.
`ReserveTracker::resolve_issuer_reserve` then failed closed with
`MultipleReservesForDenomination` (PR #14 C3: a note is not bound to a specific reserve
box, so the tracker refuses ambiguous configurations), and the acceptance predicates,
the redemption build/sign path, and the exchange flow all refused the victim — a
one-dust-box denial of service.

Checking creation-time invariants alone (`R7 == 0`, empty R5 tree — the "C4 setup
checks") cannot distinguish a legitimate root from an attacker's front-run dust box,
because the attacker can satisfy those invariants just as well as the victim can.

## Why naive fixes are wrong

- **"Run the C4 validation on every box"** breaks legitimate operation: every reserve
  action (redemption #0, top-up #1, refund initiation #2) spends the reserve box and
  creates a successor. After a redemption R5 is legitimately non-empty; after refund
  initiation R7 is legitimately set. Successors must not be re-validated against
  creation-time invariants.
- **"Oldest box wins"** hands control to the attacker on every top-up/redemption,
  because each successor is *younger* than a dust box the attacker created after the
  victim's root.
- **"C4-clean root wins"** (the v1 rule) still loses to a front-run: the attacker's
  box passes the same setup checks.

## Admission rules

A scanned box at a reserve contract address is parsed (per-box structural validation:
R4 present and a valid curve point, R6 bound to this tracker's NFT, R7 typed as Long,
token slot shape for token reserves) and then admitted only when one of the following
holds:

1. **Refresh** — the box is the currently tracked box itself.
2. **Root** — no reserve is tracked for its `(owner, denomination)`, the C4 setup
   checks pass (`R7 == 0`, R5 is the empty AVL+ tree), **and the box's creating
   transaction (fetched via `GET /blockchain/transaction/byId/{txId}`) spends at least
   one input locked by the P2PK script of the R4 owner key** (ErgoTree
   `0008cd || owner_pubkey`, checked at any input position). Spending such an input
   requires the owner's signature, so only the owner (or someone they co-signed with)
   can open a root reserve for that key.
3. **Successor** — the box's creating transaction spends the tracked box, possibly
   across missed generations: the walk follows reserve-contract inputs backwards up to
   `LINEAGE_WALK_DEPTH = 8` generations, so tracker downtime spanning several reserve
   actions still resolves. The reserve contract enforces successor register continuity
   (`contract/basis.es`: `selfOut.R4 == SELF.R4`, per-action R5/R7 rules), so a
   verified successor needs no further validation — and no P2PK proof, since reserve
   actions (redemption, permissionless top-up) legitimately have no owner P2PK input.

Anything else is **quarantined**: it never enters the tracker (so
`resolve_issuer_reserve` never sees a duplicate) and a record (`box_id`, `owner`,
`denomination`, `reason`, timestamp) is persisted in the `quarantined_reserves`
partition and exposed via `GET /reserves/quarantined`. Quarantine is sticky: a box
once quarantined is never re-evaluated, so an attacker cannot regain admission by
timing a box against a legitimate successor in a later scan cycle.

If the proof fetch is inconclusive (node error fetching the creating transaction), the
box is **pending**: it is neither admitted nor quarantined, and the tracked box for
the same `(owner, denomination)` is not removed as spent that cycle — a transient
node error must not orphan the lineage and brick the issuer.

Both ingestion paths go through admission: `ServerState::process_scan_boxes` (the
scanner loop) and the server's background scanner task (which delegates to
`process_scan_boxes` instead of inserting parsed boxes directly).

`resolve_issuer_reserve` keeps its fail-closed `MultipleReservesForDenomination`
error as defense in depth; with admission control it should be unreachable.

## Why the P2PK proof closes the front-run

The v1 lineage-only rule left one residual attack: creating a C4-clean dust reserve
for the victim *before* the victim's first reserve in a denomination, so the attacker's
box became the legitimate root. Under the P2PK-proof rule that is impossible: the
attacker's creating transaction cannot spend a P2PK(victim) input without the victim's
signature, so the front-run box is quarantined. A victim co-signing such a transaction
out-of-band is equivalent to the victim delegating reserve creation and is out of
scope.

## Deployment tooling guarantee

`basis-cli reserve create --submit` (and the TUI wallet flow, both via
`submit_reserve_to_node` in `crates/basis_cli/src/api.rs`) guarantee the proof input
exists:

- if every unspent wallet box is P2PK(owner), any automatic input selection satisfies
  the rule, so the payload goes to `/wallet/payment/send` unchanged;
- otherwise the inputs are pinned via `/wallet/transaction/send` with `inputsRaw`
  (the P2PK(owner) proof box plus funding boxes; strict input/output balance is met
  with an explicit change output to the owner's address);
- if the wallet holds no P2PK(owner) box at all, a pre-deployment transaction first
  sends 0.001 ERG to the owner's own P2PK address; once it appears in the wallet, the
  pinned deployment spends it. If the node wallet does not hold the owner key, the
  pre-deployment UTXO never appears and the command fails with an explicit error.

Manual submission of the `/reserves/create` payload via `/wallet/payment/send` is only
safe when the wallet's boxes all belong to the owner key; otherwise the node may
select inputs with no P2PK(owner) box and the tracker will quarantine the reserve.

## Fresh-resync caveat

The P2PK proof is required only for **root** admission. A tracked reserve that is
spent forward is re-admitted through the successor rule, and the currently tracked box
through the refresh rule, so day-to-day operation never re-checks the proof. But if
the reserve database is wiped and the tracker re-scans from scratch, a reserve whose
current box was created by a later action (top-up/redemption) has no P2PK input in
*its* creating transaction and will be quarantined. Recovery: wipe to a height before
the reserve's creation, or remove the quarantined record and let the lineage walk
re-admit from a synced ancestor. Operators should treat the reserve DB as state worth
backing up.

## Code map

- `crates/basis_store/src/ergo_scanner.rs`
  - `parse_reserve_box` — per-box structural validation only.
  - `admit_reserve_box` — admission decision (C4 checks + P2PK proof for roots,
    lineage for successors, quarantine stickiness).
  - `p2pk_ergo_tree` / `has_owner_p2pk_input` — the `0008cd || pubkey` proof check.
  - `is_lineage_successor` — bounded backwards walk over `/blockchain/transaction/byId`.
  - `decide_admission` — pure verdict function (unit-tested).
  - `ingest_scan_box` — parse + admit + track/persist + quarantine persistence.
  - `process_scan_boxes` — removal loop keeps boxes whose proof/lineage check is pending.
- `crates/basis_store/src/persistence.rs` — `QuarantinedReserve`,
  `quarantined_reserves` partition on `ReserveStorage`.
- `crates/basis_server/src/reserve_api.rs` — `GET /reserves/quarantined`.
- `crates/basis_cli/src/api.rs` — `submit_reserve_to_node` and helpers
  (`select_deployment_boxes`, `deployment_change`, `create_predeployment_utxo`,
  `send_pinned_deployment`) guaranteeing the P2PK proof input at deployment time.
