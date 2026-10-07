# PR #14 finding triage and disposition

External draft PR [BetterMoneyLabs/basis-tracker#14](https://github.com/BetterMoneyLabs/basis-tracker/pull/14)
("Security review: PoC tests and proposed basis.es patch") and the accompanying review issue
[#15](https://github.com/BetterMoneyLabs/basis-tracker/issues/15) report thirteen findings against
`b6b1573` (master). Every claim was re-verified against this codebase before anything was changed.
This document records the disposition, in the same format as `pr12_triage.md`.

The PR itself is **not merged as the fix**. It adds two PoC specs plus a full patched copy of
`basis.es` under `scala/src/test/resources/patched/`, and that copy is not a usable deliverable:

- The properties assert that an attack transaction is **accepted** (`if (patched) r shouldBe defined
  else r shouldBe None`). Merged as-is they lock in the vulnerable behaviour; they must be flipped
  into regression tests once the contracts are fixed.
- The patched contract is loaded with `scala.io.Source.fromFile("src/test/resources/patched/…")`,
  a **cwd-relative** path, so the specs only run from inside `scala/`.
- Keeping a second full copy of `basis.es` under test resources guarantees drift from `contract/`.
- `IndependentReviewSpec` instantiates `new BasisSpec` / `new BasisTokenSpec` and reaches into their
  internals (`ownerPk`, `minValue`, `emptyTreeErgoValue`, `mkTrackerTreeAndProof`, `createOut`,
  `createTx`, `trueScript`, …), so it breaks on any unrelated refactor of those suites.
- Keys are generated with `SigUtils.randBigInt`, making the tests non-deterministic.

What was taken from the PR is the **scenarios**, which were independently reproduced and then encoded
as negative regression tests against the real `contract/` files.

## Disposition table

| # | Severity | Issue | Real here? | Disposition |
|---|-----------|-------|-----------|-------------|
| C1 | Critical | Omitting context var #7 lets a creditor replay a fully redeemed note | **Yes** | **Fixed** — with a different, smaller fix than proposed |
| C2 | Critical | Two reserve inputs can be satisfied by one output; unsigned top-up lets anyone take a reserve | **Yes** | **Fixed** |
| C3 | High | A note is not bound to a reserve, tracker, chain or contract | **Yes** | **Partially fixed (off-chain)**; protocol change deferred |
| C4 | High | Reserve and tracker setup is never validated (R7 preset, poisoned R5, Int R7, R6 length-only) | **Yes** | **Partially fixed** — the deploy-time InsertOnly bug; scanner validation deferred |
| C5 | Medium | After a partial redemption the remainder of the note is stuck | **Yes** | **Fixed** |
| C6 | Medium | basis-token.es: anyone can take the reserve box's ERG | **Yes** | **Fixed** (not covered by the PR's patch) |
| S1 | Critical | `auth.mode = "none"` by default, wildcard CORS, `api_key` fails to parse | **Yes** — worse than reported | **Fixed** |
| S2 | Critical | `/reserves/submit` drains the tracker wallet; `/redemption/build` trusts `change_address` | **Yes** | **Fixed** |
| S3 | Critical | A debtor can lower `totalDebt` to 0 without the creditor | **Yes** | **Fixed** |
| S4 | High | Endpoints sign caller-chosen values; redemption state applied from the request body | **Yes** | **Partially fixed**; chain-derived state deferred |
| S5 | High | The Rust verifier accepts signatures the contract rejects | **Yes** | **Fixed** |
| S6 | High (liveness) | Tracker-built redemptions fail without an attacker (4 sub-causes) | **Yes** (all four) | **1 of 4 fixed**; AVL determinism + snapshots deferred |
| S7 | Medium | Committed secret keys, logged node api_key, third-party default node URL | **Yes** | **Fixed** — keys must still be rotated |

Deferred work is listed at the end.

---

## Contract

### C1 — replay by omitting context var #7 (Critical) — Fixed, differently

The original code treated an absent #7 as "never redeemed":

```scala
val lookupProofOpt = getVar[Coll[Byte]](7)
val storedTimestamp = if (lookupProofOpt.isDefined) { … } else { 0L }
val redeemedDebt    = if (lookupProofOpt.isDefined) { … } else { 0L }
val timestampCorrect = timestamp > storedTimestamp
```

A creditor could therefore redeem the same `(ownerSig, trackerSig, totalDebt, timestamp)` again and
again without #7. `insertOrUpdate` overwrites the existing entry, so the cumulative counter never
grew. No new signature was needed — the tracker signature covers only
`key || totalDebt || timestamp`, so it stays valid forever.

**The proposed fix was rejected.** Making #7 mandatory breaks four Rust builders
(`redemption_build.rs:875`, `basis_store/src/lib.rs:1016`/`1048`, `transaction_builder.rs:615`,
`basis_cli/src/commands/transaction.rs:478`), the Scala demo (`BasisNoteRedeemer.scala:399`, which
never sends #7 at all), ~31 Scala test cases, and two Rust tests that *assert the omission*
(`redemption_blockchain_tests.rs:778`, `:1027`). It also has a trap the issue text hides: `AvlTree.get`
returns `Option`, so keeping the trailing `.get` makes a **proof-of-absence throw**, i.e. every first
redemption would fail.

The fix applied instead keeps #7 optional but changes what omitting it means:

```scala
val nextTree: AvlTree = if (lookupProofOpt.isDefined) {
  SELF.R5[AvlTree].get.insertOrUpdate(Coll(redeemedKeyVal), insertOrUpdateProof).get
} else {
  // insert yields None (and so this .get throws) when the key is already present
  SELF.R5[AvlTree].get.insert(Coll(redeemedKeyVal), insertOrUpdateProof).get
}
```

sigma's `AvlTree.insert` returns `None` — so the `.get` throws — when the key is already in the tree.
A redemption that omits #7 for an already-redeemed note is therefore rejected, while a genuine first
redemption still succeeds. The branch is self-proving: it can only complete when the key was absent,
which is exactly what the `redeemedDebt = 0` read above assumes. Var #5's proof bytes are reused
unchanged; the AVL opcode at the leaf (`Insert` vs `Update`) is what distinguishes the two operations,
and `insert` rejects `Update` as well.

Same guarantee as the proposal, **zero off-chain changes, zero test churn, no API change**.

### C2 — two reserves satisfied by one output (Critical) — Fixed

Each reserve input selects its own output with `OUTPUTS(v % 10)`, and `selfPreserved` compares only
that output against `SELF`. Two reserve inputs with the same owner, R6 and R7, equal tokens and
matching post-action R5 trees can both be satisfied by a single output. Exploitable two ways:

- **Top-up** (action #1, no signature required): one output worth `max(V1, V2) + 0.1 ERG` satisfies
  both inputs; the attacker keeps the difference. Anyone can take the smaller reserve.
- **Redemption**: a creditor holding a note for D receives up to `D + V2`.

Fixed by requiring at most one reserve input per owner per transaction:

```scala
val uniqueReserveInput = INPUTS.filter({ (in: Box) =>
  in.propositionBytes == SELF.propositionBytes &&
  in.R4[GroupElement].isDefined && in.R4[GroupElement].get == ownerKey
}).size == 1
```

added to `selfPreserved` in both contracts. This is the accepted behaviour: an owner can no longer
top up two of their own reserves, or initiate refunds on two of them, in one transaction. Mutual
clearing (A's reserve + B's reserve) still works, because the filter keys on `R4 == SELF.R4`.

Note `uniqueReserveInput` is evaluated for every action, including #3, but #3 does not reference
`selfPreserved`, so completing a refund is unaffected.

### C5 — remainder of a partially redeemed note is stuck (Medium) — Fixed

`timestamp > storedTimestamp` meant a creditor who had redeemed part of a note could never redeem the
rest, even after the reserve was topped up. Changed to `>=`. This is only safe **after** C1:
`redeemedDebt` accumulates in the reserve tree and `properlyRedeemed` requires
`redeemed <= totalDebt - redeemedDebt`, so replaying a fully-redeemed note leaves no headroom. `>=`
also cannot lower the stored timestamp, because a note older than the stored one fails the check.

### C6 — basis-token.es does not protect the box's ERG (Medium) — Fixed

Neither action #0 (redemption) nor #1 (top-up) constrained `selfOut.value`. Action #1 requires no
signature, so anyone could add 1 token unit and take the entire ERG balance above the box minimum.
Action #0 computed the redeemed amount from the token delta only
(`redeemed = tokenAmountIn - tokenAmountOut`), leaving ERG unconstrained during a redemption too.

Added `ergPreserved = selfOut.value >= SELF.value` to both actions. The production builder already
satisfies this: `redemption_build.rs` sets `reserve_output_value = reserve_box.value` for token
reserves, so the reserve keeps its full ERG and the creditor's box is funded from fee inputs. Six
`BasisTokenSpec` cases constructed transactions where the reserve funded the creditor's output box;
those were rewritten to use a separate ERG funding input, matching what the tracker actually builds.

Also pinned the reserve-NFT amount: `tokenIdsPreserved` now checks `selfOut.tokens(0)._2 ==
SELF.tokens(0)._2`. Previously only the token *IDs* were compared, so an output could carry more units
of the reserve NFT than the input, i.e. mint additional reserve NFTs.

### C4 — reserve setup never validated (High) — Partially fixed

The scanner (`ergo_scanner.rs`) reads only R4, R6 and R7. **R5 is never read at all.** R6 is
length-checked (32 bytes) but never compared against the configured tracker NFT, which allows
**collateral fabrication**: a reserve box naming a rival tracker's NFT is still counted as collateral
for a victim issuer, because the tracker accepts every box at the configured P2S address.

**Fixed here — the deploy-time half**, which is the part that silently bricks reserves:

`scala/.../BasisDeployer.scala` and `BasisConstants.scala` built the **reserve** R5 with
`AvlTreeFlags.InsertOnly` (flags `0x01`). Both contracts call
`SELF.R5[AvlTree].get.insertOrUpdate(...)`, and sigma returns `None` (so the `.get` throws) when an
`Update` is applied to a tree with `updateAllowed == false`. A reserve deployed that way is
**redeemable exactly once per (owner, receiver) pair**. No test caught it because `BasisSpec:32` uses
`basisReserveFlags` with `updateAllowed = true`, so the deployed shape was never exercised. Changed to
insert + update for the reserve tree in `BasisDeployer.scala`, `BasisConstants.scala`,
`demo/src/BasisDeployer.scala` and `BasisNoteRedeemer.generateReserveInsertProof`.

The **tracker** tree is a different tree and correctly stays `InsertOnly` — it only ever inserts
fresh `(owner, receiver) -> totalDebt` pairs. `TestVectorGenerator` and `TrackerBoxSetup` were left
alone for that reason.

**Deferred**: first-seen validation of R4 (curve point), R5 (empty-tree constant `0x03`/32/0), R6
(actual NFT equality), R7 (Long type, sane range), excluding pending-refund reserves from
collateralization totals, and minting the tracker NFT with supply 1.

### C3 — note not bound to a reserve, tracker, chain or contract (High) — Partially fixed

The signed message is `blake2b256(owner || receiver) || totalDebt || timestamp`, while cumulative
redeemed debt is tracked **per reserve box** (each reserve has its own R5). Nothing connects the two,
so the same note redeems in full from *every* reserve the owner has under the same tracker. The same
signature is also valid for both contracts and on both testnet and mainnet.

**The proposed fix does not address the main damage.** Adding a domain tag and the tracker NFT id to
the message would stop cross-contract and cross-chain replay, but per-box redemption state is the
actual problem, and a message-format change cannot fix it — it would require changing the signed
message across the contract, both Rust signers, the CLI and the Scala reference, before any note
exists.

What was implemented is the off-chain mitigation: `ReserveTracker::resolve_issuer_reserve` fails
closed when an owner has more than one tracked reserve, and the collateralization endpoint returns
`409` instead of a confident-looking ratio computed from `matching_reserves.first()`. Summing the
collateral would have been worse still — it would report a healthy ratio for collateral that cannot
back the same debt twice.

**This is tracker policy, not a contract invariant.** It holds only while this tracker is the one
redeeming. The underlying per-box redemption state remains a protocol limitation and must be recorded
as such before mainnet.

---

## Tracker server

### S1 — no authentication by default (Critical) — Fixed, worse than reported

Four distinct problems:

1. `AuthMode` used `#[serde(rename_all = "lowercase")]`, giving the wire name **`apikey`**, while
   every consumer in this repository uses `api_key` (`config/basis.toml.example:16`,
   `docs/CONFIGURATION.md:39`, `crates/basis_cli/src/api.rs:54`, `crates/basis_mcp/src/server.rs:245`).
   No test deserialized `AuthMode` from a string, which is why this survived.
2. `main.rs` caught `AppConfig::load()` failure, **called `load()` a second time**, and then fell back
   to a hardcoded struct whose `auth` was `AuthConfig::default()` — `AuthMode::None`, no API key — on
   `0.0.0.0:3048`. So writing the documented `mode = "api_key"` did not produce a parse error the
   operator could see; it produced a silently anonymous **Admin-everything** server.
3. `none` mode grants `ClientRole::Admin` to every request, and CORS fell through to
   `allow_origin(Any)` — including when auth was enabled but `allowed_origins` was empty, so any web
   page open on the operator's machine could drive the API.
4. Signature auth: the replay cache was written **before** the signature was verified (and before the
   body was even read), the body was read with an explicit `usize::MAX`, and the cache key used the
   **raw** `X-Signature-Timestamp` string while the signature covered the **parsed** value. Since
   `u64` parsing accepts leading zeros, replaying a captured request with `0<ts>` produced a fresh
   cache key and the same signature verified again.

Fixed: `snake_case` with an `apikey` alias; a failed config load now logs and **exits**;
`validate_startup()` refuses `auth.mode = "none"` on a non-loopback bind unless
`server.auth.allow_anonymous_non_loopback` is explicitly set; no wildcard CORS when auth is on
(with auth on and no origins configured, **no** CORS layer is installed at all, so browsers refuse
cross-origin calls while curl/CLI/MCP are unaffected); `DefaultBodyLimit::max(8 MiB)` on the router;
the replay cache is written only after `schnorr_verify` succeeds and is keyed on the parsed timestamp.

### S2 — tracker wallet can be drained (Critical) — Fixed

`/reserves/submit` forwarded the caller's `{address, value, assets, registers}` to the node's
`/wallet/payment/send` together with the node `api_key`, so the node signed it with the **tracker's
wallet key**. With S1's defaults, anyone who could reach the port could pay any address from that
wallet — which also holds the tracker NFT box.

The endpoint is consumed by `crates/basis_cli/src/api.rs`, three demo flows and five documents, so it
was **not deleted**. Instead `/reserves/create` now fingerprints the exact payload it returns and
records a single-use, 30-minute permit; `/reserves/submit` refuses any payload whose fingerprint does
not match a live unused permit, and consumes it on use. The permit check runs *before* the node is
contacted.

`/redemption/build` took `change_address` unchecked while selecting fee inputs from the **tracker's**
wallet and signing them with the tracker key, so repeating a 1-nanoERG redemption against a tiny
self-owned reserve drained the wallet one box minus the fee at a time. The field is still accepted on
the wire for compatibility but is now **ignored** (with a warning); change always returns to the
configured tracker address.

### S3 — debt can decrease without the creditor (Critical) — Fixed

`add_note` checked only that the timestamp was newer and not in the future, and that the debtor's
signature was valid — never that `amount_collected` was at least the stored value. A debtor could
post `totalDebt = 0`, after which every note for that pair fails `trackerDebtCorrect` **permanently,
including the emergency path** (both compare the note's totalDebt against the committed value). The
debtor could then serve the 2-month refund waiting period and withdraw the collateral. The documented
novation flow has the same shape.

Fixed: `add_note` and `update_note` both reject a decrease with
`NoteError::DebtDecreaseNotPermitted { previous, requested }`. A note for an existing pair is an
increase of the *same* cumulative debt, so `amount_redeemed` is now carried forward from the previous
note instead of being reset to `0` (it was hardcoded in both `IouNote::create_and_sign` and
`POST /notes`, which destroyed the outstanding-debt / FIFO accounting).

**The documented novation flow must not be used until a creditor co-signature endpoint exists.** Debt
transfer to a *different* creditor is unaffected, since that is a different key in the tree.

### S4 — endpoints sign or store caller-chosen values (High) — Partially fixed

`/tracker/signature` signed any `(key, totalDebt, timestamp)` with no state lookup and no policy
check. Since the tracker signature is the only gate for `enforce_acceptance_policy` and the FIFO
rule, a caller could obtain a signature over any debt figure and assemble the transaction themselves
from `/reserve/proof` + `/tracker/proof`, bypassing both. `/redemption/prepare` did look up the note
but fell back to `payload.amount` when no note existed.

Fixed: both now resolve the **stored** note and sign only `note.amount_collected` /
`note.timestamp`. A request that disagrees with stored state is rejected with `409` rather than
silently overwritten, and an unknown pair is `404` — the tracker no longer signs debt that does not
exist in its committed tree.

`complete_redemption` additionally: no longer resets `note.timestamp` (the timestamp is part of the
signed message, so refreshing it invalidated the stored issuer signature and left the note
unredeemable), bounds `redeemed_amount` by the note's outstanding debt, and uses `checked_add` rather
than `+=` (which panicked in debug and wrapped in release). Because a redemption is not a new signed
note, it now goes through a new `TrackerStateManager::record_redemption` instead of `update_note` —
`update_note` legitimately demands a strictly newer timestamp and a valid signature, neither of which
applies here.

**Deferred**: deriving redemption state from confirmed on-chain transactions instead of the request
body. This is the same redesign already deferred in `pr12_triage.md` finding 3.

### S5 — verifier accepts signatures the contract rejects (High) — Fixed

The contract reads `z` and the challenge `e` with ErgoScript's `byteArrayToBigInt`, a **signed**
big-endian conversion: a value with the top bit set becomes negative, and `g.exp(negative)` throws.
The signer (`impls.rs:332`, `:347`) retries nonces until both are below 2^255, but
`SchnorrVerifier::verify_signature` did not enforce either rule, and all five verifier entry points
delegate to it.

Consequence: a debtor signing without the retry rule (~25% of signatures) produced notes the tracker
accepted and pinned, while the node rejected the resulting transaction on-chain. The creditor could
never redeem, and the tracker's tree was pinned to that note.

Fixed with a shared predicate `is_contract_compatible_be32`, applied to `z` in
`validate_signature_format` and to the recomputed challenge in `verify_signature`. Tests cover both
rejections and assert that 25 consecutive signatures from our own signer satisfy both rules.

**The same hole existed in the Scala reference signer, and the test fixtures were proof.**
`SigUtils.sign` only retried on `z.bitLength > 255` and never looked at the challenge, so roughly half
the signatures it produced were unusable on-chain. The existing cross-validation vectors were supposed
to catch exactly this class of problem, and instead two of them were themselves invalid: **TV003**
("Valid tracker signature", `should_verify: true`) and **TV009** ("Maximum u64 values") both have
challenges whose first byte is `0xf7` and `0xc1`. TV005 and TV008 have the same defect but were already
negative vectors. All four are now `should_verify: false` and kept as regression coverage, and
`SigUtils.sign` enforces the challenge rule. `specs/SCHNORR_SIGNATURE_SPEC.md` and `AGENTS.md` are
updated.

Lesson worth recording: a hardcoded vector asserted `should_verify: true` and the verifier honoured it,
so a vector can encode a bug as "expected". The vectors were only re-examined because the verifier fix
made them fail.

### S6 — tracker-built redemptions fail without an attacker (High, liveness) — 1 of 4 fixed

| Sub-cause | Status |
|---|---|
| Emergency redemptions omit context var #6, so `getVar[Coll[Byte]](6).get` throws | **Fixed** |
| `avl_tree.rs` rebuilds a prover from a `HashMap` (random order); AVL+ shape depends on insertion order | Deferred |
| Restart rebuild sorts only by timestamp, with no tiebreaker | Deferred |
| Proofs served from the live in-memory tree instead of the committed on-chain digest | Deferred |
| A dropped update transaction (node 404) is treated as pending forever | Deferred |
| No reorg handling past depth 2 | Deferred (already documented in `docs/CONFIGURATION.md`) |

The emergency path omitted var #6 entirely, but the contract reads it unconditionally with `.get`, so
the contract's own `enoughTimeSpent` fallback was unreachable and **every** emergency redemption
failed. Now an empty `Coll[Byte]` is always sent, which is what the contract branches on
(`trackerSigBytes.size > 0`). The Scala reference and the CLI already did this.

The AVL determinism work is a single workstream: the reserve side *is* digest-checked at build time
and fails closed, but the tracker side is not, so a local/on-chain tree divergence surfaces as an
unexplained invalid proof. Durable insertion order is the prerequisite for serving proofs from a
committed snapshot.

### S7 — secrets and defaults (Medium) — Fixed; keys must still be rotated

- `secrets/participants.csv` (repo root) was tracked and held **live mainnet-format keys** with
  `secret_hex` values, under a header stating it "should NOT be committed". Untracked and gitignored;
  replaced by `secrets/participants.csv.template`. Zero overlap with `scala/secrets/participants.csv`,
  which is the committed **test fixture** and stays.
- `demo/agent_celaut_use/config/basis.toml:11` held a real `tracker_secret_key`. Untracked and
  gitignored. `run.sh` generates the file at run time anyway, and its hardcoded demo keypairs are now
  generated per run by `demo/demo_keygen.py` (verified against the curve: on-curve, scalar
  multiplication and compression check out). The three sibling demos already injected from
  `$TRACKER_SECRET`.
- The node `api_key` was logged in plaintext at INFO on **every** request (`ergo_scanner.rs:160`, `:179`).
  Removed; the key's presence is still reported.
- `ergo.node.node_url` defaulted to `http://159.89.116.15:11088` — a third-party mainnet node over
  plaintext HTTP, so an operator who did not set it sent the api_key to somebody else's machine
  unencrypted. Now defaults to `http://127.0.0.1:9053`, matching `config/basis.toml.example`.

**These keys must be treated as public and rotated before any deployment.** Removing them from the
repository does not unpublish them.

---

## Regression tests added

The PoC specs from PR #14 are **not** merged: they assert that attacks are *accepted*, load the patched
contract from a cwd-relative path, and duplicate `basis.es` where it will drift. The scenarios were
re-encoded as negative tests against the real `contract/` files.

| Spec | Properties | Covers |
|---|---|---|
| `BasisSecuritySpec` | 7 | C1 replay (attack + honest first-redemption control), C2 top-up merge, C2 redemption merge, C2 per-transaction-scope control, C5 remainder, C5 older-timestamp control |
| `BasisTokenSecuritySpec` | 4 | C6 top-up ERG drain, C6 honest-top-up control, C6 redemption ERG drain, C6 reserve-NFT inflation |

Each attack property was verified to **fail against the pre-fix contract** by temporarily reverting
`contract/basis.es` and `contract/basis-token.es`: reverting `basis.es` fails C1, both C2 properties and
C5; reverting `basis-token.es` fails all three C6 attacks.

One PoC scenario was deliberately **not** ported. The PR included a "mutual clearing" case with two
reserves and two tracker data inputs in one transaction; the contract reads only
`CONTEXT.dataInputs(0)`, so such a transaction can never satisfy both reserves regardless of the fixes.
It was replaced with a control that verifies `uniqueReserveInput` is scoped per transaction rather than
per owner, which is the property that actually matters.

## Deferred

| Item | Why |
|---|---|
| C3 protocol change (domain tag + tracker NFT id in the signed message) | Breaking change across the contract, both Rust signers, the CLI and the Scala reference. The off-chain single-reserve rule holds meanwhile. |
| C4 first-seen validation (R4/R5/R6/R7), pending-refund collateral exclusion, tracker NFT supply 1 | Needs a policy decision on how a reserve failing validation is surfaced (reject vs mark) and interacts with reserve generations. |
| S4 chain-derived redemption state | Same redesign deferred in `pr12_triage.md` finding 3. |
| S6 AVL insertion-order durability, proofs from a committed snapshot | Prerequisite for each other; same family as `pr12_triage.md` finding 3. |
| S6 updater stall recovery and reorg handling | 404-on-update currently wedges the updater indefinitely; `RevertPendingNotes` exists but is never called from production code. |

## Deployment impact

`contract/basis.es` and `contract/basis-token.es` both changed, so **the compiled scripts differ and
the reserve contract P2S address changes**. The scanner discovers reserves by that configured address
(`ergo.basis_reserve_contract_p2s` / `ergo.basis_token_reserve_contract_p2s`) — it does not derive it
from the script — so:

1. Recompute and set both P2S addresses before starting a tracker against a fresh deployment.
2. Reserves already created under the old script keep their old address and will **silently stop being
   tracked** once the config is updated. Migrate them, or run both addresses, before switching.
3. `AuthMode` gained `server.auth.allow_anonymous_non_loopback`; existing configs are unaffected
   because it defaults to `false`, but any deployment relying on `auth.mode = "none"` on a
   non-loopback address will now refuse to start until authentication is configured or the flag is set.
4. `ReserveCreationResponse` gained `submission_permit`. Clients must echo it back on submit; the CLI
   does so automatically.
