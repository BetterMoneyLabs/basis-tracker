# Multi-Collateral Reserves Under a Single Tracker

Allow ERG, USE, SigUSD, rsBTC and further tokens to back reserves under **one** tracker
process, one tracker box, one AVL debt tree, one Schnorr signing key and one HTTP API.

Status: implemented. Supersedes the single-denomination assumption
documented in `specs/ergo/token_reserve.md` ("mutually-exclusive setting").

**Breaking change, by decision.** There is no requirement to stay compatible with
pre-change token reserves, token notes, or local stores. The token contract is edited
in place (which necessarily means a new P2S address — §3), pre-change token notes do
not verify under the namespaced key derivation, and the on-disk formats change without
a migration layer. The ERG contract `basis.es` and the ERG signing message are
untouched, so ERG reserves and ERG notes are unaffected. See §12 for the upgrade
procedure.

---

## 1. Summary

Today a tracker is immediately and globally either ERG-backed or single-token-backed
(`ErgoConfig::is_token_reserve_mode`, `crates/basis_server/src/config.rs:500`); the token
may not even be chosen per request — `create_reserve_payload` rejects any `token_id`
other than the configured one. This spec removes that ceiling.

The mechanism making it work is small: a **debt record's tree key is derived from the
reserve's collateral**. One `(issuer, creditor)` pair can then hold several
independent debts — one per denomination — inside the same tracker tree, while the
on-chain redemption path keeps every denomination cryptographically separated.

Everything outside the contract is off-chain work and is the bulk of this project.

## 2. Terminology

**Denomination** — the asset a reserve is denominated in and that its debt is counted
in. One of:

| `denomination_id` | Collateral | Projected onto |
|---|---|---|
| `token:<64-hex>` | the token at that id | `SELF.tokens(1)._2`, `SELF.tokens(1)._1` |
| `erg` | native ERG | `SELF.value` |

Every denomination also carries display metadata (`symbol`, `decimals`) and an
optional policy (`min_collateralization_ratio`), resolved from config. The identity
used for map keys and tree derivation is the cheap, hashable part and is called
`DenomKey` throughout. The `denomination_id` strings above are the canonical form
used in config, the API, and storage; there is exactly one generation of each, so no
namespace qualifier exists.

---

## 3. Why the contract must change

`contract/basis-token.es:262` derives the debt key on-chain:

```scala
val key = blake2b256(ownerKeyBytes ++ receiverBytes)
```

Both inputs are fixed: `ownerKey` is `SELF.R4`, and `receiverBytes` is context var #1,
a 33-byte compressed GroupElement with no room to append a tag. Something must be
added to that derivation to make room for a denomination.

The only other hook a reserve has for naming its tracker is R6, compared byte-for-byte
against the tracker box's `tokens(0)._1` (`:247-250`). R6 carries exactly a 32-byte
token id — it can say *which* tracker, never *which denomination inside* one.

Namespacing the tree **value** instead of the key was considered and rejected: ERG and
token entries would share the key `blake2b256(issuer ‖ receiver)` and overwrite each
other. The key is the namespacing dimension.

And because `selfPreserved` pins `selfOut.propositionBytes == SELF.propositionBytes`,
a reserve can never migrate to a new contract. Editing the script therefore *means* a
new P2S address. Since compatibility with existing token reserves is explicitly not
required, `basis-token.es` is edited in place and its pre-change address is abandoned:
reserve boxes at the old address remain spendable on-chain under the old script, but
this tracker no longer builds redemptions for them.

## 4. Design

### 4.1 The key split

```scala
// debt record: domain-separated per denomination
val debtKey    = blake2b256(ownerKeyBytes ++ receiverBytes ++ tokenId1)

// reserve redemption history: current derivation, kept on purpose
val reserveKey = blake2b256(ownerKeyBytes ++ receiverBytes)
```

No tag byte on the token id: both public keys are fixed 33 bytes, so the token id
always starts at a fixed offset, and the ERG input (66 bytes) cannot collide with a
token input (98 bytes) under blake2b256. A future non-token collateral kind would
need its own contract logic anyway, at which point a tag can be introduced without
touching these derivations.

`reserveKey` keeps the current derivation deliberately. The script today reuses one
`key` for both trees (`:271`, `:282`, `:288`, `:377`); reusing the namespaced key for
the reserve tree would zero every `redeemedDebt` counter and reopen the
`redeemed <= debtDelta` cap.

Resulting tree shapes:

```
tracker box R5 (one box, one tree):
  H(A‖B)                -> 100    ERG     (basis.es, unsuffixed)
  H(A‖B‖USE)            -> 250    USE     (basis-token.es)
  H(A‖B‖SigUSD)         -> 2500   SigUSD  (basis-token.es)

reserve box R5 (per active reserve, unchanged shape):
  H(A‖B) -> (timestamp(8) ‖ cumulativeRedeemed(8))   16 bytes
```

Three debts between the same pair, one box, one commit. The ERG key stays unsuffixed
and cannot collide with a token key: `H(o‖r)` vs `H(o‖r‖t)` differ by input,
and blake2b256 collision resistance covers the rest.

### 4.2 Sig message: unchanged in shape

The C3 change (`specs/pr14_triage.md`) already fixed the message at 80 bytes:

```
key(32) || totalDebt(8 BE) || timestamp(8 BE) || trackerNftId(32)
```

The token contract keeps that exact form. `key` is now `debtKey`, still 32 bytes, so
the message is **still 80 bytes** and the vector / signer / verifier plumbing is
untouched in shape — only *how* `key` is computed changes (`signing_message` in
`crates/basis_core/src/types.rs:25` gains a denomination parameter). The ERG message
is byte-identical to today.

Note that `debtKey` is not tracker-bound by itself, and does not need to be: the
message is, via the trailing `trackerNftId`.

### 4.3 ERG contract unchanged

`basis.es` keeps `key = blake2b256(issuer ‖ receiver)` — unsuffixed — permanently.
There is no token to mix in, and the box's ERG value is already the collateral.
Existing ERG reserves, notes, signatures, Schnorr vectors and the `BasisSpec` Scala
suite are untouched; zero work on the ERG path.

### 4.4 No token pinning — decision record

Pinning (hard-coding one token id as a compile-time constant per compiled instance)
was considered and **rejected**:

- The P2S address already carries no trust signal. Anyone can create a box at the
  "SigUSD address" holding 1 unit of SigUSD; the address says nothing about
  solvency or ownership. What makes a reserve real lives elsewhere: presence in
  `ReserveStorage`, an on-chain `R5` digest matching the tracker's local tree, and
  `R6` naming the right tracker NFT.
- Against a malicious tracker, one pinned property constrains very little.
- Operator-typo failure mode gets *worse* under pinning: a mistyped token id is a
  permanently wrong identity, and reserve boxes never move.
- Cost scales with the open-ended token list ("rsBTC and so on") — one permanent
  address per token, forever.
- Where the pin would earn its keep, it is a **new feature**: the contract enforces no
  collateralization ratio today (`properlyRedeemed = (redeemed > 0) &&
  (redeemed <= debtDelta) && trackerSigValid`, `:333` has no ratio term), so
  per-token ratios would be a later contract revision with per-instance constants.
  Self-describing keys carry forward unchanged if that happens.

So: **one token contract, one compile, one address, serving every token.** Hygiene
moves off-chain (§8, §10), where the rest of the protocol's trust already lives.

Deliberate consequence: adding a new token is a **config line**, not a deploy.

## 5. Contract diff

The complete change to `contract/basis-token.es` is one derivation split into two
plus four variable re-points. Nothing else moves.

```diff
-{:262} val key = blake2b256(ownerKeyBytes ++ receiverBytes)
+       // split the single key in two: debtKey is derived from the denomination
+       val debtKey    = blake2b256(ownerKeyBytes ++ receiverBytes ++ tokenId1)
+       val reserveKey = blake2b256(ownerKeyBytes ++ receiverBytes)
```

| line | use | pointed at |
|---|---|---|
| `:271` | `trackerTree.get(key, trackerLookupProof)` — debt lookup | `debtKey` |
| `:282` | `SELF.R5.get(key, lookupProofOpt)` — `storedTimestamp` | `reserveKey` |
| `:288` | `SELF.R5.get(key, lookupProofOpt)` — `redeemedDebt` | `reserveKey` |
| `:312` | `message = key ++ totalDebt ++ timestamp ++ trackerNftId` | `debtKey` |
| `:377` | `redeemedKeyVal = (key, treeValue)` — tree update | `reserveKey` |

Unchanged, and explicitly so:

- actions **#1 top-up, #2 initiate refund, #3 complete refund** — they never
  reference `key`
- context vars #0–#8, register layout R4–R7
- tracker tree values (8 bytes), reserve tree values (16 bytes)
- `tokenIdsPreserved`, `selfPreserved`, `ergPreserved`, `uniqueReserveInput`,
  the C1 insert-vs-update branch and the C5 `>=` timestamp rule
- **`contract/basis.es` — not touched at all**

The edit is in place (not a new `_v2` file): compatibility with pre-change token
reserves is not required (§3), and a single token contract keeps the deployed surface
to two addresses total.

## 6. Invariants

1. **`debtKey` namespacing.** A debt record in denomination *D* lives at
   `H(issuer ‖ creditor ‖ tokenId)` (token) or `H(issuer ‖ creditor)` (ERG).
   Two denominations never share an entry.
2. **One active reserve per `(owner, denomination)`.** Enforced at reserve creation
   and at every consumer. This extends an existing, deliberate hard rule:
   `ReserveTracker::resolve_issuer_reserve` (`crates/basis_store/src/reserve_tracker.rs:163`)
   already fails closed with `MultipleReservesForOwner` for a multi-reserve issuer,
   because per-reserve replay is accepted as an off-chain mitigation (C3). It becomes
   `resolve_issuer_reserve(owner, DenomKey)` with `MultipleReservesForDenomination`,
   so an owner holding an ERG reserve and a SigUSD reserve is no longer locked out.
   The acceptance-predicate consumers (`crates/basis_server/src/acceptance/mod.rs:169`,
   `:230`) keep their fail-closed semantics **per denomination**: a missing liability
   snapshot for `(owner, denom)` fails the collateralization check for that
   denomination exactly as a missing snapshot fails it globally today, and
   "no reserve for this denomination" must never be read as "no debt".
3. **A note redeems only against its own denomination.** Structural: a SigUSD
   reserve's collateral is SigUSD, and its redemption proof is drawn from the
   SigUSD-namespaced debt entry.
4. **Reserve boxes stay single-denomination.** A box's collateral is one token at
   `tokens(1)` or its ERG value; the contract never mixes them.

Residual risk carried by #2 is documented in §14: it is off-chain, not cryptographic.

## 7. Off-chain data model

### 7.1 `DenomKey` / `Denomination`

```rust
// crates/basis_store/src/denomination.rs (or basis_core)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DenomKey {
    Erg,
    Token([u8; 32]),
}

impl DenomKey {
    /// Canonical id: "erg" or "token:<64-hex>"
    pub fn denomination_id(&self) -> String;
    /// Debt-tree key suffix: [] for Erg, token_id for Token
    pub fn debt_key_suffix(&self) -> Vec<u8>;
    /// Debt-tree key: blake2b256(owner ‖ receiver ‖ suffix)
    pub fn debt_key(&self, owner: &PubKey, receiver: &PubKey) -> [u8; 32];
}

/// DenomKey + display metadata + policy, resolved from config.
#[derive(Clone)]
pub struct Denomination { pub key: DenomKey, pub symbol: String,
                          pub decimals: u8,
                          pub min_collateralization_ratio: Option<f64> }
```

Display metadata is kept **out** of `DenomKey` so it can stay `Copy + Hash + Eq` for
map keys. `Denomination` is resolved from configuration and looked up by `DenomKey`.

### 7.2 `crates/basis_store/src/lib.rs`

| Change | Notes |
|---|---|
| `NoteKey::from_assets(issuer, receiver, denom)` | tree key; `NoteKey::from_keys(issuer, receiver)` **kept** as shorthand for `from_assets(.., DenomKey::Erg)` |
| `reserve_avl_states: HashMap<PubKey, BasisAvlTree>` → `HashMap<(PubKey, DenomKey), BasisAvlTree>` | `reserve_tree_mut`, `get_already_redeemed`, `update_already_redeemed`, `generate_reserve_insert_proof`, `reserve_state_digest` all gain a denomination parameter |
| `IouNote.denomination: DenomKey` | new field in the fixed-width record — see §7.3 |
| `rebuild_avl_tree` | derives keys with `NoteKey::from_assets(issuer, recipient, note.denomination)` from the stored note; the timestamp-ordered replay is otherwise unchanged |
| `signing_message(.., denom)` | debt key now denomination-derived; message length stays 80 |
| `resolve_issuer_reserve(owner, denom)` | §6 invariant 2 |
| `TrackerCommand` audit | every pair-keyed variant gains a denomination: `GetNoteByIssuerAndRecipient`, `InitiateRedemption`, `CompleteRedemption`, `GenerateProof`, `GetTrackerLookupProof`, `GetCommittedTrackerLookupProof`, `GetReserveLookupProof`, `GetReserveInsertProof`, `GetReserveStateDigest`, `GetConfirmation` (`crates/basis_server/src/lib.rs:64`). `AddNote` carries it inside the note. Key-keyed variants (`MarkNotesPending`, `ConfirmPendingNotes`, `RevertPendingNotes`) are untouched — the 32-byte note key already embeds the denomination |
| `confirmed_digest` | **unchanged.** One tracker box R5 hosts all denominations, so the committed digest stays a single value and the S6 committed-digest rule applies unchanged |

### 7.3 `crates/basis_store/src/persistence.rs` — formats change, no migration layer

The three stores have different serialization regimes; per the breaking-change
decision, formats change incompatibly and upgrade means wiping local state (§12):

- **Notes partition** — hand-rolled fixed-width binary, currently
  `issuer(33) ‖ collected(8) ‖ redeemed(8) ‖ timestamp(8) ‖ signature(65) ‖
  recipient(33) ‖ tracker_nft_id(32)` = 187 bytes, with a hard length check at decode
  (`persistence.rs:307-313`, `:340`). The denomination is appended as a fixed-width
  suffix (`kind(1) ‖ token_id(32)` = 33 bytes, `kind` 0 = Erg, 1 = Token) → 220-byte
  records. The length check is updated to the new size; old records are **not**
  decodable, which is accepted.
- **Reserve AVL journal** — key `nanos(16) ‖ issuer(33) ‖ note_key(32)`, value
  `timestamp(8) ‖ already_redeemed(8)` = 16 bytes (`persistence.rs:789-795`); the
  iterator **silently skips** any entry whose key or value length mismatches
  (`persistence.rs:816`). The value gains the same 33-byte denomination suffix
  (49 bytes total) and the length check moves to the new size. Keys are untouched, so
  insertion order — the S6 guarantee — is preserved.
- **`ReserveInfo` and `NoteConfirmation`** — JSON via serde
  (`persistence.rs:394`, `:862`). `ReserveInfo` gains `denomination_id`, `symbol`,
  `decimals`, `contract_address` (per-box, §8.2), `is_active`.

The `rebuild_reserve_avl_trees` rebuilder keys by `(issuer, DenomKey)`, replaying the
journal in its recorded order — the ordering guarantee shipped with S6.

### 7.4 AVL determinism — the make-or-break requirement

`generate_insert_proof` replays the insertion-order journal and asserts the rebuilt
digest equals the committed one (S6, `crates/basis_trees/src/avl_tree.rs`). Both must
keep holding as keys multiply across denominations:

- new denominations must be inserted through the **journal**, never the in-memory
  cache;
- **acceptance criterion:** after any restart, the rebuilt per-`(issuer, DenomKey)`
  tree digests equal the digests the live manager had before shutdown, and the live
  digest of each denomination equals its active reserve's on-chain `R5`. An explicit
  restart-rebuild equality test is added for that.

Silent failure here is not the tree being wrong — it is every reserve desyncing and
redemption skipping everything forever (`redemption_build.rs:758` warn-and-skip).

## 8. Scanner and configuration

### 8.1 Config

Two contract addresses (the scanner already has exactly two slots) plus a token
metadata/policy table:

```toml
[ergo]
basis_reserve_contract_p2s = "3PQn…"        # ERG reserves (basis.es, unchanged)
basis_token_reserve_contract_p2s = "<new>"  # token reserves (basis-token.es, edited)
# REMOVED (breaking config change): reserve_token_id, reserve_token_decimals,
# is_token_reserve_mode semantics

[ergo.reserve_tokens]        # the tokens this tracker serves: metadata + policy
"a55b8735…" = { symbol = "USE",    decimals = 3, min_collateralization_ratio = 1.10 }
"e853…"     = { symbol = "SigUSD", decimals = 2, min_collateralization_ratio = 1.10 }
"…"         = { symbol = "rsBTC",  decimals = 8, min_collateralization_ratio = 1.30 }
```

- a token denomination is **served** iff its id appears in `reserve_tokens`; reserve
  creation and note issuance for an unserved token are rejected with `400`
- the scanner still records reserves backed by unserved tokens (they are real boxes at
  our contract address) but marks them unserved; no notes can exist against them, so
  they can never be redeemed through this tracker
- config load rejects duplicate token ids and malformed entries; ERG is implicit and
  always served

### 8.2 Scanner

`NodeConfig`'s two address slots (`reserve_contract_p2s`,
`token_reserve_contract_p2s`) stay as they are — the token slot now serves every
token, so no `Vec` of instances is needed.

`parse_reserve_box(box, kind)` derives collateral **per box** rather than by comparing
to one configured token:

- ERG slot → collateral = box ERG value, `DenomKey::Erg`
- token slot → read `tokens[1]`; collateral = its amount, denomination =
  `DenomKey::Token(tokens[1].id)`, served iff the id is in `reserve_tokens`

It also sets `ReserveInfo.contract_address` from **which slot the box was found at**,
fixing today's behaviour of stamping one global value.

The C4 first-seen validation is unchanged: R4 must be a valid secp256k1 point, R5 must
equal the empty-tree constant with flags `0x03`, R6 must equal this tracker's own NFT,
R7 must be zero. Because the edited token contract keeps the reserve-tree key
derivation unchanged (§4.1), a token reserve's initial R5 is the **same** empty-tree
constant as before — no new constant and no validation divergence.

## 9. Redemption pipeline

`crates/basis_server/src/redemption_build.rs` and the `TrackerCommand` surface.

- `GetReserveStateDigest { issuer_pubkey, denomination, response_tx }` — the R5 match
  at `:758` becomes per-denomination
- candidate filtering: only reserves whose `DenomKey` equals the note's
- **note identity is now a triple** `(issuer, recipient, denomination)`; every
  redemption entry point resolves the stored note by all three, and a request that
  omits the denomination defaults to the pair's *existing* note's denomination when
  exactly one exists, else `erg` — never silently creating a second edge under the
  wrong denomination
- `/redeem/complete` (B6: Admin-only, rejects `redeemed_amount == 0`) carries the
  denomination so `update_already_redeemed` lands in the right `(issuer, DenomKey)`
  tree
- the insert proof is generated from the `(issuer, DenomKey)` tree, whose on-chain
  keys are `blake2b256(owner ‖ receiver)` in both contracts — so reserve-tree
  content keys are unchanged and only *which* tree is chosen is namespaced
- emergency redemption unchanged in shape; the message stays 80 bytes and
  `trackerSigValid` behaves exactly as today
- amounts continue to respect the B2 `i64::MAX` caps (`AmountExceedsContractRange`);
  the existing `reserve_creation_max_token_amount` covers the token equivalent
- `/redeem` and `/redemption/build` still sign **stored** note state (H1), now
  resolved per denomination

## 10. Cross-denomination exchange (triage)

Same-denomination redemption is strict and needs no price oracle. The escape valve
for moving liability between denominations is off-chain novation — the mechanism
already specified in `specs/cross_tracker_debt_transfer.md` (Option 1), with
denomination as the discriminator where the tracker NFT id is.

New `POST /notes/exchange`:

```json
{
  "issuer_pubkey": "…", "recipient_pubkey": "…",
  "from_denomination": "token:e853…", "from_amount": 2500,
  "to_denomination": "erg",           "to_amount": 100,
  "timestamp": 1769400000000,
  "issuer_signature": "…", "recipient_signature": "…"
}
```

Semantics:

- **Both signatures required.** Decreasing an edge must never depend on the debtor's
  signature alone — this is the S3 rule, already fixed, and triage reuses it.
- Both signatures cover the **issuer's** edge decrease in `from_denomination` and the
  **recipient's** consent to it, plus the edge increase in `to_denomination`, on the
  80-byte message for each denomination's `debtKey`.
- Amounts respect the B2 caps in both denominations.
- **One AVL commit.** Both edges live in the single tracker tree → one `R5`
  publication, one atomic state change. This is the property that made
  one-tracker-box preferable over one-tracker-per-denomination identity: two trees
  would mean two commits and a window in which the tracker attested to an
  uncommitted state, which is where over-issuance happens.
- **Acceptance policy re-evaluated in the target denomination** using
  `load_issuer_gross_debt`'s per-`DenomKey` form; moving liability into a
  denomination with no (or an under-collateralized) reserve is rejected.
- The tracker takes no view of *what the rate means* — it is bilaterally agreed. No
  oracle, no quoting, no unit mixing in collateralization ratios.
- Triage requires at least two served denominations; it is rejected with `400` when
  the tracker serves only one.

## 11. API, CLI, TUI, MCP

| Surface | Change |
|---|---|
| `POST /reserves/create` | `token_id` optional and free-form; picks the contract (ERG vs token slot) and denomination; `400` if the token is not served, `400` if an active reserve already exists for `(owner, denomination)` |
| `CreateReserveResponse` | `denomination`, `denomination_id`, `symbol`, `decimals`, `contract_address`. The response is a payload for the **owner's own node** (B4 redesign — `/reserves/submit` is gone) |
| `CreateNoteRequest` | `+ denomination` (`"erg"` default); `tracker_nft_id` already required post-C3 with `400`/`409` handling. The server rejects a denomination it does not serve (`400`), mirroring the `tracker_nft_id` rule, and verifies the issuer signature over the denomination-derived `signing_message` — a mismatched denomination simply fails signature verification |
| **all note-addressing endpoints** | note identity is `(issuer, recipient, denomination)`, so `denomination` is added (default per §9) to `POST /redeem`, `POST /redeem/complete`, `POST /redemption/prepare`, `POST /redemption/build`, `POST /notes/state`, `POST /tracker/signature`, `POST /acceptance/check`, and as a query parameter to `GET /notes/issuer/{i}/recipient/{r}`, `GET /proof/redemption`, `GET /tracker/proof`, `GET /reserve/proof`. `GET /notes/issuer/{pubkey}` and `GET /notes/recipient/{pubkey}` return notes of all denominations, each labelled |
| `Note`/`KeyStatusResponse` | `+ denomination`, `symbol`, `decimals`, `contract_address`, `is_active` |
| `GET /key-status/{pubkey}` | per-denomination breakdown (`{denomination_id, symbol, total_debt, collateral, collateralization_ratio, reserve_box_id, has_pending_refund}`). The current handler (`crates/basis_server/src/api.rs:1535-1596`) sums every note and divides by the *first* matching reserve's collateral — unit-mixing the moment two denominations exist. The cross-denomination aggregate reports only unit-free fields (`note_count`, `last_updated`); there is no mixed-unit aggregate ratio |
| `GET /reserves/issuer/{pubkey}` | per-denomination reserves, each with `is_active` |
| `GET /config/reserve-collateral` | replaces `GET /config/reserve-token`; the two contract addresses + the `reserve_tokens` table |
| `GET /config/reserve-contract-p2s` | returns both addresses (+ per-address kind); the ERG scalar kept as a deprecated field for one release |
| `POST /notes/exchange` | §10 |
| `SerializableReserveInfo` | the new fields, plus `reserve_token_id` — already listed in `openapi.yaml:2150`, `:2438` |
| CLI | `reserve create --token-id <any served>`, `reserve status` as a per-denomination table, `--denomination` on note creation/redemption, `notes exchange` |
| TUI | the single `reserve_token_config` cache becomes a `DenomKey → Denomination` registry; `My Reserves` grouped by denomination; symbols/decimals per token |
| MCP | `reserve_create` / `reserve_status` / note tools gain the denomination parameter and report the note's denomination in results |

Per `AGENTS.md`, any route change is accompanied by `openapi.yaml`, the
`openapi_consistency` test, `docs/OPENAPI.md`, CLI client methods and MCP tools.

## 12. Breaking changes and upgrade

There is no data migration. Upgrading an existing tracker:

1. **Token reserves at the old token address** are no longer served: the tracker
   builds no redemptions for them. Owners refund or settle them (with the pre-upgrade
   software) before the cutover. ERG reserves are unaffected — `basis.es` is
   untouched.
2. **Config** moves to §8.1: `reserve_token_id` / `reserve_token_decimals` are
   removed, the token contract address is updated, and the served-token set moves to
   `[ergo.reserve_tokens]`. The loader rejects the removed fields with a descriptive
   error rather than silently ignoring them.
3. **Local stores are wiped** (`scripts/clean_database.sh`): note records and the
   reserve journal change width incompatibly (§7.3). Creditors then re-submit their
   notes. ERG note signatures remain valid (the ERG message is byte-identical), so
   ERG debt is restored by resubmission; pre-change token notes are void, since their
   signatures cover the unsuffixed key.
4. New token reserves are created at the new token address; debt and redemption
   proceed per denomination from there.

## 13. Phasing and acceptance criteria

| # | Phase | Acceptance criteria |
|---|---|---|
| 0 | this spec, plus a §8.1 config example in `config/basis.toml.example` | reviewed |
| 1 | `DenomKey`/`Denomination`, `NoteKey::from_assets`, `reserve_avl_states` re-key, new note/journal record widths, `resolve_issuer_reserve(owner, denom)` | full suite updated and green; restart-rebuild digest equality test (§7.4) passes |
| 2 | §8.1 config, scanner per-box derivation, per-box `contract_address`, served-token rule | scanner test: mixed ERG + two token kinds parse correctly with their own denominations; unserved-token creation rejected with `400` |
| 3 | `contract/basis-token.es` edit + `BasisTokenSpec` update + Schnorr vectors + new compiled address | `BasisTokenSpec` passes with the namespaced `debtKey`; token vectors regenerated; ERG vectors and `BasisSpec` unchanged |
| 4 | redemption pipeline denomination-awareness + §6 invariant 2 | integration test: two denominations for one owner, each redeeming against its own reserve; a signature for one token fails verification against another token's reserve |
| 5 | `POST /notes/exchange` + acceptance re-evaluation | creditor consent required; both edges in one commit; post-exchange collateralization enforced |
| 6 | API / CLI / TUI / MCP + `openapi.yaml` / `docs/OPENAPI.md` | `cargo test -p basis_server --test openapi_consistency`; `python3 -c "import yaml; yaml.safe_load(open('openapi.yaml'))"` |
| 7 | docs: `token_reserve.md` updated to supersede, `basis_contract_rust_notes.md` key/value shapes updated, per-`AGENTS.md` doc checklist | — |

Phase 3 can be built and tested entirely inside Scala before any Rust work depending
on it; phases 1–2 are the risky, mechanical core and are worth landing first.

## 14. Risks

| Risk | Mitigation in this spec |
|---|---|
| AVL digest drift after restart | §7.4 acceptance criterion + the S6 journal-replay machinery already in `generate_insert_proof`; digests equal or error, never silently |
| Journal entries silently skipped on restart | the journal iterator skips length mismatches (`persistence.rs:816`); the width change must move that check to the new size, and the §7.4 restart test covers it |
| Insertion-order loss as keys multiply | new denominations must flow through the journal, never the cache (§7.4) |
| Unit-mixing collateralization | `get_key_status` / `load_issuer_gross_debt` / acceptance predicates all re-keyed per `DenomKey`; no mixed-unit aggregate ratio (§11) |
| Ambiguous note identity | every pair-keyed endpoint and `TrackerCommand` variant gains a denomination (§9, §11); an omitted denomination resolves to the pair's existing note when unambiguous, else `erg` — never silently creating a second edge |
| Per-reserve replay | **not** fixed cryptographically. Invariant #2 (one active reserve per `(owner, denomination)`) extends the existing `MultipleReservesForOwner` fail-closed stance, but it is an off-chain mitigation as C3 ruled. Cheapest future option if revisited: an epoch nonce rather than the box id, since the box id churns on every redemption. **Additional control worth folding in:** zero the affected debt edges at reserve-completion refund (`action #3`), since an owner who withdrew all collateral should not retain a redeemable IOU against the tracker |
| Upgrade abandons in-flight token debt | accepted by decision (§12); token notes are re-issuable, and the cutover procedure settles or refunds old token reserves first |
| Triage leaves tracker-attested state uncommitted | one tracker box, one tree → one commit (§10); unchanged `confirmed_digest` discipline (S6) |
| Token amounts (B2 analog) | creation capped by `reserve_creation_max_token_amount`; note-path caps already `i64::MAX`-bounded |
