# Basis Agent Co-op Demo

A minimal multi-agent demonstration of a **pure-credit joint economy** built on Basis IOU notes.
Three scripted agents exchange services and settle with signed off-chain notes through the
[`basis-mcp`](../README.md#mcp-server) MCP server. No reserves, collateral, or on-chain
redemption are required — this demo focuses on the simplest possible credit-based payment flow.

## Agents

| Agent | Role |
|-------|------|
| **Alice** | Compute provider |
| **Bob** | Storage provider |
| **Charlie** | API gateway |

## Scenario

1. Each agent bootstraps its own wallet by creating an account through its isolated
   `basis-mcp` process.
2. Every agent publishes a signed **acceptance policy** that whitelists the other two agents
   with a 0.05 ERG credit limit.
3. The agents exchange services and pay with cumulative IOU notes:
   * Alice pays Bob 0.01 ERG for storage.
   * Bob pays Charlie 0.01 ERG for API routing.
   * Charlie pays Alice 0.01 ERG for compute.
   * Alice pays Bob another 0.02 ERG for more storage (cumulative debt becomes 0.03 ERG).
4. The orchestrator prints a balance sheet and a credit-utilization report.

## Prerequisites

* Rust toolchain (`cargo`)
* Python 3
* `curl` (used by `run.sh` to wait for the tracker)
* No Ergo node or blockchain access is required for this version

## Quick Start

```bash
./demo/agent_coop/run.sh
```

This will:

1. Build `basis_server`, `basis_mcp`, and `basis_cli` in release mode.
2. Start a tracker server on `http://127.0.0.1:3048` (or reuse one already running).
3. Run `orchestrator.py`, which drives the three agents via MCP stdio.
4. Stop the tracker server it started.

To use a different tracker URL:

```bash
BASIS_SERVER_URL=http://127.0.0.1:4048 ./demo/agent_coop/run.sh
```

## Files

| File | Purpose |
|------|---------|
| `run.sh` | Builds binaries, starts the tracker, runs the demo. |
| `orchestrator.py` | Python MCP client that spawns one `basis-mcp` per agent and executes the scenario. |
| `data/` | Generated per-agent wallet directories (`alice`, `bob`, `charlie`). Deleted on each run. |

## How It Uses MCP

The orchestrator talks to `basis-mcp` over stdio using the Model Context Protocol:

* `initialize` / `notifications/initialized` handshake.
* `tools/call` with the following tools:
  * `account_create` — create each agent's wallet.
  * `policy_set` — publish the whitelist acceptance policy.
  * `note_create` — issue a service-payment note.
  * `note_list` — query issued/received notes for the balance sheet.

Each agent's `basis-mcp` process is launched with a different `HOME` directory so accounts and
private keys stay isolated, while all processes connect to the same shared tracker.

## Expected Output

```
================================================================
Basis Agent Co-op Demo — Pure Credit Economy
================================================================
Tracker server: http://127.0.0.1:3048
basis-mcp:      /.../basis-tracker/target/release/basis-mcp

[BOOTSTRAP] Starting alice's MCP wallet...
  alice account: 03a566e2cfc17541407...
...

[NOTE] alice pays bob 10000000 nanoERG for: Bob stores 1 GB for Alice
  issued -> total debt now 10000000 nanoERG
...

================================================================
FINAL BALANCE SHEET
================================================================
Agent           Assets (ERG)    Liabilities (ERG)        Net (ERG)
----------------------------------------------------------------
alice             0.010000           0.030000      -0.020000
bob               0.030000           0.010000       0.020000
charlie           0.010000           0.010000       0.000000
----------------------------------------------------------------
Balance sheet checks out: net positions sum to zero.

[CREDIT LIMITS]
  alice -> 03af13e39dd0ccc7... 0.030000/0.050000 ERG [█████░░░░░] 60%
  bob -> 0303030303030303... 0.010000/0.050000 ERG [██░░░░░░░░] 20%
  charlie -> 03a566e2cfc17541... 0.010000/0.050000 ERG [██░░░░░░░░] 20%

Demo complete.
```

## Notes

* Notes are **cumulative**: the second payment from Alice to Bob sets the total debt to the
  new cumulative amount (0.03 ERG), not an incremental 0.02 ERG.
* The acceptance policies are published to the tracker, but this starter demo does not rely on
  the tracker enforcing them during `note_create`; it demonstrates policy publishing and credit
  tracking.
* Demo keys are generated fresh in `data/` on every run and are not secure — do not reuse them.

## Future Extensions

* Add a reserve-backed agent and on-chain redemption.
* Add a "treasury" agent that nets circular debt among the co-op members.
* Replace scripted decisions with LLM-driven agents using the same MCP tools.
