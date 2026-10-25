# PR #16 review triage

Triage of the security review posted on PR #16 ("Fixes for security issues reported in #15"),
authored by decentbob (Claude Opus 5.5), reviewed at commit `94ae877` against base `b6b1573`.
Status column refers to the current working tree of branch `pr14-remediation` (which includes the
C3 message change — the signed message is now 80 bytes:
`key(32) || totalDebt(8 BE) || timestamp(8 BE) || trackerNftId(32)`).

## Disposition table

| # | Severity | Finding | Disposition |
|---|----------|---------|-------------|
| B1 | Blocker | `basis.es` throws when any input has a non-`GroupElement` R4 | **Fixed** — guard restored in both contracts; regression spec added |
| B2 | Blocker | S3 bypass with `totalDebt` ≥ 2^63 | **Fixed** — `i64::MAX` caps in `add_note`/`update_note`, 400 from `POST /notes`; `if let Ok(existing)` no longer swallows storage errors |
| B3 | Blocker | `/reserves/submit` rejects most legitimate payloads | **Fixed by removal** — see B4: the endpoint is gone, so the fingerprint mismatch is gone with it |
| B4 | Blocker | Tracker wallet still funds any reserve | **Fixed (redesign)** — tracker only builds the payload; the owner submits to their own node. `/reserves/submit` deleted |
| B5 | Blocker | Code, config and docs point at pre-fix contracts | **Fixed** — all artifacts regenerated from the post-C3 contracts; consistency spec added |
| B6 | Blocker | Any Write client can poison an issuer's reserve tree | **Fixed** — `/redeem/complete` is Admin-only and rejects `redeemed_amount == 0`; `/redemption/submit` is bound to server-side build state |
| H1 | High | S4 incomplete: request values still signed | **Fixed** — `/redeem` and `/redemption/build` sign stored note state; advisory nature of policy/FIFO documented |
| H2 | High | C3 fail-closed check not used where it matters | **Fixed** — predicates and signing endpoints use `resolve_issuer_reserve`; C3 protocol change implemented |
| H3 | High | Scanner only sees 5 reserve boxes per address | **Fixed** — `byAddress` pagination; unparseable boxes quarantined instead of disabling removal |
| M1 | Medium | Tracker wallet pays for self-redemptions | **Fixed** — per-(issuer, recipient) rate limit on `/redemption/build` |
| M2 | Medium | Emergency path via `/redeem` sends 65 zero bytes | **Fixed** — empty `Coll[Byte]` context var #6, matching `/redemption/build` |
| M3 | Medium | Pre-auth 8 MiB body buffering | **Fixed** — headers validated before body read; body limit 256 KiB; 30 s timeout layer |
| L | Low | Test gaps and smaller issues | **Fixed** — see Low section |

## B1 — contract throws on non-GroupElement R4 (Blocker) — Fixed

The `in.propositionBytes == SELF.propositionBytes &&` guard was restored in
`contract/basis.es` and `contract/basis-token.es`; ErgoScript `&&` short-circuits, so
non-reserve inputs never have R4 read. Regression coverage:
`scala/src/test/scala/basis/contracts/UniqueReserveInputRegisterTypeSpec.scala` co-spends a
`sigmaProp(true)` box with R4 = `Coll[Byte]` and R4 = `Long` — both pass with the guard and
fail with `InvalidType` without it (verified both ways).

## B2 — S3 bypass with totalDebt ≥ 2^63 (Blocker) — Fixed

The contract reads `totalDebt` and `timestamp` with signed `byteArrayToLong`, so a value ≥ 2^63
reads as negative on-chain: `redeemed <= totalDebt - redeemedDebt` can never hold, and the S3
monotonicity rule then blocks any later sane value — the creditor's claim is dead. Fixed in
`crates/basis_store/src/lib.rs`: `add_note`/`update_note` reject `amount_collected` and
`timestamp` above `i64::MAX` (`NoteError::AmountExceedsContractRange`, mapped to HTTP 400).
The same functions no longer treat a storage/decode error from `lookup_note` as "no previous
note" — only `NoteError::NotFound` means that. Callers that cast to `i64`
(`redemption_build.rs`, the CLI signer) now error instead of wrapping.

## B3 + B4 — reserve submission removed (Blockers) — Fixed by redesign

Two findings shared one root cause: the tracker wallet funded reserve creation.

- **B3**: the single-use permit fingerprinted `serde_json::to_vec` of a payload whose
  `registers` is a `HashMap` with random iteration order, so create → JSON → submit round-trips
  usually mismatched (26/30 in the reviewer's probe).
- **B4**: the payload the permit protected still let the caller choose the owner key, amounts,
  and NFT id — including the tracker's own NFT — and the tracker wallet paid.

The chosen fix is the reviewer's preferred redesign, which subsumes both:

- `/reserves/submit` and the whole permit machinery (`reserve_submission.rs`) are **deleted**.
  `/reserves/create` only builds the `/wallet/payment/send`-shaped payload, with the change
  address derived from the **owner's** pubkey.
- The reserve **owner submits the payload to their own node**. CLI: `basis-cli reserve create
  --submit` posts to the `node_url`/`node_api_key` from `cli.toml` (`submit_reserve_to_node`);
  the TUI offers the same flow; the demos (`agent_teams`, `agent_celaut_use` orchestrators)
  post to `BASIS_NODE_URL` directly. The MCP `reserve_create` tool documents that the returned
  payload is not broadcast.
- Defense in depth on create: `nft_id == tracker_nft_id` rejected (400), amounts capped by
  `server.reserve_creation_max_erg_nano` (default 10 ERG) and
  `reserve_creation_max_token_amount` (default 1e9).
- Additional B4 sub-points: `api_key`/`admin_api_key` role split (the shared client key now
  grants Write only when an admin key is configured), and in `none` mode the server installs
  localhost-only CORS plus Host-header validation (DNS-rebinding protection) instead of
  `allow_origin(Any)`.

Deployment note: any automation that called `/reserves/submit` must switch to submitting the
returned `requests` array to the reserve owner's node.

## B5 — stale contract artifacts (Blocker) — Fixed

The review found every shipped P2S/ErgoTree literal pointed at pre-fix contracts. Since then the
contracts changed again (C3), so all artifacts were regenerated from the **current** contracts
(`specs/ergo/compiled_contracts.txt`, compiled with blockVersion 4 / mainnet prefix — note the
new ErgoTree header `1bf90521…`/`1bfb062a…` vs the old `10200414…`/`1b900628…`):

- `crates/basis_store/src/contract_compiler.rs` (both P2S + both ErgoTree hex)
- `config/basis.toml.example`, `docs/CONFIGURATION.md`, `specs/basis_protocol.md`,
  `specs/ergo/token_reserve.md`, `demo/lets_tutorial/config/basis.toml`,
  `demo/agent_coop/config/basis.toml`, all `demo/*/run.sh` and both `reserve_helper.py`
- `transaction_builder.rs` derives the reserve output ErgoTree from the reserve box being spent
  instead of the constant.
- `scala/src/test/scala/basis/contracts/ContractArtifactSpec.scala` recompiles
  `contract/*.es` and asserts the Rust hex literals and the example-config P2S match —
  a contract change now fails the build instead of silently leaving artifacts stale.

**Deployment note (sharper than before):** reserves created under the old scripts remain
**exploitable** on-chain (C1 replay, C2 merge, C6 ERG drain) — they must be refunded or migrated,
not merely dropped from config. New reserves must target the new P2S.

## B6 — poisoning an issuer's reserve tree (Blocker) — Fixed

- `/redeem/complete` is now **Admin-only** and rejects `redeemed_amount == 0`.
- `/redemption/submit` is bound to server-side build state: `/redemption/build` records a
  single-use, 30-minute pending entry keyed by the SHA-256 of the unsigned transaction it
  produced (storing reserve box id, parties, amounts, and the expected new reserve R5 digest).
  The submit request must echo that `unsigned_tx`; the signed transaction's inputs must include
  the tracked reserve box and its reserve-output R5 must match the stored digest, and the tree
  amounts are derived from the stored entry — caller-supplied `redeemed_amount` /
  `new_already_redeemed` are accepted on the wire but ignored (deprecated). Combined with the
  already-present `new_already_redeemed == stored + redeemed_amount` consistency check, a caller
  can no longer move the reserve tree without on-chain evidence.

## H1 — S4 incomplete (High) — Fixed

`/redeem` (`initiate_redemption`) and `/redemption/build` both sign the **stored** note's
`amount_collected`/`timestamp`/`tracker_nft_id`; request `amount`/`timestamp` are ignored (404
for unknown pairs).

**Advisory caveat (documented, not code-fixed):** the signed message binds `totalDebt` but not
the redeemed *amount*, so the acceptance policy and FIFO order remain tracker-side policy rather
than contract invariants — the contract itself bounds every redemption at
`totalDebt - redeemedDebt`. Binding an amount or nonce would be a further message-format change;
recorded here as a deliberate deferral.

## H2 — fail-closed reserve selection (High) — Fixed

- The scanner already compares reserve R6 against the configured tracker NFT (C4 fix), closing
  the dust-grief sub-vector.
- `CollateralizationPredicate` and `NoPendingRefundPredicate` now use
  `ReserveTracker::resolve_issuer_reserve`, and `/tracker/signature`, `/redemption/prepare`,
  `/redemption/build` **and `/redeem`** fail closed (409) when the issuer has more than one
  tracked reserve.
- The C3 protocol change itself (tracker NFT id in the signed message) is implemented across
  contract, Rust, CLI and Scala — see `specs/pr14_triage.md` C3.

## H3 — scanner pagination (High) — Fixed

`ergo_scanner.rs` paginates `/blockchain/box/unspent/byAddress` (limit 1000, looping until a
short page), so reserves no longer vanish once an address holds more than five boxes. A box that
fails to parse is skipped with a warning instead of disabling spent-reserve removal globally.

## Medium

- **M1 (fee drain):** `/redemption/build` is rate-limited per (issuer, recipient) pair
  (`server.redemption_build_max_per_pair_per_hour`, default 60; 429 on exceed). Caller-funded
  fees remain a possible later redesign.
- **M2 (emergency via `/redeem`):** the emergency path now sends an empty `Coll[Byte]` as
  context var #6, matching `/redemption/build` and the contract's `size > 0` branch.
- **M3 (pre-auth buffering):** signature mode now validates headers (signature hex/length,
  pubkey, timestamp window, nonce) before reading the body; the body limit is 256 KiB and a
  30 s timeout layer covers all requests.

## Low

- Zero-padded timestamp replay: new test replays the **exact captured signature** with only the
  header string padded — rejected as replay.
- Replay-cache TTL is now 3× the timestamp tolerance (was 2×), closing the future-dated
  last-millisecond replay window.
- `validate_startup` additionally refuses: the example placeholder API key, and auth over plain
  HTTP on a non-loopback bind unless `server.auth.allow_insecure_http_non_loopback` is set.
  (`localhost`/`::1` bind panics were already fixed via `to_socket_addrs`.)
- `scala/scala-utils/SigUtils.scala` now enforces the contract-compatible challenge check like
  the main `basis/offchain/SigUtils.scala`.
- Demo InsertOnly: `demo/basis/simple/src/BasisDeployer.scala` and `BasisNoteRedeemer`'s
  reserve-tree proof builder now use insert+update flags (the tracker tree stays InsertOnly);
  `scala/scala-utils/Constants.scala` gained an explicit `reserveTreeFlags`/`emptyReservePlasmaMap`
  for reserve trees.
- Rust test coverage added for `DebtDecreaseNotPermitted`, amount carry-forward, the new 400/409
  paths, pagination/quarantine, and the auth/CORS changes.

## Deliberately not changed

- Per-reserve replay within one tracker (C3 remainder) — off-chain mitigation stands, see
  `specs/pr14_triage.md`.
- Chain-derived redemption state (S4 remainder) — same deferral as `pr12_triage.md` finding 3.
- Deep reorg handling past confirmation depth 2 — documented in `docs/CONFIGURATION.md`.
- Policy/FIFO advisory nature (H1 caveat above) — would need another message-format change.
