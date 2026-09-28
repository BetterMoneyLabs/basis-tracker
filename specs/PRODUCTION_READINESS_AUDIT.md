# Basis Tracker - Production Readiness Audit

**Original Audit Date:** 2026-02-28
**Last Reviewed:** 2026-09-26
**Auditor:** Automated Code Review
**Status:** 🔴 **NOT PRODUCTION READY** — CI is non-functional and there are unauthenticated remote panics plus an unverified trust path for redemption completion.

> **Changelog note.** The previous revision of this document reported "✅ PRODUCTION READY" with
> "All critical placeholders resolved". That status was **incorrect** and has been retracted. The
> original audit was a grep-for-`TODO` exercise: it counted placeholder strings but never
> exercised the request handlers, so it missed the unauthenticated panics, the non-functional CI,
> and the fact that redemption completion trusts the request body. Issue #3 below was also stale
> (it was fixed in code but still listed as open). All findings in this revision were verified
> against the source at the commit below.

**Verified against:** `585dc03` ("why credit")

---

## Executive Summary

The core Basis protocol is implemented and the on-chain flow has been exercised on real Ergo
mainnet — including multi-redemption, tracker-server restart recovery, and a token-backed reserve
variant (four harnesses in `tests/`). However, the codebase is **not safe to expose to a network**.

Three findings dominate:

1. **CI cannot build the workspace.** `Cargo.toml` patches three crates to absolute local paths
   under `/home/kushti/`. Every push has failed at the first build step, so tests, clippy,
   `fmt --check`, the OpenAPI consistency test, and the Scala contract tests have not run in
   weeks. No change to this repository is currently verified by anything.
2. **Two request handlers panic on attacker-controlled input**, and the default auth mode grants
   `Admin` to every anonymous caller. A single malformed request kills the server.
3. **Redemption completion is unauthenticated in practice and never verified against the chain.**
   The server credits redeemed amounts from the request body at mempool-acceptance time, with no
   outstanding-debt cap, no idempotency, and no confirmation-depth gate.

### Risk Assessment

| Risk Level | Count | Description |
|------------|-------|-------------|
| 🔴 **CRITICAL** | 6 | CI broken; 2 remote panics; unverified redemption completion; no idempotency; collateralization no-op; plaintext tracker key |
| 🟡 **HIGH** | 9 | Default-open auth, missing signature verification, fail-open policies, unchecked debt arithmetic, fake demo events, AVL error suppression, vacuous proof verifiers, unbounded request bodies, no rate limiting |
| 🟢 **MEDIUM** | 14 | Startup panics, silent state divergence, dead event pipeline, stubbed cross-verification, CLI panics, hardcoded network, placeholder R5 fallback, no CI regression guard for multi-redemption, mainnet coverage limited to the legacy flow |
| 🔵 **LOW** | 6 | Test-only placeholders, unreferenced stubs, duplicated crypto implementations |

**Prior audit tallies for reference:** 48 placeholders, 13 panic/unimplemented. Those numbers
undercount the real risk because they weight a comment in a test file the same as a panic in a
request handler.

---

## Critical Issues (Must Fix Before Production)

### 1. Workspace Is Not Buildable Outside One Machine — CI Is Fully Red 🔴

**File:** `Cargo.toml:20-30`

```toml
[patch.crates-io]
ergo-lib = { path = "/home/kushti/ergo/sigma-rust/ergo-lib" }
ergotree-ir = { path = "/home/kushti/ergo/sigma-rust/ergotree-ir" }
ergotree-interpreter = { path = "/home/kushti/ergo/sigma-rust/ergotree-interpreter" }
```

**Evidence:** CI run `32781403619` (master push, 2026-08-24) fails in the first step:

```
error: failed to load source for dependency `ergo-lib`
  failed to read `/home/kushti/ergo/sigma-rust/ergo-lib/Cargo.toml`
  No such file or directory (os error 2)
```

The last three pushes (`32781403619`, `32487010584`, and the `basis-inspector-spec` PR) all
failed. `.github/workflows/test.yml` has 12 steps; **none of them have executed.**

**Impact:** Total loss of verification. Every "✅ tested" claim in this document, and in
`specs/SCHNORR_SIGNATURE_SPEC.md`, is currently unfalsifiable. A breaking change can and has
landed on master unnoticed.

**Note on the vendored fork:** `ergo_avltree_rust` is patched to `temp/vendors/ergo_avltree_rust`,
which is committed and therefore *does* travel with the repo. That patch is genuinely required —
see Issue H4. Only the three `sigma-rust` paths are machine-local.

**Fix Required:**
1. Vendor `ergo-lib`, `ergotree-ir`, and `ergotree-interpreter` into `temp/vendors/` (matching the
   existing `ergo_avltree_rust` pattern), or point the patches at a git ref/tag of a fork.
2. Confirm `cargo build --workspace` succeeds from a clean checkout on a machine without
   `/home/kushti/ergo/sigma-rust`.
3. Re-run CI and confirm the OpenAPI consistency test and Scala contract tests actually pass.

**Priority:** 🔴 CRITICAL — blocks verification of every other fix.

---

### 2. Panic in `POST /redemption/prepare` — Slice Before Length Check 🔴

**Status:** ✅ **FIXED** (2026-09-27) — see "Fix Applied" below.

> **Severity correction (2026-09-27).** An earlier revision of this audit described the two panic
> issues (#2, #3) as "unauthenticated remote crash" that "kills the server". **That was
> overstated.** Both handlers carry `#[axum::debug_handler]` (`api.rs:2947`, `:1384`) and the
> workspace sets no `[profile]` overrides, so `panic = "unwind"` is in effect. The panics are
> therefore caught and surface as **500 responses with an ERROR-level log**, not process aborts.
> They remain genuine bugs worth fixing — unauthenticated 500s, wasted work, log noise, and
> inconsistent state for anything mutated before the panic — but they are availability
> *degradation*, not a DoS kill switch. Re-rated accordingly; see the Risk Assessment.

**File:** `crates/basis_server/src/api.rs:2989-2994` (route registered at `main.rs:820`)

```rust
let redemption_id = format!(
    "redemption_{}_{}_{}",
    &payload.issuer_pubkey[..8],      // <-- slice
    &payload.recipient_pubkey[..8],  // <-- slice
    payload.timestamp
);
```

The only prior validation (`api.rs:2955-2957`) checks **decodability**, not length:

```rust
if hex::decode(&payload.issuer_pubkey).is_err()
    || hex::decode(&payload.recipient_pubkey).is_err()
```

The `bytes.len() == 33` checks are at `api.rs:2997-3019` — *after* the slice.

**Reproduction:**
```
POST /redemption/prepare
{"issuer_pubkey":"aa","recipient_pubkey":"aa","timestamp":0}
```
`hex::decode("aa")` succeeds, then `&"aa"[..8]` panics: `byte index 8 is out of bounds`.

**Impact:** Unauthenticated 500 from `POST /redemption/prepare`. The route requires only
`ClientRole::Write` (`authorization.rs:64`), which `AuthMode::None` grants to everyone (see
Issue H1). Previously assessed as a remote crash; corrected above.

**Fix Applied (2026-09-27):**
1. ✅ Replaced the decodability-only check with a combined decode **and** 33-byte length
   validation for both keys, performed up front before anything indexes the input strings.
2. ✅ Removed the fixed-offset slicing entirely. The redemption-ID prefix is now derived from the
   already-validated bytes via `hex::encode(&bytes[..4])`, which is byte-identical to the first
   8 hex characters of a well-formed 66-character pubkey — so the ID format is unchanged for
   valid input, but the panic class is eliminated rather than merely made safe.
3. ✅ Removed the now-duplicated decode blocks further down the handler.
4. ✅ Distinct error messages preserved (`Invalid hex encoding for public keys`,
   `issuer_pubkey must be 33 bytes hex-encoded`, `recipient_pubkey must be 33 bytes hex-encoded`)
   so existing client-visible behaviour is unchanged.

**Regression tests added** (`crates/basis_server/tests/redemption_api_integration_tests.rs`):
- `test_prepare_redemption_short_hex_does_not_panic` — `"aa"` / `"aa"`
- `test_prepare_redemption_short_recipient_does_not_panic` — valid issuer + `"aa"` recipient,
  asserting the recipient-specific message
- `test_prepare_redemption_empty_pubkey_does_not_panic` — empty strings

All three were confirmed to **fail against the unfixed handler** (returning 500) and pass after
the fix. Note that the recipient test deliberately uses `"aa"` rather than `"0"`: an odd-length
string such as `"0"` is rejected by the *old* decodability check before reaching the slice, so it
would pass without ever exercising the panic.

**Verification:** `cargo test -p basis_server` — 217 passed, 0 failed. `cargo fmt` reports no
diff in either changed file (the only diff is pre-existing, in `redemption_build.rs:477`).

**Priority:** ✅ RESOLVED

---

### 3. Panic in `GET /events/paginated` — Unbounded Slice 🔴

> **Severity correction (2026-09-27).** Same correction as Issue #2: `get_events_paginated` is
> `#[axum::debug_handler]` (`api.rs:1384`) and `panic = "unwind"` applies, so this yields a 500,
> not a process abort. Unauthenticated and trivially triggerable, but not a crash. Still 🔴
> because it is unauthenticated input reaching an out-of-bounds slice, and because
> `?page_size=999999999` forces a large allocation on every request.

**File:** `crates/basis_server/src/store.rs:44-46`

```rust
let start = page * page_size;
let end = std::cmp::min(start + page_size, events.len());
Ok(events[start..end].to_vec())
```

`page` and `page_size` are parsed with no upper bound and no validation
(`crates/basis_server/src/api.rs:1392-1396`):

```rust
let page = params.get("page").and_then(|p| p.parse().ok()).unwrap_or(0);
let page_size = params.get("page_size").and_then(|ps| ps.parse().ok()).unwrap_or(20);
```

**Failure modes:**
- `?page=1` with fewer than 20 events → `start=1`, `end=min(21, len)=len`. Panics whenever
  `len <= 1`; when `len == 0`, `events[1..0]` panics immediately. **A fresh server panics on
  the very first such request.**
- `?page=18446744073709551615&page_size=100` → `page * page_size` overflows (panic in debug,
  wrap in release).
- `?page_size=18446744073709551615` → `start + page_size` overflows → `end < start` → slice panic.
- `?page_size=999999999` → unbounded `Vec` allocation per request.

**Impact:** Unauthenticated 500 from `GET /events/paginated`, plus unbounded allocation via a
large `page_size`.

**Fix:** Clamp `page_size` to a sane maximum (e.g. 200), compute `start` with
`saturating_mul`, and return an empty `Vec` when `start >= events.len()`. Add regression tests
for `?page=1` against an empty store and for a maximal `page_size`.

**Priority:** 🔴 CRITICAL — next in sequence after Issue #2.

---

### 4. `POST /redeem/complete` Credits Debt From the Request Body — No Chain Verification 🔴

**File:** `crates/basis_server/src/api.rs:2127-2239` → `crates/basis_store/src/redemption.rs:345-391`

The handler validates only hex decodability and key length, then forwards caller-supplied amounts
straight into the tracker thread (`api.rs:2187-2193`):

```rust
let cmd = TrackerCommand::CompleteRedemption {
    issuer_pubkey,
    recipient_pubkey,
    redeemed_amount: payload.redeemed_amount,          // unvalidated
    new_already_redeemed: payload.new_already_redeemed, // unvalidated
    response_tx,
};
```

`RedemptionManager::complete_redemption` then mutates state (`redemption.rs:363`):

```rust
note.amount_redeemed += redeemed_amount;   // no cap, no overflow check
```

**What is missing:**
- **No on-chain verification.** No call to `/blockchain/transaction/byId/`, no reserve box fetch,
  no R5 digest comparison, no confirmation depth check. Nothing confirms a transaction exists.
- **No outstanding-debt cap.** Contrast `initiate_redemption`, which *does* check
  (`redemption.rs:164-170`):
  ```rust
  if note.outstanding_debt() < request.amount {
      return Err(RedemptionError::InsufficientCollateral(...));
  }
  ```
- **No monotonicity check** on `new_already_redeemed`.
- **No check that a redemption was ever initiated or built.**
- **No `AuthContext` extraction anywhere in `crates/basis_server/src`** (verified: zero matches).
  The keys in the body need not correspond to the caller.

**Impact:** A caller can set `amount_redeemed >= amount_collected`. Because `outstanding_debt()`
uses `saturating_sub` (`lib.rs:1375-1377`), the note's debt silently clamps to 0 and becomes
**permanently unredeemable**. The poisoned reserve-tree value is then committed on-chain as the
tracker box R5 root.

The same gap exists on the current flow, `POST /redemption/submit`
(`crates/basis_server/src/redemption_build.rs:1188-1258`), which advances state on **mempool
acceptance**:

```rust
let tx_id = match node.broadcast(&payload.signed_tx).await { ... };  // :1218
// then immediately:
TrackerCommand::CompleteRedemption { redeemed_amount: payload.redeemed_amount, ... }  // :1238
```

`NodeClient::broadcast` (`redemption_build.rs:235-251`) only inspects the HTTP status of
`POST /transactions`. There is no verification that `signed_tx` actually spends the reserve box
or pays `redeemed_amount`. Post-broadcast state-sync failures are logged and the endpoint still
returns `200 OK` (`:1245-1257`).

**Fix Required:**
1. Cap `redeemed_amount` at `note.outstanding_debt()`.
2. Require proof of broadcast: either a tx id the server resolves via the node, or the server
   itself broadcasting the built transaction.
3. Add a pending→confirmed state machine for redemption transactions mirroring the one that
   already exists for tracker box updates, and gate state advancement on
   `confirmation.min_depth`.
4. Use `checked_add` and return an error on overflow.

**Priority:** 🔴 CRITICAL

---

### 5. No Idempotency or Replay Protection on Redemption Completion 🔴

**File:** `crates/basis_server/src/models.rs:169-180`, `redemption.rs:345-391`

`CompleteRedemptionRequest` carries a `redemption_id`, which is **accepted and discarded** — it is
never stored, looked up, or compared. Grep confirms `redemption_id` is only ever *written* in
`crates/basis_server/src` (`models.rs:172`, `api.rs:1762/2059/2084/2101`) and never read in the
completion path.

**Consequences:**
- **No idempotency key.** Replaying the identical request re-applies
  `note.amount_redeemed += redeemed_amount` and overwrites the reserve AVL leaf.
- **No double-redemption guard.** There is no test for double-completion in
  `crates/basis_server/tests/redemption_api_integration_tests.rs` (which only covers hex/length/
  note-existence failures at `:1010-1082`).

**Transport-level replay protection exists but does not close this gap.** The nonce cache in
`auth_middleware.rs:216-231`:
- is active only in `AuthMode::Signature`; `ApiKey` (`:133-153`) and `None` (`:89-97`) have none;
- degrades to keying on `(pubkey, timestamp)` when the client omits `X-Signature-Nonce` (`:185`);
- defaults to a 60 s window (`2 × 30 s` tolerance, `auth_middleware.rs:55-57`, `config.rs:265`);
- prevents only byte-identical replay, not semantically duplicate redemptions with fresh nonces.

**Fix:** Record `redemption_id` (or the broadcast tx id) in a bounded seen-set; make a repeat a
no-op success or a `409`.

**Priority:** 🔴 CRITICAL

---

### 6. Collateralization Monitoring Is a No-op — `total_debt` Hardcoded to 0 🔴

**File:** `crates/basis_store/src/reserve_tracker.rs:244`

```rust
total_debt: 0,
```

Every scanned reserve is constructed through this path (`ergo_scanner.rs:583-591`). Consequently:

| Method | Location | Production behavior |
|---|---|---|
| `collateralization_ratio()` | `reserve_tracker.rs:38-44` | always `f64::INFINITY` |
| `is_sufficiently_collateralized()` | `reserve_tracker.rs:47-50` | always `true` |
| `is_warning_level()` | `reserve_tracker.rs:53-55` | never fires |
| `is_critical_level()` | `reserve_tracker.rs:58-60` | never fires |

`add_debt`, `remove_debt`, `update_collateral`, `get_warning_reserves`, and `get_critical_reserves`
are called **only from test files** (verified: `reserve_tracking_test.rs:250,257,263,264,363`).

**Impact:**
- The collateralization gate at `reserve_tracker.rs:48` is inert, so nothing stops over-issuance.
- `reserve_api.rs:48,99,153` serve `INFINITY` to clients, while `api.rs:1570-1583`
  (`GET /key-status/{pubkey}`) computes a *different* ratio from notes. **The two reserve-status
  endpoints disagree.**
- Already acknowledged in-tree: `crates/basis_server/tests/redemption_api_integration_tests.rs:859-881`
  documents this as an "unmaintained reserve `total_debt` placeholder (always 0)".

**Related arithmetic risk:** `reserve_tracker.rs:48` uses unchecked `self.total_debt + amount`.
This is the *sole* collateralization gate, so a wrap makes `new_debt` small and the check passes.

**Fix:** Derive `total_debt` from the note set / reserve AVL tree during scanning, and replace the
unchecked addition with `checked_add`.

**Priority:** 🔴 CRITICAL

---

### 7. Tracker Signing Key Stored in Plaintext; the TODO Annotates a Dead Function 🔴

**File:** `crates/basis_server/src/config.rs:475-487`

```rust
/// TODO: Implement secure storage (e.g., HSM, key vault, encrypted keystore) for production use.
pub fn tracker_private_key_bytes(&self) -> Result<Option<[u8; 32]>, Box<dyn std::error::Error>> {
    // This is a temporary solution - in production, private keys should be
    // retrieved from secure storage
    match self.tracker_secret_key_bytes() {
        Some(secret_bytes) => Ok(Some(secret_bytes)),
        None => Ok(None),
    }
}
```

The tracker's Schnorr signing key (`ergo.tracker_secret_key`, 64 hex chars) is read from
`config/basis.toml` and decoded to raw `[u8; 32]` with no envelope encryption, no key derivation,
and no file-permission check (`config.rs:459-473`).

**What that key authorizes** (each site receives raw secret bytes):
- `redemption_build.rs:743` — signs the redemption attestation (context var #6). Possession allows
  **forged tracker approval for any redemption**.
- `redemption_build.rs:1112-1129` — signs fee inputs of redemption transactions.
- `tracker_box_updater.rs:184,1112-1129` — signs and broadcasts tracker box update transactions
  **out of the node's wallet**.
- `api.rs:2687, 2876` — signs tracker signatures for `POST /tracker/signature`.

Plaintext exposure of `config/basis.toml` therefore yields both forged redemption authorization
and the ability to spend the node's wallet.

**Three defects beyond the missing HSM:**
1. **The TODO is on a dead function.** `tracker_private_key_bytes` has **zero callers**. All real
   access goes through the un-annotated `tracker_secret_key_bytes`, so the TODO does not flag the
   actual key-handling path. `Result<Option<[u8; 32]>, Box<dyn Error>>` has no error variant — it
   can only ever return `Ok`.
2. **Malformed key material is silently ignored.** `config.rs:459-473` returns `None` if the hex
   fails to decode *or* the length is not 32. `redemption_build.rs:743-750` then reports a generic
   `500 tracker secret key not configured for local signing`, so a typo is indistinguishable from
   an absent key — and in `main.rs:536` it **silently disables tracker box updating**, leaving the
   on-chain AVL root digest permanently stale with no error.
3. **No zeroization.** The `[u8; 32]` is returned by value, cloned into
   `TrackerBoxUpdateConfig`, and converted to an `ergo_lib::SecretKey`; none are wiped.

**Fix:** Move the key to an encrypted keystore or an env/HSM reference; make decode and length
failures return `Err` rather than `None`; annotate `tracker_secret_key_bytes` as the real
accessor; zeroize on drop.

**Priority:** 🔴 CRITICAL

---

## High Priority Issues

### H1. Default Configuration Grants Anonymous Callers Full Admin 🟡

**Files:** `crates/basis_server/src/config.rs:89-94`, `auth_middleware.rs:89-97`

```rust
pub enum AuthMode {
    #[default]
    None,      // <-- default
```

```rust
AuthMode::None => {
    request.extensions_mut().insert(AuthContext {
        pubkey: None,
        role: ClientRole::Admin,   // <-- every anonymous request
```

Out of the box, `/reserves/create`, `/reserves/submit`, and `/acceptance/policy`
(`authorization.rs:59`, `Admin`) and `/redeem/complete` (`authorization.rs:63`, `Write`) are open
to the internet. `main.rs:915-922` only warns when auth is *on* without TLS; nothing warns when
auth is entirely off.

**Fix:** Default to a mode that fails closed, or refuse to start with `AuthMode::None` unless an
explicit opt-in flag is set; log a prominent warning at boot.

---

### H2. `POST /redemption/build` Does Not Verify the Issuer Signature 🟡

> **Severity note (added 2026-09-27).** Traced in Issue M14: the contract — not the server — is
> the enforcement point, and confirmed mainnet redemptions prove `basis.es` rejects forged
> signatures. Treat this as a defense-in-depth gap, not a fund-loss bug. The genuinely
> under-tested path is `/redemption/build` itself (Issue M14a).

**File:** `crates/basis_server/src/redemption_build.rs:731-739`

```rust
let issuer_signature = match hex::decode(payload.issuer_signature.trim()) {
    Ok(b) if b.len() == 65 => b,
    _ => return api_err(StatusCode::BAD_REQUEST, "issuer_signature must be 65-byte hex"),
};
```

Length-checked, then written directly into context var #2 (`:860`):

```rust
context_extension.insert("2".to_string(), serialize_coll_bytes(&issuer_signature));
```

There is **no signature verification anywhere in `redemption_build.rs`**. The legacy
`initiate_redemption` path does verify, via `note.verify_signature(&issuer_pubkey)`
(`basis_store/src/redemption.rs:137-139`, and again at `:417-418` in `verify_redemption_proof`) —
but see Issue M14b: the CLI's `generate-redemption --local-sign` path never calls `/redeem`.

The handler also has **no outstanding-debt check**: `payload.amount` is compared only against
reserve collateral (`:448-455`), never against `total_debt - already_redeemed`, so a caller may
request up to the entire reserve collateral regardless of outstanding debt.

**Fix:** Verify the Schnorr signature before building; add the outstanding-debt check.

---

### H3. Acceptance Policy Fails Open on a Storage Read Error 🟡

**File:** `crates/basis_server/src/acceptance/redemption_check.rs:215-222`

```rust
Err(e) => {
    tracing::warn!("Redemption policy check: failed to read stored policy for {}: {:?}", ...);
    global()          // <-- may resolve to DefaultPolicy::Accept
}
```

The parse-error path at `:205-212` correctly falls back to `DefaultPolicy::Reject`. A transient DB
read error instead resolves to the global default, so a holder's `Reject` policy silently flips to
accept.

---

### H4. Redemption Policy Check Silently Skipped When Pubkeys Do Not Decode 🟡

**File:** `crates/basis_server/src/api.rs:1787-1794` and `:1875-1879`

```rust
_ => {
    tracing::warn!("Skipping redemption policy check: could not decode issuer/recipient public keys");
}
```

Redemption proceeds with **no policy enforcement**. Note `normalize_public_key` is applied to the
issuer but not the recipient. The newer `/redemption/build` path handles this correctly
(`redemption_build.rs:519-572`, keys validated at `:321-333`).

---

### H5. AVL Proof Errors Silently Discarded; Dummy Node Resolver in Prover *and* Verifiers 🟡

**File:** `crates/basis_store/src/avl_tree.rs`

- `:49, :69, :88, :104` — `let _ = prover.generate_proof();` in `new`/`insert`/`update`/`remove`.
- `:115` — `let result = self.prover.perform_one_operation(&operation).ok().flatten();` in
  `generate_lookup_proof`. A lookup **error** is indistinguishable from **key absent**, and callers
  (`lib.rs:936`, `:1256`) discard the value entirely.
- `:62, :81, :97` — `let _ = self.prover...map_err(...)?;` is a no-op discard of an already-`?`'d
  `Result`; the enclosing function returns `Ok(())` unconditionally.
- `:16-30` — `simple_resolver` returns a **dummy all-zero leaf** rather than failing, and is
  installed into the prover (`:44`) *and* both verifiers (`:148`, `:179`):
  ```rust
  fn simple_resolver(_digest: &[u8; 32]) -> Node {
      // Return a dummy leaf node instead of panicking.
      key: Some(ADKey::from(vec![0u8; 32])), value: ADValue::from(vec![]), ...
  }
  ```
  Any digest requiring resolution silently resolves to a zero key, with no instrumentation
  proving it is never called.

Identical patterns at `crates/basis_trees/src/avl_tree.rs:26-40` (resolver, used by the verifier at
`:145`), `:123`, `:198` (`let _ = ... generate_proof()`), and `:126` (`.ok().flatten()`).

---

### H6. `basis_trees` Verifier Miscomputes Labels on the Update Path 🟡

**File:** `crates/basis_trees/src/avl_tree.rs:373-376`

```
// NOTE: the resulting verifier digest is NOT asserted here: ergo_avltree_rust 0.1.1's
// verifier miscomputes the final label for the update path (it diverges from the
// prover), while JVM scrypto 2.3.0's verifier matches the prover exactly
```

A digest-equality assertion was downgraded to a comment, meaning Rust-side and on-chain
verification of the update path are **known to disagree**. `Cargo.toml:24-27` compensates with the
vendored fork ("JVM-compatible insert semantics"), so the shipped `verify_insert_proof` path is
untested against this divergence. Note this is **not** merely theoretical: the vendored fork is
what makes the mainnet sequential redemptions in `tests/` succeed, so the divergence is currently
only caught by spending real funds (Issue M13).

---

### H7. Vacuous `verify()` Methods on Public Proof Types 🟡

**File:** `crates/basis_trees/src/proofs.rs`

- `:62-75` `MembershipProof::verify` → `Ok(!self.avl_proof.is_empty())` (`// Placeholder implementation`, `:73`)
- `:173-187` `NonMembershipProof::verify` → `Ok(!self.avl_proof.is_empty())` (`:185`)
- `:298-306` `StateProof::verify` → `Ok(!self.proof_data.is_empty())` (`:304`)

Re-exported as public API at `basis_trees/src/lib.rs:23`. Not currently invoked from production
paths, but the names are actively misleading and invite a future caller to trust them.

**Fix:** Implement real verification, or rename to `verify_placeholder` and make them `unreachable!`
outside tests.

---

### H8. Unchecked Debt Arithmetic Throughout; `AmountOverflow` Is Unreachable 🟡

**File:** `crates/basis_store/`

`NoteError::AmountOverflow` is declared (`lib.rs:285`) and mapped to an error string in five
handlers (`api.rs:304, 430, 542, 692, 786`), but it is **never constructed**: there are **zero**
`checked_add` / `checked_sub` / `checked_mul` calls in `crates/basis_store/src`.

| Location | Expression | Reachability |
|---|---|---|
| `crates/basis_server/src/redemption_build.rs:691` | `already_redeemed + payload.amount` | public handler, both operands influenced |
| `crates/basis_store/src/redemption.rs:238` | `note.amount_redeemed + request.amount` | public path |
| `crates/basis_store/src/redemption.rs:363` | `note.amount_redeemed += redeemed_amount` | via Issues #4/#5 |
| `crates/basis_store/src/redemption.rs:1334` | `note.amount_redeemed + request.amount` | public path |
| `crates/basis_store/src/reserve_tracker.rs:48` | `self.total_debt + amount` | collateralization gate (see Issue #6) |
| `crates/basis_store/src/reserve_tracker.rs:142` | `reserve.total_debt + amount` | error message |
| `crates/basis_store/src/reserve_tracker.rs:146` | `reserve.total_debt += amount` | public path |

**Fix:** Adopt `checked_*` throughout and construct `AmountOverflow`, or delete the variant.

---

### H9. Hardcoded Fake Events Injected Into the Event Store on Every Startup 🟡

**File:** `crates/basis_server/src/main.rs:576-683`

Seven fabricated `TrackerEvent`s with pubkeys `"0101...01"` / `"0202...02"`, box id
`"box1234567890abcdef"`, timestamps `1234567890..1234567896`, heights `1000..1003` are added
**unconditionally** — there is no config gate:

```rust
for event in demo_events {
    if let Err(e) = event_store.add_event(event).await {
        tracing::warn!("Failed to add demo event: {:?}", e);
    }
}
```

They occupy event IDs 1-7, so real events are never returned as "recent" by `GET /events`
(`api.rs:1437`) or `GET /events/paginated` (`api.rs:1401`). Because `EventStore` is in-memory
only (`store.rs:12-19`, unbounded `Vec`, no eviction, no disk persistence), the contamination also
makes the paginated endpoint trivially panicable (Issue #3).

**Fix:** Gate behind an explicit `demo_mode` flag, or remove.

---

### H10. `SharedTrackerState` Fabricates a Syntactically Valid Digest on Poisoned Lock 🟡

**File:** `crates/basis_server/src/tracker_box_updater.rs:108-122`

```rust
pub fn get_avl_root_digest(&self) -> [u8; 33] {
    if let Ok(root_lock) = self.avl_root_digest.read() { *root_lock } else { [0u8; 33] }
}
pub fn get_tracker_pubkey(&self) -> [u8; 33] {
    if let Ok(pubkey_lock) = self.tracker_pubkey.read() { *pubkey_lock } else { [0u8; 33] }
}
```

`[0u8; 33]` is a *well-formed* digest that would be committed to R5 as a real state commitment.
The "not initialized" case is only checked at `:416`, so a poisoned lock is indistinguishable
from uninitialized — and is a value that would be written on-chain. `set_*` (`:84-106`)
correspondingly no-ops silently.

---

### H11. Replay Cache Populated *Before* Signature Verification; Unbounded Body Read 🟡

**File:** `crates/basis_server/src/auth_middleware.rs`

- `:224-231` inserts `(pubkey, nonce)` into the replay cache; actual Schnorr verification happens
  at `:250-265`. Anyone who knows an authorized client's pubkey can **pre-poison the cache** and
  lock that client out for the full TTL (`:55-57`, `2 × 30 s = 60 s`).
- `:235` — `to_bytes(body, usize::MAX)` buffers the entire request body with no cap, bypassing
  axum's `DefaultBodyLimit` (which only applies to extractors) → unbounded memory per request.

**Fix:** Verify the signature first, then insert into the cache; cap the body size.

---

### H12. No Rate Limiting Anywhere 🟡

Consistent with the previous audit's open item. Combined with `AuthMode::None` (H1), the server
exposes unbounded, unauthenticated, unthrottled state-mutating endpoints.

---

## Medium Priority Issues

### M1. `TrackerBoxUpdater` Silently Drops Commands to the Tracker Thread 🟢

**File:** `crates/basis_server/src/tracker_box_updater.rs:378-385, 452-460, 503-511`

```rust
let _ = tx.send(crate::TrackerCommand::ConfirmPendingNotes { .. }).await;
let _ = rrx.await;
```

Note/pending-state divergence is invisible, and the cycle proceeds to submit another tracker
update against a digest the tracker may not have recorded.

### M2. R5 Register Parsed Without Validating the SAvlTree Type Byte 🟢

**File:** `crates/basis_server/src/tracker_box_updater.rs:432-470`

```rust
if let Ok(r5_bytes) = hex::decode(r5_value) {   // error silently ignored
    if r5_bytes.len() >= 34 {
        let onchain_digest = &r5_bytes[1..34];   // no check that r5_bytes[0] == 0x64
```

A malformed R5 is silently skipped, and the mismatch branch (`:483-490`) only warns — the updater
then submits a new update based on an unverified view.

### M3. `build_savl_tree_from_digest` Substitutes an All-Zero Digest on Bad Hex 🟢

**File:** `crates/basis_offchain/src/ergo_tx.rs:59`

```rust
let digest_bytes = hex::decode(digest_hex).unwrap_or_else(|_| vec![0u8; 33]);
```

A malformed digest silently becomes a **valid-looking SAvlTree R5** committed on-chain — a
swallowed error directly in the state-commitment path.

### M4. `u64 as i64` Casts Into On-Chain Context Vars 🟢

**File:** `crates/basis_server/src/redemption_build.rs:861` (`total_debt as i64`), `:864`
(`timestamp as i64`), `:983` (`refund_initiation_height as i64`); `basis_offchain/src/ergo_tx.rs:46`.
Values above `i64::MAX` wrap negative and are zigzag-VLQ encoded into context vars #3, #4 and R7.

### M5. Startup Panics 🟢

- `crates/basis_store/src/lib.rs:336-341` — `panic!("Failed to initialize note storage: {:?}")`,
  with the comment *"In production, this should handle errors properly"*.
  `lib.rs:350-353` — `panic!("Failed to initialize AVL tree: {:?}")`.
  `TrackerStateManager::new` is called from `main.rs` and from `TrackerServerState::clone()`
  (`tracker_scanner.rs:90`), so any DB/permission error kills the server.
- `crates/basis_server/src/config.rs:302` — `.expect("Invalid socket address")` with no
  validation on deserialize. `host = "localhost"` (a very natural config) **panics at boot**;
  `main.rs:894` calls it.
- `crates/basis_server/src/main.rs:129` — `panic!("Failed to create minimal scanner")` on the
  fallback path of an already-warned failure.
- `main.rs:941, 945` — `.expect("tls_cert_path checked by tls_enabled")`, coupled to
  `config.rs::tls_enabled()` by convention only.

### M6. `update_tracker_state` Is a Logging-Only Stub 🟢

**File:** `crates/basis_store/src/tracker_scanner.rs:342-368`

```rust
// For now, we'll just log the boxes
// In a real implementation, this would update the tracker state manager
// with cross-verification logic
```

Called from `main.rs:209` and `tracker_scanner.rs:455`; returns `Ok(())` having done nothing.
Tracker boxes are fetched and persisted but **never cross-verified** against local state.

### M7. Confirmation-Record Persistence Failures Swallowed 🟢

**File:** `crates/basis_store/src/lib.rs:626, 714, 750, 791, 835` —
`let _ = self.storage.store_confirmation(key, entry);` across `recompute_confirmation_status`,
`mark_notes_pending`, `confirm_pending_notes`, `revert_pending_notes`, and
`reconcile_with_confirmed_digest`. A disk-write failure leaves in-memory and on-chain views
permanently divergent with no alarm.

Two related defects in the same file:
- `lib.rs:646` — `self.storage.get_all_confirmations().unwrap_or_default();` treats a read error
  as "no confirmations", so after a restart every note silently becomes `LocalOnly`.
- `lib.rs:698` — `let _ = (digest, submitted_height);` in `mark_notes_pending` discards the two
  values identifying *which* commitment the pending state refers to.

### M8. Dead Reserve Event Pipeline 🟢

- `crates/basis_store/src/ergo_scanner.rs:765-789` — `ReserveEvent::ReserveRedeemed` and
  `ReserveSpent` have **no producer** anywhere in the tree.
- `crates/basis_server/src/main.rs:1123-1124` — `process_reserve_event` is
  `#[allow(dead_code)]` and never called.
- The `EventType::ReserveRedeemed` / `ReserveSpent` events visible via `GET /events` are the
  hardcoded demo events from H9.

Spending detection (`ergo_scanner.rs:646-694`) infers a spent reserve by **absence from the
unspent set**, and only when `!current_box_ids.is_empty() && parse_failures == 0`. Critically, it
removes the reserve from `ReserveTracker`/`ReserveStorage` but does **not** decrement note
`amount_redeemed`, does not touch the reserve AVL tree, and emits no event. The scanner only ever
queries `/blockchain/box/unspent/byAddress` (`:357-436`); there is no spent-box or
transaction-level scanning.

### M9. `EventStore` Is Unbounded and In-Memory Only 🟢

**File:** `crates/basis_server/src/store.rs:12-36`

```rust
// In a real implementation, this would load from disk
// For now, we'll use in-memory but structured for easy disk persistence
```

`events.push(event)` with no cap and no eviction → unbounded growth for the process lifetime, and
total loss of history on restart.

### M10. Hardcoded `NetworkPrefix::Mainnet` and Default Public Node 🟢

- `crates/basis_server/src/api.rs:1656` — `AddressEncoder::new(NetworkPrefix::Mainnet)` with the
  comment *"could be configurable"*. Produces mainnet addresses on testnet.
  Also `main.rs:523` (`let _network_prefix = ...`, a dead binding) and
  `basis_offchain/src/ergo_tx.rs:118`.
- `crates/basis_server/src/config.rs:269` — default `ergo.node.node_url` is
  `http://159.89.116.15:11088`, a **plaintext public mainnet node**. Same in
  `crates/basis_store/src/ergo_scanner.rs:799`. A misconfigured deployment silently talks
  plaintext to a third party.
- `crates/basis_server/src/main.rs:44-65` — config-load failure falls back to a hardcoded ~1.5 KB
  mainnet P2S address, so a config typo silently binds the server to a mainnet contract.
  Duplicated at `basis_store/src/contract_compiler.rs:18`.

### M11. Hardcoded Placeholders in the Legacy Redemption Path 🟢

| Location | Value |
|---|---|
| `basis_store/src/redemption.rs:296`, `:1399` | `vec!["test_fee_input_placeholder"]` |
| `basis_store/src/redemption.rs:317` | `"tracker_signature_key"` — *"Placeholder - in real implementation, this would be tracker's pubkey"* |
| `basis_store/src/redemption.rs:1413` | `"tracker_pubkey_required"` |
| `basis_store/src/redemption.rs:1376`, `:1417` | `fee: 1000000` / `estimated_fee = 1000000`, ignoring `config.transaction.fee` |
| `basis_store/src/redemption.rs:1290-1291` | `vec![0u8; 65]` dummy tracker signature on emergency redemption |
| `basis_server/src/api.rs:2042` | `value: 100000, // Minimum ERG value for box (0.001 ERG)` — comment off by 10× |
| `basis_server/src/api.rs:1577`, `:1582` | `999999.0` magic sentinel for "no debt" |
| `crates/basis_store/src/transaction_builder.rs:374` | `let total_debt = note.amount_collected;` |
| `crates/basis_cli/src/commands/transaction.rs:20` | `const NODE_URL: &str = "http://127.0.0.1:9053";` |

### M12. `reserve_nft_id` Taken From `assets.first()` 🟢

**File:** `crates/basis_server/src/redemption_build.rs:803-807`

```rust
let reserve_nft_id = reserve_box.assets.first()
    .map(|a| a.token_id.clone())
    .unwrap_or_else(|| tracker_nft_id.clone());
```

Nothing enforces that `assets[0]` is the reserve NFT; a differently-ordered asset list writes the
wrong NFT into the preserved reserve output (`:934-937`).

### M13. Multi-Redemption Is Mainnet-Validated but Has No Automated Regression Guard 🟢

**Correction to the previous revision of this audit.** An earlier draft of M13 claimed the `#7`
reserve lookup proof was "never verified against a non-empty tree by the real contract." **That was
wrong**, and this issue has been rewritten. The repo contains four dedicated mainnet integration
harnesses under `tests/`, and multi-redemption has been validated on real Ergo mainnet repeatedly
(`ea01252`, 2026-08-03 "locally signed sequential redemptions tested"; `2d7bdcc`, 2026-08-20).

| Script | Scenario |
|---|---|
| `test_mainnet_02erg_3redemptions.py` | 0.2 ERG reserve, 3 × 0.04 ERG |
| `test_mainnet_04erg_3redemptions_restart.py` | 0.4 ERG reserve, 3 × 0.1 ERG, **server restart after redemption 2** + state-recovery check |
| `test_mainnet_use_token_3redemptions_restart.py` | **token-backed (USE) reserve**, 3 × 0.1 USD, restart after redemption 1 |
| `test_local_sign_multiple_redemptions.py` | 0.3 ERG reserve, 2 × 0.1 ERG |

These are genuine end-to-end runs, not mocks: the issuer DLOG secret is exported from the unlocked
node wallet (`/wallet/getPrivateKey`), signatures are real Schnorr, transactions are broadcast and
confirmed via `wait_for_tx` (`:111-136` — polls `/transactions/unconfirmed/{txid}` until it leaves
the mempool, then confirms via `/wallet/transactionById`), the tracker box commitment is verified
on-chain (`wait_for_note_confirmed`, `:158-174`, requires `status == "confirmed" && redeemable`),
and each redemption's collateral reduction is observed by the scanner
(`wait_for_reserve_updated`, `:389-404`).

**So the real contract does validate the `#7` lookup proof** — only the mainnet ErgoScript
interpreter can reduce a valid `basis.es` script, and redemptions 2 and 3 confirm. This also means
the vendored AVL fork (`Cargo.toml:24-27`) has live mainnet evidence behind it, not just one
manual run.

**The actual gap is regression protection, not correctness:**

**a. None of these run in CI.** `grep` over `.github/workflows/test.yml` finds no reference to any
script in `tests/` — they require a funded mainnet wallet, so they are manual-only. A regression in
the sequential-redemption path would not be caught by any automated check.

**b. The Scala suite — the only automated real-interpreter guard — has zero coverage of the
subsequent-redemption path.** `mkLookupProof` is defined at
`scala/src/test/scala/basis/contracts/BasisSpec.scala:1754` and has **zero call sites**. All 22
proof setups use `mkTreeAndProof`, which builds the proof from an *empty* tree; the comment at
`:1740-1741` even states "For first or subsequent redemption: start with empty tree". So of ~40
Scala properties, **none exercise a subsequent redemption.** This is the highest-value remaining
item: it is the one check that would catch a regression without spending real mainnet funds.

**c. The Rust unit test's mock validator hardcodes first-redemption state.**
`crates/basis_store/src/redemption_blockchain_tests.rs:124`:

```rust
let stored_timestamp = 0u64; // First redemption
```

`MockContractValidator::validate_redemption` is invoked a second time at `:1061-1076` for the
second redemption but still validates as if first. `already_redeemed` *is* threaded through
(`:130`), so the amount check runs, but **timestamp monotonicity is never exercised in the mock.**

**d. That unit test bypasses `initiate_redemption` for redemption 2**, by design — the comment at
`:1043-1046` explains that `complete_redemption` refreshes the note timestamp and so invalidates
the original signature. It calls `build_unsigned_redemption_transaction` directly with
`avl_proof: vec![]`, `operations: vec![]` (`:1100-1101`). The mainnet harnesses cover this path
properly via the real `basis_cli transaction generate-redemption --local-sign` flow, so this is a
unit-test-only limitation.

**e. The multi-redemption proptest is near-vacuous.**
`crates/basis_store/src/property_tests.rs:357-418` runs 1..10 sequential redemptions, but at
`:404` it uses `if result.is_ok()`, silently tolerating failures, and the signatures are
`"01".repeat(65)` / `"02".repeat(65)`.

**Fix:** Add a Scala property that performs a genuine second redemption — build a non-empty reserve
tree, derive `mkLookupProof` against it, reduce it through the real interpreter — and wire it into
CI. Then parameterize `MockContractValidator` with the prior timestamp. Also document the
prerequisites and expected `WALLET_ADDRESS` setup for the four mainnet scripts so they can be run
on demand (a `tests/README.md` does not currently exist).

---

### M14. Mainnet Coverage Exercises the Legacy Flow, Not the Current One 🟢

Verified by tracing every endpoint the harnesses and the CLI actually call (2026-09-27).

**a. The harnesses drive the legacy redemption flow only.** All four scripts invoke
`basis_cli transaction generate-redemption --local-sign`, which fetches `/tracker/proof` and
`/reserve/proof` and broadcasts client-side. **None uses `redeem-assisted`**
(`crates/basis_cli/src/commands/transaction.rs:135`), i.e. the 2-phase
`POST /redemption/build` + `POST /redemption/submit` flow described in that command's own doc
comment as exercising "the new 2-phase server endpoints end-to-end."

`/redemption/build` is exactly where Issue H2 lives (no issuer signature verification, no
outstanding-debt check), and its only coverage is the mocked HTTP tests in
`crates/basis_server/tests/redemption_api_integration_tests.rs` (zero references to a real node).
**The current redemption path therefore has neither mainnet nor real-node coverage.**

**b. Correction to H2 — and a narrowing of its severity.** H2 states that
`RedemptionManager` "does verify," citing `redemption.rs:138` and `:415`. That is correct in
substance: both call `note.verify_signature(&issuer_pubkey)`
(`redemption.rs:137-139`, `:417-418`), not a literal `schnorr_verify`. However:

- `initiate_redemption` (`POST /redeem`) is the **only** server path that verifies the issuer
  signature.
- The CLI's `generate-redemption --local-sign` path **never calls `/redeem`** — it calls only
  `get_note`, `get_latest_tracker_box_id`, `get_reserves_by_issuer`, `get_tracker_proof`,
  `get_reserve_proof`, `get_reserve_token_config`, and `get_basis_reserve_contract_p2s`.

So server-side signature verification is bypassed in precisely the flow that has mainnet coverage.
The actual enforcement point is the **contract**, which is why the flow works — confirmed redemptions
prove `basis.es` rejects forged signatures. **H2 is therefore a defense-in-depth gap, not a
fund-loss bug**, and should be prioritised accordingly. The same reasoning applies to the missing
outstanding-debt check in `/redemption/build`: the contract enforces `redeemed <= totalDebt -
alreadyRedeemed` on-chain.

**c. Doc drift in the harness docstrings.** All four claim the test exports the issuer key via
`/wallet/getPrivateKey` (in fact the CLI does, via `crypto.rs`) and reference
`/wallet/payment/send` while the code calls `POST /wallet/transaction/send`.

**d. What does check out.** Every contract the harnesses depend on matches current code: all six
tracker routes they call are registered (`main.rs:788, 849, 807, 833, 851, 853`); all CLI flags
match the clap definitions (`transaction.rs:96-130`); all five server routes the CLI build path
needs are registered (`main.rs:811, 814, 858, 855, 842`); and the request/response field names
match (`models.rs:449-452`, `:456-466`, `:169-180`). There is **no** endpoint or schema drift —
the gap is coverage, not correctness.

**Fix:** Add a mainnet harness (or an opt-in CI job) for `redeem-assisted` covering
`/redemption/build` + `/redemption/submit`; fix the docstring drift; and record explicitly that
contract-level enforcement — not server-side checks — is the security boundary for redemption.

---

## Low Priority

| # | Item | Location |
|---|---|---|
| L1 | `basis_cli` panics on any server error — 17 × `data.unwrap()`. The server's error shape is `{"success":false,"data":null,...}` (`models.rs:532`), so **every 4xx/5xx crashes the CLI** instead of surfacing the error. | `crates/basis_cli/src/api.rs:472,508,576,710,740,769,780,820,869,936,958,976,1002,1021,1054,1197,1223,1244` |
| L2 | TUI panics: `current_account.as_ref().unwrap()` with no account loaded; `stdin().read_line().unwrap()` panics on EOF; `stdout().flush().unwrap()` panics on closed stdout. | `crates/basis_app/src/ui.rs:1078,1227,1520,1670,2263`; `:1849,1857,1865`; `:133,1847,1855,1863` |
| L3 | Demo key loading `.unwrap()` / `.expect("... secret not found in secrets/participants.csv")`. | `crates/basis_cli/src/demo_keys.rs:80,81,122,140,141,147,173` |
| L4 | `reserve_tracker.rs` `.unwrap()` on every `RwLock` acquisition (11 sites). One panic while the write lock is held poisons it, after which *every* reserve read panics — a permanent DoS. `std::sync::RwLock` also blocks the async executor. | `crates/basis_store/src/reserve_tracker.rs:90,97,110,119,125,134,152,172,183,193,203` |
| L5 | `.unwrap()` on `SystemTime::duration_since` — panics if the clock predates the Unix epoch. The safe `unwrap_or_default()` idiom is already used elsewhere in the same files. | `basis_store/src/lib.rs:1315-1318`; `reserve_tracker.rs:247-250`; `redemption.rs:324-327,1421-1424`; `basis_trees/src/avl_tree.rs:249-252` |
| L6 | Slicing inside `debug!`/`info!` args. Length-validated today, but `RUST_LOG` defaults to `basis_store=debug` (`main.rs:100-102`) so the args **are** evaluated. | `tracker_scanner.rs:358-360`; `api.rs:3561` |

Also noted: `basis_trees/src/storage.rs:1-88` is an in-memory `TreeStorage` stub (unreferenced
from production); `basis_offchain/src/lib.rs:11` and `basis_app/src/lib.rs:3` are placeholders;
`PredicateBuilder::clone_predicate` (`acceptance/builder.rs:184-190`) would write a
permanently-false `AnyOf` into a cache, currently dead but dangerous if the empty cache-read
branch at `:75-79` is ever completed.

### Duplicated Cryptography — No Cross-Check Test

Three independent Blake2b implementations (`basis_offchain/src/ergo_tx.rs:114`,
`basis_store/src/lib.rs:1481-1483`, `basis_cli/src/commands/note.rs:628-630`) and three Schnorr
implementations (`basis_core/src/impls.rs:31-129`, `basis_offchain/src/schnorr.rs`,
`basis_cli/src/crypto.rs:59-160`), with no test enforcing they stay identical. The Scala
cross-validation in `specs/SCHNORR_SIGNATURE_SPEC.md` covers `basis_core` only.

---

## Resolved Since the Last Audit

These remain fixed and should not be re-litigated:

1. **CLI transaction builder** — real signatures and proofs from server APIs
   (`crates/basis_cli/src/commands/transaction.rs`).
2. **Redemption manager placeholders** — real box IDs, heights, addresses
   (`crates/basis_store/src/redemption.rs`, `crates/basis_server/src/api.rs`).
3. **Transaction builder first-redemption detection** — was listed as the sole remaining CRITICAL
   in the previous revision; that was **stale**. Fixed at
   `crates/basis_store/src/transaction_builder.rs:371-373`:
   ```rust
   let already_redeemed = note.amount_redeemed;
   let is_first_redemption = already_redeemed == 0;
   ```
   Covered by `test_two_sequential_redemptions_0_3_erg_reserve_with_nft`
   (`crates/basis_store/src/redemption_blockchain_tests.rs:915-1178`), which asserts the reserve
   value trajectory (0.3 → 0.2 → 0.1 ERG), NFT preservation, recipient payouts, and the
   `#5`/`#7` context-variable shape for first vs. subsequent redemptions.

   **Additionally validated on real Ergo mainnet** by four integration harnesses in `tests/`
   (see Issue M13): two ERG-backed (2 and 3 redemptions), one token-backed USE, and one
   local-sign variant, two of which restart the tracker server mid-flow and verify state
   recovery. Commits `ea01252` (2026-08-03) and `2d7bdcc` (2026-08-20). The mainnet runs use
   real DLOG keys from the node wallet, real Schnorr signatures, and confirmed on-chain
   redemptions — so the `#7` lookup proof *is* accepted by the real `basis.es` interpreter.
   The residual gap is automated regression coverage, not correctness (Issue M13).
4. **Tracker box updater R5 format** — correct 37-byte `SAvlTree` serialization
   (`tracker_box_updater.rs`); validated against a local node in July 2026.
5. **CLI address generation** — proper P2PK derivation via ergo-lib.
6. **Reserve tracker contract address** — populated from config.
7. **API redemption register values** — correct R4/R5/R6 serialization.
8. **Server register fallback** — skips boxes without R4 instead of inventing owner pubkeys.
9. **CLI API contract address and box bytes** — fetched from server config and the node.

---

## Recommended Action Plan

### Phase 0: Restore Verification (blocking, ~1 day)

- [ ] Vendor the three `sigma-rust` crates into `temp/vendors/` or repoint the patches to a git
      ref (Issue #1).
- [ ] **Verified 2026-09-27:** the fix requires bumping the *declared versions*, not just the
      paths. The code needs `ergo_lib::ergotree_ir::chain::{context, context_extension}`
      (`crates/basis_offchain/src/signing.rs:21-22`), which only exist at sigma-rust ≥ `dc6c41c6`,
      by which point the crates are **0.29.0**. Declared `0.28.0` (`Cargo.toml:16`,
      `crates/basis_store/Cargo.toml:32-33`) makes cargo **silently drop** the `[patch]` entries,
      which is why the committed `Cargo.lock` shows `ergo-lib 0.28.0` from crates.io with zero
      sigma-rust references. Bumping all three to `0.29.0` was confirmed to compile and to make
      the path patches apply — but the **absolute paths must still be replaced** (git `rev`, or
      vendored) or CI stays red.
- [ ] Confirm `cargo build --workspace` from a clean checkout on a machine without
      `/home/kushti/ergo/sigma-rust`.
- [ ] Get CI green; confirm the OpenAPI consistency test and `sbt test` actually execute.
- [ ] **Re-audit the 9 "resolved" items above now that CI runs** — they have not been
      machine-verified in weeks.

### Phase 1: Remote Crash Fixes (half day)

- [x] Hoist the `len() == 33` checks above the slice in `api.rs:2989`; remove fixed-offset
      slicing entirely (Issue #2) — **done 2026-09-27**, 3 regression tests added
- [ ] Clamp `page` / `page_size` and guard the empty case in `store.rs:44-46` (Issue #3)
- [ ] Add regression tests for Issue #3; assert no panic on `?page=1` with an empty store

### Phase 2: Redemption Integrity (1-2 days)

- [ ] Cap `redeemed_amount` at `note.outstanding_debt()`; use `checked_add` (Issues #4, H8).
- [ ] Gate state advancement on on-chain evidence: resolve the broadcast tx via the node, and
      apply `confirmation.min_depth` to redemptions, not just tracker box updates (Issue #4).
- [ ] Add a pending→confirmed redemption state machine mirroring `SharedTrackerState` (Issue #4).
- [ ] Record `redemption_id` / tx id in a bounded seen-set; make repeats idempotent (Issue #5).
- [ ] Verify the issuer Schnorr signature in `/redemption/build`; add the outstanding-debt check
      (Issue H2).
- [ ] Add integration tests for double-completion, over-redemption, and reorg.

### Phase 3: Collateralization and Monitoring (1 day)

- [ ] Derive `ExtendedReserveInfo::total_debt` from the note set / reserve AVL tree during
      scanning (Issue #6).
- [ ] Reconcile the two disagreeing reserve-status endpoints (`reserve_api.rs` vs
      `api.rs:1570-1583`).
- [ ] Gate or remove the hardcoded demo events (Issue H9).
- [ ] Bound and persist `EventStore` (Issue M9).

### Phase 4: Scanner and Cross-Verification (2-3 days)

- [ ] Add spent-box or transaction-level scanning; produce real `ReserveEvent::ReserveRedeemed`
      events and wire up `process_reserve_event` (Issue M8).
- [ ] Implement `update_tracker_state` cross-verification, or remove the call and the
      misleading signature (Issue M6).
- [ ] Surface confirmation-persistence failures instead of `let _ =` (Issue M7).

### Phase 5: Cryptographic Correctness (2 days)

- [ ] Replace the dummy AVL node resolver with real node storage; stop discarding proof errors
      (Issue H5).
- [ ] Implement real `verify()` for the three proof types, or make them explicitly unreachable
      outside tests (Issue H7).
- [ ] Resolve the update-path verifier divergence or pin the fork and assert against JVM vectors
      (Issue H6). Note the fork does have live mainnet evidence from the `tests/` harnesses.
- [ ] Add a Scala property that reduces a real second redemption through the interpreter, wire it
      into CI, and parameterize `MockContractValidator` with the prior timestamp (Issue M13).
- [ ] Move the tracker key to an encrypted keystore; make decode failures `Err` not `None`;
      zeroize on drop (Issue #7).
- [ ] Add a cross-implementation test that all three Schnorr/Blake2b copies agree.

### Phase 6: Robustness Sweep (1 day)

- [ ] Replace the two startup `panic!`s and `socket_addr().expect(...)` (Issue M5).
- [ ] Fail closed in `redemption_check.rs:215-222` and stop skipping the policy check on
      undecodable pubkeys (Issues H3, H4).
- [ ] Default to a fail-closed auth mode (Issue H1).
- [ ] Fix the replay-cache ordering and cap the request body (Issue H11).
- [ ] Add rate limiting (Issue H12).
- [ ] Fix the 17 CLI `data.unwrap()` panics and the TUI `unwrap()`s (Issues L1, L2).

### Phase 7: Remaining Validation

- [x] End-to-end single redemption on mainnet (first confirmed 2026-07-19)
- [x] Tracker box update test — live against a mainnet-synced node, R5 digest matched the
      server's AVL root after note creation
- [x] Multi-redemption end-to-end on mainnet — `tests/test_mainnet_02erg_3redemptions.sh`,
      `test_mainnet_04erg_3redemptions_restart.sh`, `test_mainnet_use_token_3redemptions_restart.sh`,
      `test_local_sign_multiple_redemptions.sh` (manual; need a funded mainnet wallet)
- [ ] Add a Scala property reducing a **genuine** second redemption (non-empty tree, real
      `mkLookupProof`, real interpreter) and wire it into CI — Issue M13b
- [ ] Parameterize `MockContractValidator` with the prior timestamp — Issue M13c
- [ ] Document mainnet-script prerequisites (`tests/README.md`) — Issue M13
- [ ] Emergency redemption end-to-end test — still outstanding
- [ ] Mainnet coverage for the 2-phase `redeem-assisted` flow (`/redemption/build` +
      `/redemption/submit`) — Issue M14a
- [ ] Reorg / dropped-transaction recovery test

---

## Production Readiness Checklist

### Build and Verification
- [ ] Workspace builds outside `/home/kushti` (Issue #1)
- [ ] CI green; all 12 workflow steps executing
- [ ] `cargo clippy --all -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] OpenAPI consistency test passing
- [ ] `sbt test` (Scala contract tests) passing

### Core Protocol
- [x] Signing message format (`key || totalDebt || timestamp`, 48 bytes)
- [x] Emergency redemption uses same message format, tracker signature becomes optional
- [x] Context extension variables (#0-#8)
- [x] Tracker AVL tree storage (`hash(A||B) -> totalDebt`)
- [x] Tracker proof API endpoint
- [x] CLI transaction generation
- [x] Server redemption flow
- [x] Tracker box updates
- [x] Sequential redemptions against a single reserve (R5 flags `0x03`) — Rust unit test asserts
      value trajectory, NFT preservation, and `#5`/`#7` shape
- [x] Multi-redemption on real Ergo mainnet — 4 harnesses in `tests/`, up to 3 redemptions, incl.
      server-restart state recovery and a token-backed (USE) variant
- [ ] Subsequent redemption reduced through the real interpreter **in CI** (Issue M13b)

### Blockchain Integration
- [x] Reserve box scanning (unspent only)
- [x] Tracker box scanning
- [x] Current height retrieval (10-min caching)
- [x] Box serialization
- [x] Transaction submission
- [ ] Spent-box / tx-level scanning (Issue M8)
- [ ] Redemption events derived from chain data (Issue M8)
- [ ] Confirmation depth applied to redemptions (Issue #4)
- [ ] Reorg handling (deferred per `specs/pr12_triage.md:79-80`)

### Error Handling
- [ ] No panics reachable from request handlers (Issue #3 remains; #2 fixed)
- [x] Proper error responses (not panics) — for handled error paths
- [ ] No startup panics on bad config or storage failure (Issue M5)
- [ ] CLI surfaces server errors instead of panicking (Issue L1)
- [x] Graceful degradation
- [x] Logging and monitoring

### Security
- [ ] Verify issuer Schnorr signature in `/redemption/build` (defense-in-depth; contract already
      enforces) — Issue H2
- [x] Schnorr verification in `RedemptionManager::initiate_redemption` — but **not** reached by the
      CLI local-sign path (Issue M14b)
- [ ] AVL proof verification backed by real node resolution (Issue H5)
- [ ] `verify()` methods are not vacuous (Issue H7)
- [ ] Tracker key out of plaintext config (Issue #7)
- [ ] Fail-closed default auth mode (Issue H1)
- [ ] Nonce cached after signature verification; bounded body (Issue H11)
- [ ] Acceptance policy fails closed (Issues H3, H4)
- [x] Replay protection in `AuthMode::Signature`
- [ ] Idempotency on redemption completion (Issue #5)
- [ ] Checked debt arithmetic (Issue H8)
- [ ] Rate limiting (Issue H12)
- [ ] Key management for CLI (basic wallet support)
- [ ] Input validation — partial

---

## Conclusion

**Current Status:** 🔴 **NOT PRODUCTION READY**

The previous "✅ PRODUCTION READY" verdict does not hold. The protocol implementation is real and
has been exercised on mainnet, but three things block deployment:

1. **Nothing is verified.** CI has been red on every push since at least 2026-08-21 because
   `[patch.crates-io]` points at absolute local paths. No test, lint, format check, OpenAPI
   consistency check, or Scala contract test has run in weeks.
2. **The server returns 500 on attacker-controlled input in two handlers.** `POST /redemption/prepare`
   and `GET /events/paginated` both panic on short or out-of-range input, and the default auth
   mode grants `Admin` to every unauthenticated caller. Note these surface as caught 500s
   (`#[axum::debug_handler]` + `panic = "unwind"`), not process aborts. Issue #2 is fixed;
   Issue #3 remains.
3. **Redemption completion trusts the caller.** Debt is credited from the request body with no
   outstanding-debt cap, no idempotency, and no confirmation-depth gate — and the collateralization
   monitor that should catch over-issuance is inert because `total_debt` is hardcoded to `0`.

**Recommendation:** Complete Phase 0 and Phase 1 before any network exposure. These are small,
well-localized changes (a path fix, a length check, a bounds clamp) and they restore the feedback
loop needed to land everything else safely. Phases 2-3 should follow before handling real value.
