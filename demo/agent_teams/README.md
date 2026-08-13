# Basis Agent Teams Demo

A multi-agent demonstration of a **three-tier money spectrum** built on Basis IOU notes.
Two agent teams — each led by a **managing agent** that decomposes a task into subtasks and
hires role agents — collaborate economically through the
[`basis-mcp`](../README.md#mcp-server) MCP server, and a **human judge** evaluates their
deliverables and rewards the winning team with **backed money** that is redeemed on-chain.

Unlike [`demo/agent_coop`](../agent_coop/README.md), this demo requires a **real Ergo node**:
reserves, collateralization checks, and redemption are all on-chain.

## The three tiers of money

| Tier | Where | Backing |
|------|-------|---------|
| **Pure credit** | Intra-team (manager → workers) | Trust only — whitelisted with a credit limit |
| **Collateralized credit** | Cross-team (manager → rival workers) | Issuer's reserve must cover **≥ 50%** of its liabilities (`collateralization` acceptance predicate) |
| **Backed money** | Judge → winning manager | Judge's on-chain reserve, **≥ 100%** collateralized, redeemable on-chain |

## Cast

| Agent | Team | Role |
|-------|------|------|
| `adam` | Alpha | **Manager** — decomposes the task, hires workers, submits the deliverable |
| `ava` | Alpha | Compute worker |
| `alex` | Alpha | Storage worker |
| `bella` | Beta | **Manager** |
| `bryn` | Beta | Compute worker |
| `ben` | Beta | Storage worker |
| `judy` | — | **Human judge** — scripted wallet, human decision (stdin prompt; `--auto` picks a scripted winner) |

## Scenario

1. Each agent bootstraps its own wallet via an isolated `basis-mcp` process.
2. Acceptance policies are published:
   * workers accept **pure credit** from their own manager (0.05 ERG limit);
   * workers accept **cross-team credit** from the rival manager only if the issuer's reserve
     covers ≥ 50% of its liabilities (0.02 ERG limit);
   * managers accept **the judge's money** only if fully backed (0.15 ERG limit);
   * the judge rejects all incoming notes — she only issues money.
3. The orchestrator shows the collateralization gate live: a cross-team payment is **rejected**
   by `/acceptance/check` before any reserve exists, and **accepted** after the managers'
   reserves are confirmed on-chain.
4. On-chain reserves are created: judy 0.20 ERG, each manager 0.05 ERG.
5. Work round — managers pay workers with cumulative IOU notes (pure credit), then buy extra
   capacity from the rival team (≥ 50% backed credit):
   * `adam → ava` 0.02 ERG (compute), `adam → alex` 0.01 ERG (storage)
   * `bella → bryn` 0.02 ERG (compute), `bella → ben` 0.01 ERG (storage)
   * `adam → bryn` 0.015 ERG (Alpha buys extra compute from Beta)
   * `bella → alex` 0.005 ERG (Beta buys backup storage from Alpha)
6. The **human judge** reviews both deliverables and picks a winner interactively.
7. Judy issues a **0.10 ERG backed prize note** to the winning manager; the note's
   `reserve_status_before/after` is printed as proof of backing.
8. The winning manager **redeems 0.04 ERG of the prize on-chain** (`tx_id` printed).
9. The winning manager pays completion bonuses to its workers (cumulative note restatement).
10. Reports: per-agent and per-team balance sheet, and a collateralization report per issuer.

## Prerequisites

* Rust toolchain (`cargo`), Python 3, `curl`
* A running **Ergo node** — local dev node (`http://127.0.0.1:9053`, see
  [docs/ergo_node_setup.md](../../docs/ergo_node_setup.md)) or testnet
* The node **wallet unlocked**, funded with ≥ ~0.35 ERG (three reserves + tracker box + fees)
* **Four NFTs** in the node wallet (see "Minting the NFTs" below)

## Setup

Export the required environment variables:

```bash
export TRACKER_NFT_ID=<nft identifying the tracker instance>
export JUDGE_RESERVE_NFT_ID=<nft for the judge's reserve>
export ADAM_RESERVE_NFT_ID=<nft for team Alpha manager's reserve>
export BELLA_RESERVE_NFT_ID=<nft for team Beta manager's reserve>

# Optional (defaults shown):
export BASIS_NODE_URL=http://127.0.0.1:9053
export BASIS_NODE_API_KEY=<your node api key>
export BASIS_SERVER_URL=http://127.0.0.1:3048
```

### Minting the NFTs

Each NFT is just a unique token held by the node wallet. Mint one per box id you control, e.g.:

```bash
curl -X POST $BASIS_NODE_URL/wallet/payment/send \
  -H "api_key: $BASIS_NODE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '[{"address": "<your-wallet-address>", "value": 1000000,
        "assets": [{"tokenId": "<an-unspent-box-id-you-own>", "amount": 1}]}]'
```

The resulting token id (equal to the spent box id) is your NFT. Repeat four times, or transfer
existing unique tokens to the wallet. See
[docs/TRACKER_BOX_SETUP.md](../../docs/TRACKER_BOX_SETUP.md) §"Tracker NFT Created".

## Quick Start

Preflight only (node reachable, wallet unlocked/funded, all four NFTs present):

```bash
./demo/agent_teams/run.sh --check
```

Full demo:

```bash
./demo/agent_teams/run.sh
```

This will:

1. Run the preflight checks and fail fast with guidance if anything is missing.
2. Build `basis_server`, `basis_mcp`, and `basis_cli` in release mode.
3. Generate a fresh demo tracker keypair and a tracker config pointing at your node.
4. Start a tracker on `http://127.0.0.1:3048` and wait for the on-chain **tracker box**
   (auto-created on startup; required for redemption — see
   [docs/TRACKER_BOX_SETUP.md](../../docs/TRACKER_BOX_SETUP.md)).
5. Run `orchestrator.py`, which drives the seven agents via MCP stdio.
6. Stop the tracker server it started.

When stdin is not a TTY the judge's decision is scripted (`--auto`, team Alpha wins).

## Files

| File | Purpose |
|------|---------|
| `run.sh` | Preflight, build, tracker config/start, runs the demo. |
| `orchestrator.py` | Python MCP client that spawns one `basis-mcp` per agent and executes the scenario. |
| `data/` | Generated per-agent wallet directories. Deleted on each run. |

## How It Uses MCP

Same pattern as `demo/agent_coop` — one `basis-mcp` process per agent over stdio, each with an
isolated `HOME` — plus the backed-money tools:

* `account_create`, `policy_set`, `note_create`, `note_list` — as in the co-op demo;
* `reserve_create` — builds the reserve-creation payload (submitted via the tracker's
  `/reserves/submit` endpoint);
* `reserve_status` — polled until the scanner confirms collateral on-chain;
* `note_redeem` — local-signing on-chain redemption of the prize.

The composite acceptance policies use the `any_of` / `all_of` / `whitelist` /
`collateralization` predicates from [specs/acceptance_predicates.md](../../specs/acceptance_predicates.md).

## Notes

* Notes are **cumulative**: a second payment restates the *total* debt, not the increment.
* The tracker box must exist on-chain before redemption works; the server creates it
  automatically, but on testnet it takes a couple of blocks. If redemption is not ready in
  time, the demo prints a warning and continues — retry later with
  `basis-cli note redeem`.
* Demo keys are generated fresh in `data/` on every run and are not secure — do not reuse them.
  The demo tracker secret key is written to the generated `config/basis.toml` (gitignored).
