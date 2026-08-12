# PR #12 finding triage and disposition

External draft PR BetterMoneyLabs/basis-tracker#12 ("Basis remediation stack",
+24.5k lines) reports eight hardening findings. The PR itself is not merged or
ported: it retires all nine v1 redemption endpoints with HTTP 410, which is
incompatible with the live v1 surface this branch maintains (see
`specs/redemption_acceptance_policy.md`). Instead, each finding was checked
against this codebase and the real, still-relevant ones were fixed with our own
implementation.

## Disposition table

| # | PR finding | Real here? | Disposition |
|---|-----------|-----------|-------------|
| 1 | Collateralization could use an unmaintained reserve debt placeholder instead of the issuer's projected cumulative liabilities | **Yes** — `CollateralizationPredicate` read `ExtendedReserveInfo.total_debt`, which is only touched by scanner events and is 0 in practice | **Fixed** |
| 2 | Settlement progress could be advanced without confirmed-chain evidence | Partly — pending notes were promoted to `Confirmed` when the tracker update transaction merely appeared in a block | **Fixed (minimal)** |
| 3 | Persisted note/AVL/publication state lacked reciprocal integrity/restart joins | Partly — `rebuild_confirmations` exists, but there is no root/consistency verification on restart | **Deferred** — the PR's versioned-snapshot/lease machinery is out of scope; needs its own design |
| 4 | Scanner HTTP 404 / partial-page failures could be confused with an authoritative empty snapshot | Partly — fetch errors already propagated (state preserved), but **partially parseable** snapshots triggered reserve removal | **Fixed** |
| 5 | Outbound node requests, response bodies, and actor waits not uniformly bounded | **Yes** — bare `reqwest::Client::new()` with no timeouts or body caps across scanner, updater, redemption build, and API node calls | **Fixed** |
| 6 | Tracker publisher trusted node wallet APIs (signing boundary) | Partly — the updater uses node `/wallet/transaction/sign` (key stays on the node) but trusts wallet-box JSON; local ergo-lib signing exists only for some paths | **Deferred** — full local-signing rework is a separate project |
| 7 | Global v1 reserve AVL/domain model cannot represent reserve generations; v2 bounds | No — the v1 model is the intended design here; v2 is out of scope | **Skipped** |
| 8 | Legacy proof/signature/redemption routes remained callable | No — those routes are the live product surface; this branch extends them rather than retiring them | **Skipped** |

## Fix 1 — acceptance collateralization from real liabilities

- `PredicateContext` gained `issuer_gross_debt: Option<u64>`
  (`crates/basis_server/src/acceptance/mod.rs`). `None` means the liability
  snapshot is unavailable; collateral checks fail closed.
- `check_acceptance` (`crates/basis_server/src/api.rs`,
  `load_issuer_gross_debt`) computes liabilities from tracker note state: per
  recipient edge `max(amount_collected, confirmed_total_debt,
  pending_total_debt)` with checked arithmetic; the candidate note replaces its
  own edge (never lowering an observed value) or is added as a new edge. Gross
  (not net) debt is deliberate: redemption state is not reconstructed on
  reorgs, so subtracting redeemed amounts could fail open.
- `CollateralizationPredicate` uses the snapshot instead of
  `ExtendedReserveInfo.total_debt`; `requires_liability_snapshot()` propagates
  through composite predicates, and `NotPredicate` fails closed instead of
  negating a missing-input evaluation failure.
- `redemption_check.rs` (redemption-time policy check) keeps its **net**
  outstanding-debt semantics by explicit decision — its simulation needs the
  debt that actually leaves the reserve.
- Tests: unit tests in `acceptance/mod.rs` (including fail-closed under
  missing snapshot and short-circuit / fail-closed composite behavior); handler
  regression test `test_acceptance_check_uses_note_derived_liabilities` in
  `crates/basis_server/tests/redemption_api_integration_tests.rs`.

## Fix 2 — bounded node HTTP

- New module `crates/basis_store/src/http.rs`: `bounded_client()` (3 s connect,
  15 s total timeout) and `read_body_capped` / `read_json_capped` (2 MiB body
  cap, `Content-Length` checked plus chunked-read backstop).
- Applied at all node call sites: `ergo_scanner.rs`, `tracker_scanner.rs`,
  `redemption_build.rs` (`NodeClient`), `tracker_box_updater.rs`, and the API
  node calls in `api.rs` (`call_schnorr_sign_api`, reserve submission).
- Tests: client construction, small-body parse, oversized-body rejection,
  chunked-oversized-body rejection, and invalid-JSON rejection in `http.rs`.

## Fix 4 — scanner fail-closed on non-authoritative snapshots

- `process_scan_boxes` (`crates/basis_store/src/ergo_scanner.rs`) now counts
  box parse failures and skips the reserve-removal phase unless **every**
  fetched box parsed. Previously one valid box plus one malformed box would
  cause all reserves absent from the parsed set to be deleted from the tracker
  and database.
- Tests:
  - `test_process_scan_boxes_removes_absent_reserves_on_clean_scan`
  - `test_process_scan_boxes_preserves_reserves_on_empty_scan`
  - `test_process_scan_boxes_preserves_reserves_when_boxes_fail_to_parse`

## Fix 2 (minimal) — minimum confirmation depth

- `TrackerBoxUpdateConfig.min_confirmation_depth` (config `[confirmation]
  min_depth`, default 2): the tracker box updater only promotes pending notes
  to `Confirmed` (making them redeemable) once the update transaction has the
  required depth, instead of treating first-block inclusion as confirmation.
  Box-summary and height fetch failures now retry the next cycle rather than
  confirming with placeholder metadata.
- Reorg handling beyond this depth gate is deliberately out of scope (that is
  the PR's 4k-line reconciler; see finding 3 deferral).
- Tests: `test_confirmation_min_depth_defaults_to_two` in `config.rs`; pure
  `confirmation_depth` helper tests in `tracker_box_updater.rs`.

## Files changed

| Area | Files |
|------|-------|
| Acceptance collateralization | `crates/basis_server/src/acceptance/mod.rs`, `crates/basis_server/src/api.rs` |
| Bounded node HTTP | `crates/basis_store/src/http.rs` (new), `crates/basis_store/src/ergo_scanner.rs`, `crates/basis_store/src/tracker_scanner.rs`, `crates/basis_server/src/redemption_build.rs`, `crates/basis_server/src/tracker_box_updater.rs`, `crates/basis_server/src/api.rs` |
| Scanner fail-closed | `crates/basis_store/src/ergo_scanner.rs` |
| Min confirmation depth | `crates/basis_server/src/tracker_box_updater.rs`, `crates/basis_server/src/config.rs`, `crates/basis_server/src/main.rs`, `config/basis.toml.example` |
| Config plumbing for new sections | `crates/basis_server/src/main.rs`, `crates/basis_server/tests/*.rs`, `crates/basis_server/src/create_reserve_tests.rs` |
| Tests | `crates/basis_server/tests/redemption_api_integration_tests.rs`, `crates/basis_server/tests/acceptance_api_integration_tests.rs`, `crates/basis_store/src/http.rs`, `crates/basis_store/src/ergo_scanner.rs`, `crates/basis_server/src/config.rs`, `crates/basis_server/src/acceptance/mod.rs`, `crates/basis_server/src/tracker_box_updater.rs` |
| Docs | `specs/pr12_triage.md`, `specs/acceptance_predicates.md`, `docs/CONFIGURATION.md` |

## Ancillary fixes discovered during validation

- `crates/basis_server/src/create_reserve_tests.rs`: added a global
  `STORAGE_INIT_LOCK` and removed the same-directory fallback for tracker/policy
  storage initialization. The fallback masked an lsm-tree/fjall creation race
  that intermittently aborted the lib test suite (`panic in a destructor`,
  root cause `assertion failed: !manifest_path.try_exists()?`).

## Validation summary

- `cargo test -p basis_server --lib`: 108 passed
- `cargo test -p basis_store --lib`: 186 passed / 3 ignored
- `cargo test -p basis_server --test http_api_integration_tests`: 9 passed
- `cargo test -p basis_server --test cors_tests`: 10 passed
- `cargo test -p basis_server --test redemption_api_integration_tests -- --test-threads=1`: 27 passed
- `cargo test -p basis_server --test acceptance_api_integration_tests`: 15 passed
- `cargo clippy -p basis_server -p basis_store --tests`: pre-existing warnings only; no new warnings introduced by these changes
- `cargo fmt --check -p basis_server -p basis_store`: clean

Caveat: previously `--workspace` builds were red because of this `basis_mcp`
compile error; it is now fixed by passing `submit = false` to match the tool's
payload-builder semantics.
