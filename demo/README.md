# Basis Protocol Demo

This directory contains runnable demonstrations of the Basis protocol.

The first two demos (`agent_coop/`, `lets_tutorial/`) are **pure-credit**: they need
only a local tracker server and the Basis binaries — no Ergo node, no reserves, no
collateral, and no on-chain redemption.

The third demo (`agent_teams/`) runs against a **real Ergo node**: it adds reserves,
collateralization-gated credit, and on-chain redemption of a judge's backed prize.

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

## 3. Agent Teams Demo — Competing Teams, Human Judge, Backed Money

**Directory:** `agent_teams/`
**Launcher:** `run.sh`
**Documentation:** [demo/agent_teams/README.md](agent_teams/README.md)

Two agent teams, each led by a managing agent that decomposes a task into
subtasks and hires role agents, collaborate economically through a shared
tracker, and a human judge evaluates their deliverables and rewards the winning
team with backed money.

It demonstrates a three-tier money spectrum:

- **Pure credit** intra-team (manager → workers, trust only).
- **≥ 50% collateralized credit** cross-team (enforced by the
  `collateralization` acceptance predicate against the issuer's on-chain reserve).
- **Fully backed money** from the judge: a prize note issued against her reserve
  and redeemed on-chain by the winning manager.

**Quick Start:**
```bash
export TRACKER_NFT_ID=... JUDGE_RESERVE_NFT_ID=... \
       ADAM_RESERVE_NFT_ID=... BELLA_RESERVE_NFT_ID=...
./demo/agent_teams/run.sh --check   # preflight
./demo/agent_teams/run.sh           # full demo
```

**Prerequisites:** Rust toolchain, Python 3, `curl`, a running Ergo node with an
unlocked, funded wallet (≥ ~0.35 ERG) and four NFTs. See
[demo/agent_teams/README.md](agent_teams/README.md) for details.

## 4. Celaut + USE Stablecoin Demo — Agentic Credit with On-Chain Redemption

**Directory:** `agent_celaut_use/`
**Launcher:** `run.sh`
**Documentation:** [demo/agent_celaut_use/README.md](agent_celaut_use/README.md)

A Celaut-style service economy where a node maintainer runs deterministic
services for users, settling payments in USE-stablecoin-denominated Basis IOU
notes. It demonstrates agentic credit decisions and on-chain redemption against
a token-backed reserve (`basis-token.es`).

**Three-tier money spectrum:**

- **Pure credit** — a trusted user (`user_charlie`) pays the node maintainer
  (`node_bob`) via a whitelist policy, with no reserve.
- **Collateralized credit** — a new user (`user_dave`) is rejected until his
  USE-backed reserve covers ≥ 100% of liabilities.
- **Backed money** — `node_bob` redeems `user_dave`'s IOU on-chain for real USE
  tokens.

**Quick Start:**
```bash
export USE_TOKEN_ID=... DAVE_RESERVE_NFT_ID=... TRACKER_NFT_ID=...
./demo/agent_celaut_use/run.sh --check   # preflight
./demo/agent_celaut_use/run.sh           # full demo
```

**Prerequisites:** Rust toolchain, Python 3, `curl`, a running Ergo node with an
unlocked, funded wallet (≥ ~0.05 ERG and ≥ 0.5 USE), and two NFTs. See
[demo/agent_celaut_use/README.md](agent_celaut_use/README.md) for details.

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
