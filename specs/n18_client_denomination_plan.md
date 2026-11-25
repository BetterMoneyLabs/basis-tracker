# N18: Clients ignore denomination — fix plan

> **Status: implemented** on branch `pr21-merge-master` (2026-11-18), with one
> deliberate deviation in Gap C: rather than deleting the offline
> (`generate-redemption` without `--local-sign`) path, it now refuses to build
> while the tracker's live tree diverges from the committed on-chain tracker
> box (see Gap C below).

Issue #15 follow-up finding N18. Verified against branch `pr21-merge-master` (HEAD `2a5284f`).

## Current state

Server-side handling is already correct: `resolve_denomination`
(`crates/basis_server/src/api.rs:5020`) resolves an empty denomination to the pair's
unique existing note denomination, else ERG, and validates explicit values against the
served set. The CLI note flow (`crates/basis_cli/src/commands/note.rs:445`, uses
`DenomKey::debt_key`), the CLI redemption flow (`commands/transaction.rs`,
`--denomination` threaded through debt key, proofs, and reserve selection), and the MCP
server (`crates/basis_mcp/src/server.rs`) all thread denomination correctly.

Three gaps remain.

## Gap A — acceptance check never carries a denomination

- `TrackerClient::check_acceptance` hard-codes `denomination: String::new()`
  (`crates/basis_cli/src/api.rs:1500`).
- Callers: `basis-cli acceptance check` (`crates/basis_cli/src/commands/acceptance.rs:97`,
  no `--denomination` flag) and the TUI "test policy" screen
  (`crates/basis_app/src/ui.rs:2426`).
- Effect: for a new pair being checked for a token-denominated note the server falls
  back to ERG, so policy predicates (collateralization, reserve checks) run against the
  wrong reserve and `total_debt` is interpreted in the wrong units.

### Fix

1. Add `denomination: Option<&str>` to `TrackerClient::check_acceptance`; serialize it
   into `CheckAcceptanceRequest` (empty keeps today's server fallback).
2. Add `--denomination` to `AcceptanceCommands::Check` and pass it through.
3. TUI: pass the currently selected denomination (from Gap B's selector); until then,
   pass the token id when in token mode.
4. OpenAPI/docs: no endpoint change (field already exists in the request schema) — only
   verify `openapi.yaml` documents `denomination` on `/acceptance/check`.

## Gap B — TUI is structurally ERG-only while displaying token labels

- `NoteInfo` (`crates/basis_app/src/app.rs`) has no `denomination` field;
  `compute_wallet_stats` sums `amount - redeemed` across all notes regardless of
  denomination (mixed units), and the headline coverage ratio uses the ERG reserve entry
  or the first one.
- Note creation (`crates/basis_app/src/ui.rs:~1040-1135`) computes the debt key as
  `blake2b256(issuer ‖ recipient)` with **no token suffix** and posts
  `denomination: String::new()` — token-denominated notes cannot be created; in token
  mode amounts are labeled "token" but produce ERG notes (or fail signature
  verification when the pair resolves to a token denomination).
- Reserve creation (`crates/basis_app/src/ui.rs:1734-1800`) uses only the **first**
  served token via `GET /config/reserve-token` (a transitional endpoint that reports a
  single token, `crates/basis_server/src/api.rs:4671-4682`). The full served set is
  available via `GET /config/reserve-collateral`
  (`TrackerClient::get_reserve_collateral_config`, `ServedDenomination` list) but unused.

### Fix

1. Add `denomination: String` to `NoteInfo`; populate in `refresh_data` from the note
   responses (the API note struct already carries `denomination`).
2. Make `compute_wallet_stats` per-denomination (`BTreeMap<String, WalletStats>` keyed by
   denomination id), each summing only its own notes, with the coverage ratio from the
   matching `reserve_statuses` entry; render per-denomination sections in the dashboard.
   Never aggregate across denominations.
3. Note creation: add a denomination prompt (choices: `erg` + served tokens from
   `/config/reserve-collateral`; default to the pair's existing denomination via server
   resolution); replace the hand-rolled key hash with
   `DenomKey::parse(..)?.debt_key(..)` (basis_core, same as `note.rs:445`); send the
   explicit denomination in `CreateNoteRequest`.
4. Reserve creation: fetch `get_reserve_collateral_config()`, offer ERG plus every served
   token, and build the request with the chosen `token_id`; drop the first-token-only
   flow. `amount_label`/decimals become per-selected-denomination.
5. Redemption (`draw_redeem_note`): thread the stored note's denomination into the
   redemption request (the assisted flow already accepts `--denomination`).

## Gap C — legacy local-signing redemption uses live-tree proofs

`transaction generate-redemption --local-sign` builds the transaction from
`GET /tracker/proof` (var #8 lookup proof and `total_debt`) and `GET /reserve/proof`
(`crates/basis_cli/src/commands/transaction.rs:455` and `:889`). `/tracker/proof` proves
against the **live** local tree — explicitly informational only
(`crates/basis_server/src/api.rs:2473-2477`; issue #15 S6 requires committed state for
redemption). Notes posted since the last on-chain commit make the proof and digest
mismatch the on-chain tracker box, producing an invalid transaction. The assisted flow
(`transaction redeem-assisted` → `POST /redemption/build`) already uses committed state
internally.

### Fix (as implemented)

On inspection, the `--local-sign` path had already been rerouted through
`POST /redemption/build` (committed state) by the N17 work; its only remaining
live-tree dependency was the `total_debt` figure for the issuer-signature
message. Implemented:

1. `execute_local_redemption` (used by both `--local-sign` and
   `redeem-assisted`) no longer calls `/tracker/proof`: the signing message
   uses the note's own `amount_collected`, which is exactly what the stored
   issuer signature commits to. The server build enforces committed state and
   rejects uncommitted notes with a clear error (issue #15 S6).
2. The offline path (`generate-redemption` without `--local-sign`, node-wallet
   JSON) keeps assembling from `/tracker/proof` but now guards on committed
   state: `tracker_box_r5_digest` compares the on-chain tracker box's R5
   register against the proof's `tracker_state_digest` and refuses to build
   while they diverge ("wait for the next tracker box update"). This is the
   plan's Option 3 guard, chosen because the path's whole purpose is offline
   signing, which the assisted flow cannot replace.
3. The TUI redemption (`tracker_assisted_redeem`, normal and emergency) got
   the same two changes: denominated note lookup and the note's own
   `amount_collected` instead of the live `/tracker/proof` total.

Regression coverage: `tracker_box_r5_digest` unit tests
(`crates/basis_cli/src/commands/transaction.rs`).

Alternatives considered:

- **Option 2** — add `?state=committed` to `/tracker/proof` and `/reserve/proof`: more
  server surface duplicating what `/redemption/build` already does; needs OpenAPI and
  consistency-test updates.
- **Option 1 (original recommendation)** — retire the raw-proof path entirely in favor
  of the assisted flow: rejected for the unsigned output mode, whose purpose is offline
  node-wallet signing; the committed-state guard makes it fail-safe instead.

## Tests and docs (as implemented)

- CLI: clap parsing of `acceptance check --denomination`
  (`crates/basis_cli/src/commands/acceptance.rs`); `tracker_box_r5_digest`
  unit tests (`crates/basis_cli/src/commands/transaction.rs`).
- TUI: `stats_never_sum_across_denominations` proves mixed-denomination notes
  are never summed and each denomination gets its own coverage ratio
  (`crates/basis_app/src/app.rs`).
- Docs: `specs/ergo/token_reserve.md` "TUI Wallet Support" rewritten for the
  denomination-aware wallet.
