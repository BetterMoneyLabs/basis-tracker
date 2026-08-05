# Basis Protocol Demo

This directory contains runnable demonstrations of the Basis protocol.

Both demos are **pure-credit**: they need only a local tracker server and the
Basis binaries — no Ergo node, no reserves, no collateral, and no on-chain
redemption.

## 1. Agent Service Co-op Demo

**Directory:** `agent_coop/`
**Launcher:** `run.sh`
**Documentation:** [demo/agent_coop/README.md](agent_coop/README.md)

A multi-agent economy where three scripted agents (Alice, Bob, Charlie) exchange
services and settle with cumulative IOU notes through the
[`basis-mcp`](../specs/agent_integration.md) MCP server. It demonstrates the
simplest possible credit-based payment flow and serves as a starting point for
LLM-driven agent economies.

**Scenario:**
- Alice pays Bob for storage.
- Bob pays Charlie for API routing.
- Charlie pays Alice for compute.
- Alice pays Bob again for more storage (cumulative debt).

The orchestrator prints a balance sheet and credit-utilization report.

**Quick Start:**
```bash
./demo/agent_coop/run.sh
```

This builds `basis_server`, `basis_mcp`, and `basis_cli`, starts a tracker on
`http://127.0.0.1:3048`, and runs the three agents via MCP stdio. State is written
to `demo/agent_coop/data/`.

**Prerequisites:** Rust toolchain, Python 3, `curl`.

## 2. LETS Tutorial — Mutual Credit with TUI Wallet

**Directory:** `lets_tutorial/`
**Launcher:** `run_lets_tutorial.sh`
**Documentation:** [demo/lets_tutorial/README.md](lets_tutorial/README.md)

A human-driven Local Exchange Trading System (LETS) demo where each community
member runs their own `basis-ui` TUI wallet. Members whitelist each other, set a
common credit limit, and issue cumulative IOU notes. The TUI stats screen shows
assets, liabilities, and net position in real time.

**Scenario:**
- Alice pays Bob 2 ERG for bread.
- Bob pays Carol 1 ERG for a ride.
- Carol pays Alice 1.5 ERG for tutoring.

**Quick Start:**
```bash
./demo/lets_tutorial/run_lets_tutorial.sh --tmux
```

This builds `basis_server`, `basis_cli`, and `basis-ui`, starts a tracker, creates
isolated home directories for each member, uploads their LETS acceptance policies,
and launches all three wallets in a tmux session.

**Prerequisites:** Rust toolchain, `bash`, `curl`, `python3`, optional `tmux`.

## References

- [Agent Integration Spec](../specs/agent_integration.md)
- [TUI Wallet LETS Spec](../specs/tui_wallet_lets.md)
- [Acceptance Predicate Spec](../specs/acceptance_predicates.md)
- [Protocol Specification](../specs/spec.md)

## Security Warning

Demo keys are generated locally for each run and are for testing only. Never use
them in production. In production:
- Generate secure keypairs
- Use hardware wallets or HSMs
- Protect private keys
- Monitor reserve collateralization if you later move to backed notes
