# Agent Integration Spec — Operating the Basis Wallet Programmatically

This is the canonical reference for AI agents (Kimi Code CLI, Claude, other LLM tooling)
that operate the Basis wallet: accounts, IOU notes, reserves, and the acceptance policy —
without driving the interactive TUI.

Two interfaces are provided, sharing one typed core (`basis_cli_lib::commands::*`):

| Interface | Binary | Best for |
|-----------|--------|----------|
| MCP server | `basis-mcp` (crate `basis_mcp`) | LLM agents with MCP client support — typed, self-describing tools |
| JSON CLI | `basis_cli --json` | Any shell-capable agent — universal fallback |

For per-command CLI examples see `docs/AGENT_INTERFACE.md`. For humans, the same
operations are available in the TUI (`basis-ui`) and the interactive REPL
(`basis_cli interactive`).

## Setup

```bash
cargo build --release -p basis_mcp -p basis_cli
```

MCP client configuration (Kimi CLI / Claude Desktop style):

```json
{
  "mcpServers": {
    "basis": { "command": "/path/to/basis-tracker/target/release/basis-mcp" }
  }
}
```

The server speaks MCP over stdio (rmcp 0.17). `BASIS_SERVER_URL` (or `--server-url`)
overrides the tracker URL; default comes from `~/.basis/cli.toml`
(`http://127.0.0.1:3048`). First call after connecting: `server_status` — a connection
error here means the tracker server is not running (start it with
`./scripts/run_server.sh`).

## Concepts agents must know

- **Amounts are in nanoERG**: 1 ERG = 1_000_000_000 nanoERG. All `amount` parameters
  and `total_debt`/`collateral` fields are nanoERG integers.
- **Public keys** are 33-byte compressed secp256k1, hex-encoded (66 chars).
- **Notes are cumulative-debt IOUs**: `amount_collected` is the total debt the issuer
  has ever acknowledged to the recipient; `amount_redeemed` is how much was paid back
  on-chain; `outstanding = amount_collected - amount_redeemed`.
- **Signing happens in-process** with keys stored locally. No interface ever returns
  private key material (the one exception is the human-oriented `basis_cli account
  export` command — agents must not use it; see Security rules).
- **Shared state**: accounts live in `~/.basis/cli.toml`, the acceptance policy in
  `~/.basis/ui.toml`. The CLI, TUI, and MCP server share both files — changes made
  through one interface are visible in the others.

## MCP tool reference (`basis-mcp`)

Tool errors are returned as MCP `isError` content carrying the full error chain
(e.g. `... Connection refused (os error 111)`) — never as process exits. Success
content is pretty-printed JSON.

### Read-only tools (`readOnlyHint: true`)

| Tool | Parameters | Returns |
|------|-----------|---------|
| `server_status` | `server_url?` (override tracker URL) | `{healthy: bool, recent_events: [{timestamp, summary, height?}]}` |
| `account_list` | — | `[{name, pubkey_hex, created_at, current}]` — never key material |
| `account_current` | — | `{name, pubkey_hex, created_at}` or `null` |
| `note_list` | `pubkey?` (default: current account), `direction?`: `"issued"` \| `"received"` (default `"received"`) | `[{issuer_pubkey, recipient_pubkey, amount, redeemed, outstanding, timestamp}]` |
| `note_get` | `issuer`, `recipient` | note object `{issuer_pubkey, recipient_pubkey, amount_collected, amount_redeemed, timestamp, signature, confirmation?}` or `null` |
| `reserve_status` | `pubkey?` (default: current account) | `{total_debt, collateral, collateralization_ratio, note_count, last_updated, issuer_pubkey, has_pending_refund}` |
| `policy_get` | — | local `AcceptanceConfig` from `~/.basis/ui.toml` |

### Write tools

| Tool | Parameters | Annotations | Returns |
|------|-----------|-------------|---------|
| `account_create` | `name` | — | `{name, pubkey_hex, created_at}`; first account becomes current |
| `account_switch` | `name` | — | `{switched: name}` |
| `account_import` | `name`, `private_key_hex` (64 hex chars) | — | `{name, pubkey_hex, ...}`; the key is persisted, never returned |
| `note_create` | `recipient`, `amount` (nanoERG) | — | `{issuer_pubkey, recipient_pubkey, amount, timestamp, signature, reserve_status_before, reserve_status_after}`; signed with the **current** account |
| `note_redeem` | `issuer`, `amount` (nanoERG) | `destructiveHint` | redemption result incl. `tx_id` on broadcast; local-signing path (current account = recipient) |
| `reserve_create` | `nft_id` (64 hex), `amount` (nanoERG), `token_amount` (optional raw token units), `token_id` (optional 64 hex) | — | reserve-creation payload; pass `token_amount` and `token_id` to create a token-backed reserve such as USE stablecoin |
| `policy_set` | `policy` (JSON object matching `AcceptanceConfig`) | `destructiveHint` | `{saved, uploaded, policy_hash, uploaded_at}` |

`policy_set` saves to `~/.basis/ui.toml` **and** uploads to the tracker signed with the
current account. If the upload fails, the local save still stands and the tool returns
an `isError` result reading `policy saved locally but upload failed: ...` — treat this
as partial success, not a clean failure.

`AcceptanceConfig` shape (see `crates/basis_core/src/acceptance.rs`):

```json
{
  "default": "reject",
  "root": "require_full_collateral",
  "predicates": [
    {"type": "collateralization", "name": "require_full_collateral", "min_ratio": 1.0}
  ]
}
```

## CLI fallback (`basis_cli --json`)

Every command accepts a global `--json` flag (any position) and prints a single JSON
document to stdout; diagnostics go to stderr. Exit-code contract:

- `0` — success (stdout parses as JSON)
- `1` — error; stderr carries `{"error": "..."}`
- `2` — tracker server unreachable; stderr carries `{"error": "... Connection refused ..."}`

Representative commands (full list in `docs/AGENT_INTERFACE.md`):

```bash
basis_cli --json status
basis_cli --json account info
basis_cli --json account create alice
basis_cli --json note list --recipient
basis_cli --json note create --recipient <66-hex> --amount 1000000000
basis_cli --json note redeem --issuer <66-hex> --amount 1000000000
basis_cli --json reserve status
basis_cli --json acceptance upload --policy-file policy.toml
basis_cli --json acceptance check --issuer <66-hex> --recipient <66-hex> --total-debt 1000000000
```

## Standard workflows

### Bootstrap a wallet
1. `account_current` → if `null`, `account_create {name}` (first account becomes current).
2. Report `pubkey_hex` to the user — this is what others need to pay the wallet.

### Pay someone with an IOU note
1. Confirm the recipient pubkey and amount (nanoERG) with the user.
2. `reserve_status` (current account) — check `collateralization_ratio` stays healthy
   after the new debt; the tracker will reject notes violating the recipient's policy.
3. `note_create {recipient, amount}` — signed with the current account.
4. Confirm with `note_get {issuer: <current pubkey>, recipient}`.

### Check receipts and redeem
1. `note_list {direction: "received"}` → pick notes with `outstanding > 0`.
2. `note_get {issuer, recipient: <current pubkey>}` for details.
3. Confirm the amount with the user, then `note_redeem {issuer, amount}` —
   builds, signs, and broadcasts the on-chain redemption (`destructiveHint`).
4. `reserve_status {pubkey: issuer}` or `note_get` again to confirm.

### Create a reserve
1. `reserve_create {nft_id, amount}` (owner = current account) — returns the unsigned
   payload (requests, fee, change address). Submitting it on-chain requires an Ergo
   node/wallet; see `docs/BUILD_AND_CREATE_RESERVE.md`.

### Manage the acceptance policy
1. `policy_get` → show the user the current policy.
2. Apply the requested change → `policy_set {policy}` with the full new config.
3. On the partial-failure message, inform the user the policy is local-only until the
   tracker is reachable.

## Example: multi-agent service co-op

A runnable pure-credit example is provided in `demo/agent_coop/`. It spawns three
isolated `basis-mcp` processes (Alice, Bob, Charlie), has each publish a whitelist
acceptance policy, then executes a round of service payments via `note_create`.
The orchestrator prints a balance sheet and credit-utilization bars.

```bash
./demo/agent_coop/run.sh
```

No reserves, collateral, or redemption are used — it is the simplest end-to-end
MCP workflow and a starting point for LLM-driven agents. See
`demo/agent_coop/README.md` for the full story and expected output.

## Example: competing agent teams with backed judge prize

A runnable reserve-backed example is provided in `demo/agent_teams/`. It spawns
seven isolated `basis-mcp` processes:

- Team Alpha (`adam` manager, `ava` compute, `alex` storage)
- Team Beta (`bella` manager, `bryn` compute, `ben` storage)
- A human judge (`judy`)

Managers decompose the judge's task into subtasks and hire workers; cross-team
credit is accepted only when the issuer's reserve covers ≥ 50% of liabilities;
the judge's prize is 100% backed by her reserve and is redeemed on-chain.

```bash
export TRACKER_NFT_ID=...
export JUDGE_RESERVE_NFT_ID=...
export ADAM_RESERVE_NFT_ID=...
export BELLA_RESERVE_NFT_ID=...
./demo/agent_teams/run.sh
```

See `demo/agent_teams/README.md` for prerequisites, NFT setup, and expected output.

A human-driven LETS demo using the `basis-ui` TUI wallet is provided in
`demo/lets_tutorial/`. Each member runs their own isolated wallet, whitelists the
other members, and issues cumulative IOU notes. The TUI stats screen shows assets,
liabilities, and net position in real time.

```bash
./demo/lets_tutorial/run_lets_tutorial.sh --tmux
```

See `demo/lets_tutorial/README.md` for the trading scenario and expected balances,
and `specs/tui_wallet_lets.md` for the design specification.

## Security & safety rules for agents

- **Never request, store, or echo private keys.** Use `account_import` only when the
  user explicitly provides a key for import. There is deliberately no key-export MCP
  tool; do not work around this via `basis_cli account export`.
- **Confirm before destructive/irreversible calls**: `note_redeem` (broadcasts an
  on-chain transaction), `policy_set` (overwrites the published policy), and
  `note_create` (creates real debt). State amount (in ERG and nanoERG) and
  counterparty pubkey, and get the user's go-ahead.
- **Validate pubkeys** (66 hex chars) before passing them; a malformed key is a user
  error, not something to retry blindly.
- **Amounts are nanoERG** — double-check unit conversion when the user says "ERG".
- **Error handling**: MCP tools report failures as `isError` content; CLI uses exit
  codes 1/2. Exit code 2 / "Connection refused" means the tracker is down — suggest
  starting it, don't retry in a loop.
- **Concurrency**: the MCP server guards shared state with a mutex; concurrent tool
  calls are safe but their completion order is not guaranteed.
