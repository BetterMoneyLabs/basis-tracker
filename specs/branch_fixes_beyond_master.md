# Fixes on `pr21-merge-master` not yet landed in `master`

Master is at `6a6ebfe` ("close #20: exchange atomicity"). The branch
`pr21-merge-master` adds 21 non-merge commits (through `2a5284f`) plus the
uncommitted N18/N19 work listed below. Finding IDs refer to the follow-up
security review of issue #15
([comment](https://github.com/BetterMoneyLabs/basis-tracker/issues/15#issuecomment-6502635356));
N8 is **not** listed because its fix (reserve admission control, issue #19)
already landed on master in `4bde3d9`.

## Blockers

- **N1 — `/redemption/submit` accepts an unrelated transaction** (`933684f`).
  Any Write client could desync any issuer's reserve tree by submitting a
  top-up-shaped tx against a redemption build. The build now records the
  unsigned tx id and submit requires `signed_tx.id` to equal it.
- **N2 — committed-tree rebuild is wrong** (`678ddbe`). `/redemption/build`
  failed tracker-wide after a repeat payment or a backdated note. Committed
  tracker proofs now survive repeat payments and updater races.
- **N3 — `/redeem` panics the tracker thread** (`ec70d0a`). One request could
  kill the tracker loop; the loop now stays alive on panic. Same commit: VLQ
  `Coll[Byte]` length encoding fix.
- **N4 — scanner stops permanently once a page exceeds 2 MiB** (`9c2266a`).
  A full `byAddress` page of maximum-size boxes is now guaranteed to fit the
  response body cap.

## High

- **N5 — tracker secret key logged at INFO** (`b8bdc47`). The updater no
  longer logs the key on tracker updates.
- **N6 — clients sign/submit whatever the tracker returns** (`754577e`,
  `e879051`, `b84cd61`, `232af0b`). Clients now verify the tracker's payload:
  fee and change-address checks on reserve payloads, only plain P2PK fee
  inputs, never signing context var #2 over tracker totals, and the pending
  redemption binds the context extension.
- **N7 — admin key split is opt-in; clients break when it's on** (`a3bb957`,
  `c737d84`, `4bf3f15`). The `/redeem` endpoint and the server-sign client
  paths are removed; reserve payloads and signed policy uploads are Write
  routes; startup refuses weak/example admin keys.
- **N9 — reserve rotation pays old notes again** (`aea7589`). A rotated
  reserve's root carries the previous tree's redemption history forward via a
  `pending_root_r5` record, so old redemption vars cannot be replayed.
- **N10 — exchange signatures: standalone note, held exchanges erase
  payments** (`43cc30a`). Exchange messages are domain-separated and carry a
  validity window.
- **N11 — previous token script reads ERG debt as token debt; still shipped**
  (`5fef2be`, `4bf3f15`). The exploitable previous token reserve script is no
  longer shipped, and startup refuses stale reserve contract addresses.
- **N12 — shipped clients can't redeem as a creditor** (`754577e`,
  `e879051`). The client redemption path was fixed so creditors redeem with
  the note signature (covered by the N6 client-hardening commits).
- **N13 — build depends on an unpublished local sigma-rust** (`d843c8f`).
  The `[patch.crates-io]` entries pin a public sigma-rust git revision.

## Medium

- **N14 — reserve-tree mirror never re-synced from chain** (`9e83719`).
  `/redemption/build` repairs a diverged reserve mirror from on-chain lineage
  instead of trusting API-event-driven state.
- **N15 — vendored `ergo_avltree_rust` turns `insert` into an upsert**
  (`ab75d2d`). The fork and its patch entry are removed; the workspace uses
  crates.io 0.1.1, restoring JVM-compatible insert-fails-on-existing-key
  semantics that the C1 contract fix relies on.
- **N16 — pending-refund reserves count as full collateral** (`2a5284f`).
  `NoPendingRefund` was added to the default acceptance policy, so a reserve
  with an initiated refund no longer backs redemptions at full value.
- **N17 — `/redemption/build` can be abused by any Write client** (`ab2b76c`).
  Redeemer-funded fees plus abuse-resistant build limits.
- **N18 — clients ignore the denomination** (`e5b1f1d`). Acceptance
  checks are evaluated per denomination, the TUI wallet is overhauled for
  per-denomination operation, and redemption build is guarded against a
  diverged committed state. Plan: `specs/n18_client_denomination_plan.md`.
- **N19 — a fresh database rejects every reserve with history**
  (`e5b1f1d`). History-aware reserve admission: the scanner walks
  creating transactions back to an authenticated root (empty R5, R7 = 0,
  owner-P2PK deployment input), admits verified successors of a spent tracked
  box, retries (never quarantines) on inconclusive walks, and clears pre-N19
  quarantine records at startup. Bound: `ergo.scanner.admission_walk_depth`
  (default 512). Plan: `specs/n19_history_aware_admission_plan.md`; design:
  `specs/ergo/reserve_admission.md`.

## Tests

- `0899410` adds `#[ignore]`d regression tests for the findings that were
  still open when PR #21 was drafted (N2, N11, N15); the fixes above
  activate the covered behavior.

## Status note

All fixes are committed on the branch: N1–N17 in the individual commits
listed above, N18 and N19 together in `e5b1f1d`. Master remains at `6a6ebfe`.

A post-implementation review pass (2026-11-23) refined the N19 code without
changing behavior: the history walk now fetches each generation's creating
transaction once (previously twice — the loop re-fetched the parent's
transaction after following it), `decide_admission` was reduced to the single
`proof` parameter its call sites actually vary, and a storage-level unit test
covers `migrate_quarantine_v2` idempotency and marker handling alongside the
scanner-level end-to-end migration test.

A CI remediation pass (2026-11-23, after N13 made CI reach compilation for
the first time since `bb0cfbd`) made the workspace build, test, clippy and
fmt steps green under the workflow's `-D warnings`: the `basis_cli` binary no
longer re-compiles the library modules (`src/main.rs` is a thin shell over
`basis_cli_lib`, eliminating the bin-only dead-code errors), unused test
constants/helpers and pattern bindings were removed or ignored, the needless
borrows in `persistence.rs` were dropped with a `ReserveAvlUpdate` type alias
for the journal tuple, and two targeted `#[allow]`s document the intentional
exceptions (`TrackerCommand` variant size, `create_reserve` arity).
