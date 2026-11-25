# N19: History-aware reserve admission (fresh-DB recovery)

> **Status: implemented** on branch `pr21-merge-master` (2026-11-19), as planned.
> One simplification: no wiring through `crates/basis_server/src/config.rs` was
> needed — `ergo.scanner` is deserialized straight into the scanner `NodeConfig`,
> so `admission_walk_depth` (serde default 512) flows through automatically.
> A review pass (2026-11-23) additionally made the walk cost exactly one node
> request per generation (it previously fetched each creating transaction
> twice) and collapsed `decide_admission` to its single varying parameter.

## Context

Reserve admission control (issue #19, `crates/basis_store/src/ergo_scanner.rs`,
`admit_reserve_box`) admits a box only as (1) the tracked box itself, (2) a verified
successor of the tracked box within `LINEAGE_WALK_DEPTH = 8` generations, or (3) a
**root** whose creating transaction spends a P2PK input of the R4 owner key and whose
R5 is the empty tree with R7 == 0. Paths 1–2 depend on local state. On a fresh
database (wipe, new deployment against an existing chain, #18-style migration) every
reserve whose current box was created by a top-up/redemption/refund-initiation fails
root admission and is **permanently quarantined** ("once quarantined, always
quarantined"), so creditors can no longer get `/redemption/build` for that reserve.
The 8-generation bound likewise quarantines legitimate reserves after long tracker
downtime. Today only a documented manual DB recovery exists
(`specs/ergo/reserve_admission.md` "Fresh-resync caveat").

Goal: admission must be decidable from the **chain alone** — a box that verifiably
descends from an authenticated root is admitted regardless of local history.

## Approach (recommended)

Add a **history walk** to the scanner: from the scanned box, walk creating
transactions backwards (following only inputs at the reserve contract address, the
same rule `is_lineage_successor` uses) until a generation whose creating transaction
has **no** contract-address input — that transaction is the deployment, and the box
it created is the root. Admit the scanned box iff the root passes the same checks as
live root admission: R5 == `empty_reserve_tree_register()`, R7 == 0, and the
deployment transaction spends a P2PK input of the R4 owner key. This is sound because
the on-chain contract enforces register continuity for every successor generation
(R4/R6 constant, R5/R7 per-action rules), so a verified chain to an authenticated
root needs no further validation; a grief dust box fails fast (its creating
transaction has no contract input, so it is judged as a root and fails the P2PK/C4
checks exactly as today).

### Key changes (all in `crates/basis_store/src/ergo_scanner.rs` unless noted)

1. **Extend `fetch_tx_inputs`** to return a struct
   (`TxInputInfo { box_id, address, creating_tx_id, ergo_tree, registers }`) instead
   of a 4-tuple, so the walk can read the root box's R5/R7 from the indexed input
   JSON (extraIndex returns full box data for inputs; no new endpoint). Update the
   two existing callers (`is_lineage_successor`, `has_owner_p2pk_input`)
   mechanically.

2. **New `verify_authenticated_lineage(scan_box, reserve_info, contract_address)`**:
   walks as described above (bounded, see config below) and returns
   `Ok(true)` (authenticated root reached), `Ok(false)` (chain ended at a root that
   fails validation, or no contract lineage at all), `Err` on node errors.
   - Root validation reuses `empty_reserve_tree_register()` and `p2pk_ergo_tree`;
     R7 absent or serialized Long 0 (`0500`) counts as 0.

3. **Rewire `admit_reserve_box`**:
   - *Untracked (root candidate)*: keep the fast path (C4-clean + P2PK proof →
     admit; node error → Retry). When it fails, run the history walk instead of
     quarantining immediately: `Ok(true)` → admit, `Ok(false)` → quarantine
     (new reason text covering "no authenticated lineage"), `Err`/depth-exceeded →
     **Retry** (never quarantine on inconclusive walks).
   - *Tracked, successor check `Some(false)`* (includes the >8-generation downtime
     case): run the history walk as a fallback. Admit only if the walk succeeds
     **and** the currently tracked box is spent on-chain — checked via
     `GET /blockchain/box/byId/{tracked_box_id}` (`spentTransactionId` present on
     the indexed box JSON). This admits the true chain tip after long downtime
     while still quarantining clone-sibling outputs (a sibling of a live action's
     real successor has an authenticated chain but the tracked box is unspent →
     quarantined, matching today's first-admitted-wins behavior).
   - *Tracked, successor `Some(true)` / `None`*: unchanged (admit / Retry).

4. **One-time quarantine migration**: on startup, clear all persisted quarantine
   records (versioned marker in the reserve-storage metadata, same pattern as the
   `reserve_avl_journal` v1→v2 versioning in `persistence.rs`), so boxes quarantined
   by the old rules are re-evaluated by the new walk. Safe: the walk is purely
   chain-determined, so grief boxes are re-quarantined after one fast failed walk,
   and re-evaluation cannot be raced (it does not depend on local tracking state —
   the property the original stickiness guarded). Adds `clear_quarantined()` +
   marker get/set to `crates/basis_store/src/persistence.rs`; the migration runs
   where the scanner/storage is constructed.

5. **Config**: `ergo.scanner.admission_walk_depth` (default **512**) bounding the
   walk; exhaustion → Retry with a warn naming the config knob. Wire through
   `crates/basis_server/src/config.rs` → `NodeConfig` (in `ergo_scanner.rs`) and
   document in `config/basis.toml.example`.

### Tests (existing mock-node harness in `ergo_scanner.rs` tests)

Extend `tx_json` mock inputs with optional `additionalRegisters`, then:

1. Fresh DB, box with 2+ generations of history, root deployment with owner-P2PK
   input → **admitted** (the core N19 case).
2. Fresh DB, history box whose root lacks the P2PK input → quarantined.
3. Grief dust box (no contract lineage) → quarantined (walk fast-fail).
4. Current box with pending refund (R7 ≠ 0) and valid history → **admitted**
   (N19 pending-refund sub-case).
5. Tracked reserve, >8 missed generations: successor check false, walk true,
   tracked box spent (mock `box/byId` with `spentTransactionId`) → admitted.
6. Clone sibling: tracked box unspent + walk true → quarantined.
7. Walk-depth exhaustion → Retry (not quarantine).
8. Migration: a pre-existing quarantine record is cleared at startup and the box
   is re-admitted on next ingest.

### Docs

- Rewrite `specs/ergo/reserve_admission.md` "Fresh-resync caveat" → describe
  history-aware admission, the walk bound, the spent-check rule, and the remaining
  limitation: **rotated reserves** (N9 roots with deliberately pre-filled R5) still
  fail validation on a fresh DB by design (C4-conservative); their `pending_root`
  records must be backed up, or recovery stays manual.
- No OpenAPI/endpoint changes.

### Verification

```bash
cargo test -p basis_store ergo_scanner --offline
cargo test --workspace --offline
cargo clippy --workspace --offline   # compare warning set against baseline
```

## Alternatives considered

- **Minimal version**: history walk only for untracked root candidates; no successor
  fallback, no quarantine migration (existing quarantined boxes stay manual-recovery).
  Less code, but leaves the >8-generation quarantine and all currently-bricked
  reserves in place.
- **Server-side revalidation endpoint** (`POST /reserves/quarantined/{id}/revalidate`)
  instead of the startup migration: operator-driven, but every fresh deployment
  would need the manual step the finding complains about; the chain-determined walk
  makes automatic re-evaluation safe, so the endpoint adds API surface for no
  security gain.

## Assumptions

- The scanner node runs `extraIndex = true` (already required), so
  `/blockchain/transaction/byId` inputs carry full register data and
  `/blockchain/box/byId` returns spent boxes with `spentTransactionId`.
- Rotated (pre-filled R5) roots remain rejected on fresh syncs — relaxing that to
  "P2PK-proof is enough" would re-open the C4 fake-history vector for owner keys;
  kept conservative, documented as a limitation.
- Default walk depth 512 (one node GET per generation, once per reserve per fresh
  sync) is acceptable; it is configurable for pathological cases.
- Clone-sibling outputs at the contract address (possible because the contract
  takes the successor's output index from context var #0) remain handled by
  first-admitted-wins, as today; the walk does not make this worse.
