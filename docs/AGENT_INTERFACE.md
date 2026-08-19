# Basis CLI Agent Interface

> **See also:** `specs/agent_integration.md` — the canonical agent integration spec
> (MCP tool reference, workflows, security rules), and `.agents/skills/basis/SKILL.md`
> — the auto-discovered project skill for AI agents.

`basis-cli` supports a machine-readable output mode intended for scripts and
agents (e.g. an MCP server): pass the global `--json` flag and every command
prints a single JSON document to stdout instead of human-readable text.

## Usage

```bash
basis-cli --json <command> ...   # or
basis-cli <command> ... --json   # --json is global, both positions work
```

- Success: the command's typed result is printed to **stdout** as pretty JSON
  (no envelope wrapper, so `jq` works directly on it). List commands print a
  JSON array. Progress/diagnostic lines that are part of the human output go
  to **stderr** in JSON mode, so stdout always stays parseable.
- Failure: `{"error": "<message>"}` is printed to **stderr**, nothing to stdout.

### Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success (JSON result on stdout) |
| 1    | Command failed (bad input, server-side error, etc.) |
| 2    | Tracker server (or Ergo node) unreachable — connection refused, timeout, DNS failure |

Without `--json` the human output and error behavior are unchanged. (Note:
clap argument-parsing errors also exit with code 2 before the command runs —
check stderr for a usage message vs. a JSON error document to distinguish.)

## Examples

### `status`

```bash
$ basis-cli --json status
{
  "healthy": true,
  "recent_events": [
    {
      "timestamp": 1783612740,
      "summary": "State commitment at height 1523400",
      "height": 1523400
    }
  ]
}
```

Server down:

```bash
$ basis-cli --json status; echo "exit=$?"
# stderr: {"error":"http://127.0.0.1:3048/: Connection Failed: ... Connection refused (os error 111) ..."}
exit=2
```

### `account list`

```bash
$ basis-cli account list --json
[
  {
    "name": "alice",
    "pubkey_hex": "03a566e2cfc17541407aeca16852786647e8c18850f0b2c52303baa955fc0a6875",
    "current": true,
    "source": "config"
  },
  {
    "name": "alice",
    "pubkey_hex": "03a566e2cfc17541407aeca16852786647e8c18850f0b2c52303baa955fc0a6875",
    "current": true,
    "source": "memory"
  }
]
```

(`source` is `config` for persisted accounts and `memory` for in-session
accounts; both groups are listed, mirroring the human output.)

### `account info`

```bash
$ basis-cli account info --json
{
  "name": "alice",
  "pubkey_hex": "03a566e2cfc17541407aeca16852786647e8c18850f0b2c52303baa955fc0a6875",
  "created_at": 1783612700
}
```

Prints `null` when no account is selected. `account switch` prints a small
confirmation: `{"switched": "alice"}`.

### `note list --recipient`

```bash
$ basis-cli note list --recipient --json
[
  {
    "issuer_pubkey": "02d84d...",
    "recipient_pubkey": "03a566...",
    "amount": 1000000,
    "redeemed": 0,
    "outstanding": 1000000,
    "timestamp": 1783612740170
  }
]
```

Prints `[]` when there are no notes (or when neither `--issuer` nor
`--recipient` is given). `note get --issuer <pk> --recipient <pk> --json`
prints the note object or `null`.

### `note create`

```bash
$ basis-cli note create --recipient 03a566... --amount 1000000 --json
{
  "issuer_pubkey": "02d84d...",
  "recipient_pubkey": "03a566...",
  "amount": 1000000,
  "timestamp": 1783612740170,
  "signature": "a1b2...",
  "reserve_status_before": { "total_debt": 0, "collateral": 10000000, "...": "..." },
  "reserve_status_after": { "total_debt": 1000000, "collateral": 10000000, "...": "..." }
}
```

`note create --demo --json` prints the demo note in the Scala demo JSON
format (same document the human mode prints).

### `reserve status`

```bash
$ basis-cli reserve status --json
{
  "total_debt": 1000000,
  "collateral": 10000000,
  "collateralization_ratio": 10.0,
  "note_count": 1,
  "last_updated": 1783612740,
  "issuer_pubkey": "02d84d...",
  "has_pending_refund": false
}
```

### Other commands

- `acceptance upload --policy-file policy.toml --json` → `{"uploaded": true, "policy_hash": "..."}`.
- `acceptance check --issuer PUBKEY --recipient PUBKEY --total-debt N --json` → `{"acceptable": true, "reason": null}`.
- `generate-keypair --json` → `{"public_key_hex": "...", "private_key_hex": "..."}`
- `account create/import` → the created account object; `account export` →
  `{"name": ..., "private_key": ...}` or `null`.
- `reserve create --json` → the reserve-creation payload
  (`{nft_id, owner_pubkey, amount, payload: {requests, fee, change_address}}`).
- `reserve collateralization --json` → `{issuer_pubkey, ratio, status}`.
- `note redeem --json` → `{amount, server_sign, redemption_id?, proof_available?, tx_id?}`.
- `transaction generate-redemption --json` → `{tx_id}` with `--local-sign`,
  otherwise `{transaction, issuer_pubkey, ..., output_file?}` (the unsigned
  transaction plus build metadata).
- `transaction redeem-assisted --json` → `{tx_id}`.
- `test test-redemption --json` → `{issuer_pubkey, recipient_pubkey, redemption_amount, output_file, transaction}`.

## Typed results for programmatic use

The command logic lives in `basis_cli_lib::commands::*` as `pub` functions
returning serde-serializable result structs (e.g. `account::create_account`,
`note::list_notes`, `reserve::get_reserve_status`, `status::get_server_status`,
`transaction::generate_redemption_transaction`). The `handle_*_command`
functions are thin wrappers that render either human text or JSON. Other
binaries (TUI, MCP server) can depend on the library and reuse the same cores.

## MCP server (`basis-mcp`)

The `basis_mcp` crate provides `basis-mcp`, an MCP (Model Context Protocol)
server over stdio built with the `rmcp` SDK (0.17). It wraps the same typed
command cores as `basis-cli --json`, so an agent can use the wallet through
any MCP client instead of shelling out.

Run it directly (stdio transport; stdout carries only protocol messages,
logging goes to stderr, quiet by default — set `RUST_LOG` for more):

```bash
basis-mcp                          # server URL from ~/.basis/cli.toml
basis-mcp --server-url http://127.0.0.1:3048   # or override (env: BASIS_SERVER_URL)
```

### Tools

Read-only (`readOnlyHint: true`):

| Tool | Description |
|------|-------------|
| `server_status` | Tracker health + recent events (optional `server_url` override) |
| `account_list` | All accounts: name, pubkey, created_at (never private keys) |
| `account_current` | Current account name + pubkey (`null` if none) |
| `note_list` | Notes for `pubkey` (default: current account), `direction` "issued"/"received" (default "received") |
| `note_get` | A note by `issuer` + `recipient` pubkeys |
| `reserve_status` | Reserve status for `pubkey` (default: current account) |
| `policy_get` | Local acceptance policy from `~/.basis/ui.toml` |

Write tools:

| Tool | Description |
|------|-------------|
| `account_create` | Create + persist account (`name`) |
| `account_switch` | Switch current account (`name`) |
| `account_import` | Import from `private_key_hex` (stored locally, never echoed back) |
| `note_create` | Create note to `recipient` for `amount` nanoERG, signed with the current account |
| `note_redeem` | Redeem `amount` nanoERG from `issuer` (local-signing path; `destructiveHint: true`) |
| `reserve_create` | Build reserve-creation payload (`nft_id`, `amount`; owner = current account) |
| `policy_set` | Replace acceptance policy (`policy` object matching `AcceptanceConfig`); saves to `~/.basis/ui.toml` and uploads signed with the current account (`destructiveHint: true`) |

Tool results are JSON text content (same shapes as `basis-cli --json` output);
failures are returned as MCP tool errors (`isError: true`) with the underlying
error message — the server never exits on a tool error.

### Security notes

- **No private-key export**: there is deliberately no key-export tool, and no
  tool response contains key material. `account_import` takes a key as input
  but only returns the account name and public key.
- **Signing stays in-process**: note creation, redemption, and policy upload
  are signed locally by the wallet; keys never leave the `basis-mcp` process.
- **Tracker-server authentication**: when the tracker server requires
  authentication, `basis-mcp` reads credentials from environment variables
  first, then falls back to `~/.basis/cli.toml`:
  - `BASIS_TRACKER_AUTH_MODE`: `none`, `api_key`, or `signature`
  - `BASIS_TRACKER_API_KEY`: shared secret for API-key mode
  - `BASIS_TRACKER_AUTH_PUBKEY`: hex public key for signature mode
  - `BASIS_TRACKER_AUTH_SECRET_KEY`: hex secret key for signature mode

  For signature mode the private key is used only to sign HTTP requests to the
  tracker server; it is not the user's wallet key. See
  `specs/server/authentication_authorization.md` for the full scheme.

### Client configuration

Kimi CLI / Claude Desktop style `mcpServers` entry:

```json
{
  "mcpServers": {
    "basis": {
      "command": "/path/to/basis-mcp"
    }
  }
}
```

With a server-URL override:

```json
{
  "mcpServers": {
    "basis": {
      "command": "/path/to/basis-mcp",
      "args": ["--server-url", "http://127.0.0.1:3048"]
    }
  }
}
```

With tracker-server authentication via environment variables:

```json
{
  "mcpServers": {
    "basis": {
      "command": "/path/to/basis-mcp",
      "env": {
        "BASIS_TRACKER_AUTH_MODE": "signature",
        "BASIS_TRACKER_AUTH_PUBKEY": "020202...",
        "BASIS_TRACKER_AUTH_SECRET_KEY": "..."
      }
    }
  }
}
```
