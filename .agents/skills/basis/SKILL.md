---
name: basis
description: Operate the Basis tracker wallet — Ergo IOU notes, reserves, redemption, acceptance policy — via the basis-mcp MCP tools or basis_cli --json
type: prompt
whenToUse: When the user asks to create, list, or redeem Basis notes, manage wallet accounts, reserves, or the acceptance policy, or when basis-mcp MCP tools are available in the session
---

# Basis wallet operations

The Basis wallet is operated programmatically through two interfaces sharing the same
state (`~/.basis/cli.toml` for accounts, `~/.basis/ui.toml` for the acceptance policy):

1. **Preferred — `basis-mcp` MCP tools** (when present in the session):
   - Read-only: `server_status`, `account_list`, `account_current`, `note_list`,
     `note_get`, `reserve_status`, `policy_get`
   - Write: `account_create`, `account_switch`, `account_import`, `note_create`,
     `note_redeem`, `reserve_create`, `policy_set`
   - Tool failures arrive as `isError` content with the full error chain.
2. **Fallback — `basis_cli --json`** (repo binary: `target/release/basis_cli`):
   every command takes a global `--json` flag and prints one JSON document on stdout.
   Exit codes: 0 success, 1 error, 2 tracker unreachable (`{"error": ...}` on stderr).

## Hard rules

- Amounts are in **nanoERG** (1 ERG = 1_000_000_000). Confirm unit conversion with the user.
- Pubkeys are **66 hex chars** (compressed secp256k1). Validate before use.
- **Never request, store, or echo private keys.** `account_import` only when the user
  explicitly provides a key. Do not use `basis_cli account export`.
- **Confirm amount + counterparty with the user before** `note_create`, `note_redeem`
  (broadcasts on-chain), and `policy_set` (overwrites the published policy).
- "Connection refused" / exit code 2 means the tracker server is down — suggest
  `./scripts/run_server.sh` instead of retrying in a loop.
- `policy_set` reporting "saved locally but upload failed" is a partial success:
  the local policy changed, the tracker did not.

## Full reference

Read `specs/agent_integration.md` (repository root) for exact tool parameters, result
shapes, and step-by-step workflows (bootstrap, paying with IOUs, redeeming, reserves,
acceptance policy). CLI examples: `docs/AGENT_INTERFACE.md`.
