# Basis Sovereign Agent Demo — Metabolism + Growth, No Human in the Loop

A runnable demonstration of a **self-sovereign agentic economy**: an agent that
*maintains itself* (buys its own compute), *improves itself* (reinvests surplus
into capabilities), and settles everything in **USE stablecoin** on a real Ergo
node — with **zero human actions**.

Built on the primitives proven by [`demo/agent_coop`](../agent_coop/README.md)
and [`demo/agent_celaut_use`](../agent_celaut_use/README.md) (see
[`specs/sovereign_agentic_economy.md`](../../specs/sovereign_agentic_economy.md)).

## The sovereignty loop

```
        ┌──────────── metabolism ────────────┐
        │ rent compute → execute task        │
        ▼                                    │
  infra_compute                       bounty_board (escrow-backed,
  (collateral-gated credit)           hash-verified bounties)
        ▲                                    │
        │ collateralization ratio            ▼
        │ = survival variable         backed income note
        │                                    ▼
        └── fixed seed reserve ◄── on-chain redemption (real USE)
                                             │
                                     surplus → skill_vendor + tier upgrade
                                             ▼
                                       growth: higher-value tasks
```

## Agents

| Agent | Role | Policy |
|-------|------|--------|
| `sovereign` | Protagonist. Identity = Ergo node wallet key; fixed USE seed reserve ("seed capital") | whitelist(bounty_board, 1500) |
| `infra_compute` | Celaut node maintainer selling compute tiers v1/v2 | initially `collateralization ≥ 100%`; after 3 clean rounds adds `whitelist(sovereign, 200)` |
| `bounty_board` | Automated escrow/judge posting hash-verified bounties against a fully-collateralized reserve | reject-all (only issues backed money) |
| `skill_vendor` | Sells the `premium-markets` pack unlocking premium tasks | reject-all |

## Scenario

All amounts in raw USE units (3 decimals).

1. **Bootstrap** — each agent creates an isolated `basis-mcp` wallet; policies are published.
   Preflight has already locked two on-chain reserves: the sovereign's **700 seed**
   and the board's **1000 escrow**.
2. **Stranger gate** — `/acceptance/check` live-rejects an unfunded probe key:
   no reserve, no reputation → no compute.
3. **Rounds 1–3 (metabolism)** — sovereign buys v1 compute (25/round) on credit,
   executes basic bounties (90 each); the board verifies the deliverable hash and
   issues cumulative backed notes; the first income is **redeemed on-chain** into
   real USE. Acceptance rides the **collateralization** branch.
4. **Credit emergence** — after three clean rounds infra republishes its policy to
   whitelist the sovereign up to 200: pure credit earned by behavior. Later rounds
   show acceptance flipping between whitelist and collateral branches as cumulative
   debt crosses the limit.
5. **Round 4 (growth)** — sovereign buys the `premium-markets` pack (120 from the
   vendor; amortizes in 3 rounds at +55 margin/round), upgrades to v2 compute,
   unlocks premium bounties (**300**, margin +120 vs +65).
6. **Round 5 (shock)** — prices ×2. Premium margin flips negative (−60); the
   decision engine downgrades to basic (+40) and survives.7. **Round 6 (recovery)** — prices normalize; re-upgrade; final debt 605 vs seed
   700 → collateralization **1.16**, runway 0 premium rounds.
8. **Final report** — per-round P&L, collateralization history, capability timeline;
   P&L: earned 960, spent 605 (incl. skill pack), net **+355 raw USE**, all income
   settled on-chain.

## Prerequisites

* Rust toolchain (`cargo`), Python 3, `curl`
* A running **Ergo node** — local dev node (`http://127.0.0.1:9053`) or mainnet node,
  unlocked wallet
* Wallet funds:
  * ≥ ~0.15 ERG (tracker box, two reserve storage rents, fee boxes)
  * ≥ 1.7 USE total across the wallet and already-locked reserves
    (700 seed + 1000 escrow)
* **Three NFTs** in the node wallet (or already locked in their reserve boxes):
  * tracker box NFT (`TRACKER_NFT_ID`)
  * sovereign seed reserve NFT (`SOVEREIGN_RESERVE_NFT_ID`)
  * bounty escrow reserve NFT (`BOUNTY_ESCROW_NFT_ID`)

## Setup

```bash
export USE_TOKEN_ID=<64-hex USE token id>
export TRACKER_NFT_ID=<tracker box nft>
export SOVEREIGN_RESERVE_NFT_ID=<sovereign seed reserve nft>
export BOUNTY_ESCROW_NFT_ID=<bounty escrow reserve nft>

# Optional (defaults shown):
export BASIS_NODE_URL=http://127.0.0.1:9053
export BASIS_NODE_API_KEY=<your node api key>
export BASIS_SERVER_URL=http://127.0.0.1:3048

# Re-run tuning (defaults shown):
export SEED_RESERVE_USE=700
export ESCROW_RESERVE_USE=1000
export BOUNTY_PUBKEY=02ea220b8d7b6b1727b7e555493144f1b7085debdf24e41e547a687425b0d3c802
export BOUNTY_SECRET=<only if you changed BOUNTY_PUBKEY>
```

### Minting the NFTs

Each NFT is a unique token held by the node wallet:

```bash
curl -X POST $BASIS_NODE_URL/wallet/payment/send \
  -H "api_key: $BASIS_NODE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '[{"address": "<your-wallet-address>", "value": 1000000,
        "assets": [{"tokenId": "<an-unspent-box-id-you-own>", "amount": 1}]}]'
```

The resulting token id (equal to the spent box id) is your NFT. Mint three.

## Quick Start

Preflight only (node reachable, wallet unlocked/funded, NFTs present, both
reserves locked):

```bash
./demo/agent_sovereign/run.sh --check
```

Full demo:

```bash
./demo/agent_sovereign/run.sh
```

Engine self-test (no node required):

```bash
python3 demo/agent_sovereign/economics.py
```

## Files

| File | Purpose |
|------|---------|
| `run.sh` | Preflight, reserve locking, build, tracker config/start, runs the demo |
| `orchestrator.py` | Python MCP client driving the four agents through the rounds |
| `market.py` | Compute tiers, skill packs, deterministic task execution/verification |
| `economics.py` | Ledger, budget rules, adaptation/growth policy, reports, self-test |
| `reserve_helper.py` | Direct reserve creation via the Ergo node wallet API |
| `data/` | Generated per-agent wallet directories (deleted on each run) |

## How It Uses MCP

Same pattern as the sibling demos — one `basis-mcp` process per agent over stdio,
each with an isolated `HOME`, all connected to one shared tracker:

* `account_create` / `account_import` / `account_switch` — wallets;
* `policy_set` — publish acceptance policies (the sovereignty rules);
* `note_create` — pay for compute/skills, issue escrow-backed bounties;
* `note_list` / `reserve_status` — reporting;
* `note_redeem` — settle earned income on-chain (falls back to the run-6-proven
  `basis_cli transaction generate-redemption --local-sign` path).

## Notes

* Notes are **cumulative**: each payment restates the pair's total debt.
* The sovereign's identity is the **node wallet key**, so on-chain income lands
  where the preflight-funded reserves came from — the metabolic loop closes.
* The seed reserve is intentionally **fixed** (no top-up surface exists yet):
  cumulative debt approaches the ceiling every round, which is what makes
  "runway" a real number. See the gap analysis in
  [`specs/sovereign_agentic_economy.md`](../../specs/sovereign_agentic_economy.md).
* Demo keys are generated fresh in `data/` each run (except the bounty-board
  keypair, which must own its preflight-locked escrow). Not secure — do not reuse.

## Security Warning

Demo keys are for testing only. In production use secure key generation,
hardware wallets/HSMs, and monitor reserve collateralization continuously.
