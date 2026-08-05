# TUI Wallet LETS Specification

## Overview

This specification describes how to model a **Local Exchange Trading System (LETS)** using the `basis-ui` TUI wallet as the member client. The design is intentionally minimal: pure mutual credit, no on-chain reserves, no collateral, and no redemption. It demonstrates the simplest trust-based payment flow in Basis and serves as a foundation for community-currency experiments.

## Goals

- Let multiple people run `basis-ui` wallets against a single local tracker.
- Represent LETS membership through acceptance-policy whitelists.
- Cap individual negative balances with per-member credit limits.
- Show real-time assets, liabilities, and net position in the TUI stats screen.
- Require no Ergo node, no reserves, and no blockchain fees.

## Mapping LETS Concepts to Basis

| LETS Concept | Basis Implementation |
|--------------|----------------------|
| Member | `basis-ui` account + isolated `$HOME` directory |
| Membership list | Acceptance-policy whitelist predicate |
| Common credit limit | `max_debt` on the whitelist predicate |
| Unit of account | nanoERG (for accounting; no actual ERG moves) |
| Payment | Cumulative IOU note from payer to payee |
| Balance sheet | TUI Wallet Stats screen (assets − liabilities) |

## Architecture

```
┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐
│   Alice TUI     │         │    Bob TUI      │         │   Carol TUI     │
│  (basis-ui)     │         │   (basis-ui)    │         │   (basis-ui)    │
└────────┬────────┘         └────────┬────────┘         └────────┬────────┘
         │                           │                           │
         │  signed IOU notes         │  signed IOU notes         │
         │  + policy uploads         │  + policy uploads         │
         └───────────────┬───────────┴───────────┬───────────────┘
                         │                       │
                  ┌──────▼───────────────────────▼──────┐
                  │      Basis Tracker Server           │
                  │  stores hash(issuer||recipient) →   │
                  │  totalDebt in an AVL tree           │
                  └─────────────────────────────────────┘
```

Each TUI instance owns:
- `~/.basis/ui.toml` — server URL, current account, acceptance policy, address book.
- `~/.basis/cli.toml` — account keypairs used for signing notes and policies.

The tracker server owns:
- `acceptance_policies` storage — per-recipient policies uploaded by members.
- `iou_notes` storage — cumulative note state.

## Acceptance Policy

Each member uploads a policy that whitelists all other members and rejects everyone else by default.

```toml
default = "reject"
root = "lets_trust"

[[predicates]]
name = "lets_members"
type = "whitelist"
holders = ["02bob...", "02carol..."]
max_debt = 5000000000  # 5 ERG

[[predicates]]
name = "lets_trust"
type = "any_of"
predicates = ["lets_members"]
```

### Policy Semantics

- `default = "reject"`: notes from non-members are rejected.
- `lets_members`: accepts if the issuer is in the whitelist and their cumulative debt is within `max_debt`.
- `lets_trust`: root predicate that delegates to `lets_members` (extendable with additional predicates).

See [`specs/acceptance_predicates.md`](acceptance_predicates.md) for the full predicate language.

## Member Isolation

Because `basis-ui` and `basis-cli` store configuration in `~/.basis/`, multiple wallets on one machine must use different `$HOME` directories:

```bash
HOME=/path/to/alice /path/to/basis-ui
HOME=/path/to/bob   /path/to/basis-ui
HOME=/path/to/carol /path/to/basis-ui
```

This is handled automatically by `demo/lets_tutorial/run_lets_tutorial.sh`.

## Address Book Seeding

The TUI address book is persisted in `ui.toml` under `[address_book]`:

```toml
[address_book]
bob = "0216ebaa..."
carol = "032934b5..."
```

On startup, `basis-ui` merges saved address-book entries with local account names. Members can therefore select payees by name when creating notes.

## Sample Transaction Trace

Starting balances are zero.

1. Alice → Bob: 2 ERG
   - Alice liabilities: +2 ERG
   - Bob assets: +2 ERG
2. Bob → Carol: 1 ERG
   - Bob liabilities: +1 ERG
   - Carol assets: +1 ERG
3. Carol → Alice: 1.5 ERG
   - Carol liabilities: +1.5 ERG
   - Alice assets: +1.5 ERG

Final net positions:

| Member | Assets | Liabilities | Net |
|--------|--------|-------------|-----|
| Alice  | 1.5    | 2.0         | −0.5 |
| Bob    | 2.0    | 1.0         | +1.0 |
| Carol  | 1.0    | 1.5         | −0.5 |

The sum of net positions is zero, which is the accounting identity of a closed mutual-credit system.

## CLI Integration

The demo uses a new `basis-cli acceptance upload` command to upload policies without requiring manual TUI interaction:

```bash
basis-cli acceptance upload --policy-file policy.toml
```

The command:
1. Parses `policy.toml` as an `AcceptanceConfig`.
2. Serializes it to JSON.
3. Signs the JSON with the current account's private key.
4. POSTs it to `/acceptance/policy` on the tracker.

A companion command checks whether a note would be accepted:

```bash
basis-cli acceptance check --issuer 02alice... --recipient 02bob... --total-debt 2000000000
```

## Files

| File | Purpose |
|------|---------|
| `demo/lets_tutorial/run_lets_tutorial.sh` | Orchestrates tracker + members + tmux launch |
| `demo/lets_tutorial/README.md` | Human-facing tutorial |
| `demo/lets_tutorial/config/community.toml` | Example raw acceptance policy template |
| `crates/basis_cli/src/commands/acceptance.rs` | `acceptance upload` / `acceptance check` commands |
| `crates/basis_app/src/app.rs` | TUI config with address-book persistence |
| `crates/basis_app/src/ui.rs` | Address-book add/delete with persistence |

## Future Extensions

- **Federated LETS**: multiple trackers that accept each other's notes across communities.
- **Demurrage / interest**: time-based predicates that adjust acceptance based on note age.
- **Anchored LETS**: optional reserve backing for members who want collateralized notes while keeping the community whitelist for others.
- **MCP agent members**: reuse the policy and account setup, but drive payments through `basis-mcp` instead of the TUI.

## References

- [`specs/acceptance_predicates.md`](acceptance_predicates.md)
- [`demo/lets_tutorial/README.md`](../demo/lets_tutorial/README.md)
- [`demo/agent_coop/README.md`](../demo/agent_coop/README.md)
