# Redemption-Time Acceptance Policy Check

## Overview

Acceptance policies (see `specs/acceptance_predicates.md`) were originally evaluated
only when a note is *accepted* (`POST /acceptance/check`). However, a redemption can
harm *other* debt holders of the same reserve after they accepted their notes:
paying out amount `A` moves the issuer's collateralization ratio from `C/D` to
`(C-A)/(D-A)`, which is strictly worse when the reserve is undercollateralized
(`C < D`).

The tracker now verifies, at redemption time, that a redemption does not **newly
violate** the acceptance policy of any other debt holder of the same reserve
(issuer). The check is implemented in
`crates/basis_server/src/acceptance/redemption_check.rs`
(`check_redemption_policy_compliance`) and is enforced by both redemption flows:

- Legacy `POST /redeem` (`crates/basis_server/src/api.rs`, `initiate_redemption`):
  after the reserve lookup, before the tracker signature is issued.
- Split flow `POST /redemption/build` (`crates/basis_server/src/redemption_build.rs`,
  `build_redemption_inner`): after a reserve with sufficient collateral is selected,
  before the unsigned transaction is built.

## Simulated ratio math

Let:

- `C` = reserve box value (nanoERG) backing the issuer's debt,
- `D` = total outstanding debt over all of the issuer's notes, where per-note
  outstanding debt is `amount_collected - amount_redeemed`,
- `A` = redemption amount.

The post-redemption state is simulated as:

- `C' = C - A - fee`
- `D' = D - A`

In both current transaction flows the miner fee is paid by explicit fee inputs and
is **not** deducted from the reserve output, so both call sites pass `fee = 0`.
The `fee` parameter remains in the helper signature for flows that might deduct it
from the reserve.

Ratio semantics follow `CollateralizationPredicate`
(`crates/basis_server/src/acceptance/mod.rs`): the ratio is a **fraction**
(`collateral / debt`, e.g. `1.0` = 100%), compared against `min_ratio` with `>=`.
Zero debt (`D = 0` or `D' = 0`) means fully collateralized — no violation is
possible.

## Which predicates are evaluated

Only `Collateralization { min_ratio }` leaves can be *newly* violated by someone
else's redemption. All other leaf kinds (`Whitelist` including `max_debt`,
`Blacklist`, `NoPendingRefund`) are unaffected by another holder's redemption, so
they are evaluated once against their current values (issuer key, holder's
cumulative debt, reserve refund-pending flag). Composite predicates keep their
`AllOf` / `AnyOf` / `Not` semantics; the whole tree is evaluated twice, once with
`(C, D)` and once with `(C', D')`.

For each other holder (recipient with outstanding debt, excluding the redeemer),
the effective policy is resolved with the same precedence as `check_acceptance`:

1. Per-recipient stored policy (`AcceptancePolicyStorage`), parsed from JSON.
   Empty or corrupted stored policies reject by default (mirroring
   `check_acceptance`).
2. The server's global acceptance configuration (`[acceptance]` in `basis.toml`).
3. The config default (`acceptance.default`).

## Decision rules

For each other holder, the policy result is classified as:

- **satisfied**: passes both pre (`C/D`) and post (`C'/D'`),
- **already violated**: fails pre,
- **newly violated**: passes pre but fails post.

Then:

1. If any holder is **newly violated** → reject with
   `RedemptionPolicyError::WouldViolatePolicy` → HTTP 400, failure id
   `failed_policy_violation`.
2. Else if **all** other holders are already violated (distressed reserve —
   blocking everyone would deadlock redemptions forever), the **FIFO fallback**
   applies: only the holder of the issuer's *oldest outstanding note* (minimum
   `timestamp` among notes with outstanding > 0) may redeem. Any other redeemer is
   rejected with `RedemptionPolicyError::NotOldestNote` → HTTP 400, failure id
   `failed_not_oldest_note`. The error message includes the oldest note's
   timestamp, so the client knows the queue position.
3. Otherwise (no other holders, all satisfied, or a mixed case with some already
   violated but none newly violated) → allowed.

## Configuration

```toml
[redemption]
enforce_acceptance_policy = true
```

- Default: `true`.
- Parsed into `RedemptionConfig` (`crates/basis_server/src/config.rs`), accessible
  via `AppState.config.redemption`; no separate `AppState` field is needed.
- When `false`, redemption requests skip the policy check entirely (previous
  behavior).

## Error ids

Both flows return HTTP 400 with an error message prefixed by the failure id:

- `failed_policy_violation: redemption would newly violate acceptance policy of
  holder <hex>: collateralization ratio would drop from ... to ...`
- `failed_not_oldest_note: reserve is distressed; only the oldest outstanding note
  (timestamp <ts>) may redeem, requested note has timestamp <ts>`

## Emergency redemption limitation

Emergency redemption (the contract path available after the refund time lock,
`contract/basis.es`) **bypasses the tracker**: it requires no tracker signature, so
the tracker cannot enforce any ordering or policy on it. Requests with
`emergency = true` therefore skip this check in both flows. The policy check only
governs tracker-assisted (normal) redemptions, matching the contract's design where
the tracker is the ordering arbiter for normal redemptions and first-come-first-served
applies on-chain otherwise.

## Tests

- Unit tests in `crates/basis_server/src/acceptance/redemption_check.rs` cover:
  well-collateralized pass; newly-violated holder → `WouldViolatePolicy`;
  all-holders-violated FIFO fallback (non-oldest rejected with `NotOldestNote`
  carrying both timestamps, oldest allowed, timestamp ties, fully-redeemed notes
  skipped in the queue, redeemer without an outstanding note); mixed case
  allowed; no other holders allowed; `D' = 0` edge; zero-debt collateralization
  leaf; `min_ratio` boundary (equal passes, just-below fails); stored-policy
  precedence in both directions (stricter and looser than global);
  corrupted/empty stored policy counts as violated; `AllOf`/`Not` composite
  semantics; `NoPendingRefund` with a pending refund.
- Handler-level tests in
  `crates/basis_server/tests/redemption_api_integration_tests.rs`:
  `test_redeem_rejected_when_would_violate_holder_policy` (400
  `failed_policy_violation`), `test_redeem_rejected_when_not_oldest_note` (400
  `failed_not_oldest_note`, message contains the oldest timestamp),
  `test_redeem_skips_policy_check_when_enforcement_disabled` (`[redemption]
  enforce_acceptance_policy = false` skips the check), and
  `test_build_redemption_rejected_when_would_violate_holder_policy` /
  `test_build_redemption_rejected_when_not_oldest_note` — the same two
  rejections through `POST /redemption/build` with a mock Ergo node serving the
  reserve box (R5 digest matching the tracker's reserve tree).
- Config tests in `crates/basis_server/src/config.rs`:
  `enforce_acceptance_policy` defaults to `true` and deserializes both ways.
