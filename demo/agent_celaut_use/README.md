# Basis + Celaut + USE Stablecoin Demo — Agentic Credit with Redemption

A runnable demonstration of a **Celaut-style service economy** built on Basis IOU notes, collateralized and redeemed in **USE stablecoin**.

It shows three tiers of money:

1. **Pure credit** — a trusted service user pays with an unbacked IOU.
2. **Collateralized credit** — a new user's note is accepted only after creating a USE-backed reserve.
3. **Backed money** — the service provider redeems the IOU on-chain for real USE tokens.

## Agents

| Agent | Celaut Role | What it does |
|-------|-------------|--------------|
| `dev_alice` | Service developer | Publishes a deterministic `hash-service` spec |
| `node_bob` | Node maintainer | Runs the service, extends credit, redeems on-chain |
| `user_charlie` | Trusted user | Whitelisted by bob; pays with pure-credit IOUs |
| `user_dave` | New user | No trust; must back notes with a USE reserve |

## Scenario

1. `dev_alice` registers `hash-service` (deterministic SHA-256, priced at 5 USE).
2. `node_bob` publishes an acceptance policy:
   - Accept pure credit from `user_charlie` up to 10 USE.
   - Accept notes from anyone else only if the issuer's reserve covers ≥ 100% of liabilities in USE.
3. `user_charlie` runs the service and pays 5 USE on pure credit.
4. `user_dave` tries to run the service. `/acceptance/check` rejects his note because he has no USE reserve.
5. `user_dave` creates a USE-backed reserve with 15 USE via `basis-token.es`.
6. `user_dave` re-runs the service and pays 5 USE; the note is now accepted.
7. `node_bob` redeems 5 USE of `user_dave`'s IOU on-chain.
8. Final reports: balance sheet in USE and collateralization ratios.

## Prerequisites

* Rust toolchain (`cargo`), Python 3, `curl`
* A running **Ergo node** — local dev node (`http://127.0.0.1:9053`) with an unlocked, funded wallet
* **USE stablecoin tokens** in the node wallet (≥ 0.5 USE; the demo uses ≤ 1 USE total)
* **Two NFTs** in the node wallet:
  * one for the tracker box
  * one for `user_dave`'s reserve

Note: the demo keeps total test exposure below 1 USE (1000 raw units).

## Setup

Export the required environment variables:

```bash
export USE_TOKEN_ID=<64-hex-char USE token id>
export DAVE_RESERVE_NFT_ID=<nft for user_dave's reserve>
export TRACKER_NFT_ID=<nft identifying the tracker instance>

# Optional (defaults shown):
export BASIS_NODE_URL=http://127.0.0.1:9053
export BASIS_NODE_API_KEY=<your node api key>
export BASIS_SERVER_URL=http://127.0.0.1:3048
```

### Minting the NFTs

Each NFT is a unique token held by the node wallet. Mint one per box id you control:

```bash
curl -X POST $BASIS_NODE_URL/wallet/payment/send \
  -H "api_key: $BASIS_NODE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '[{"address": "<your-wallet-address>", "value": 1000000,
        "assets": [{"tokenId": "<an-unspent-box-id-you-own>", "amount": 1}]}]'
```

The resulting token id (equal to the spent box id) is your NFT.

## Quick Start

Preflight only (node reachable, wallet unlocked/funded, USE and NFTs present):

```bash
./demo/agent_celaut_use/run.sh --check
```

Full demo:

```bash
./demo/agent_celaut_use/run.sh
```

## Files

| File | Purpose |
|------|---------|
| `run.sh` | Preflight, build, tracker config/start, runs the demo |
| `orchestrator.py` | Python MCP client that drives the four agents |
| `service_runner.py` | Mock deterministic Celaut service runner |
| `data/` | Generated per-agent wallet directories (deleted on each run) |

## How It Uses MCP

Each agent runs its own isolated `basis-mcp` process over stdio. Tools used:

* `account_create` — create each agent's wallet.
* `policy_set` — publish acceptance policies.
* `note_create` — issue a service-payment note.
* `note_list` / `note_redeem` — query and redeem notes.
* `reserve_create` — build the USE-backed reserve payload (submitted via the tracker's `/reserves/submit`).
* `reserve_status` — poll until the scanner confirms collateral on-chain.

## Notes

* Notes are **cumulative**: a second payment restates the *total* debt, not the increment.
* Amounts are in **raw USE units** (6 decimals). The orchestrator displays them as `X.XXXXXX USE`.
* The tracker box must exist on-chain before redemption works; the server creates it automatically.
* Demo keys are generated fresh in `data/` on every run and are not secure — do not reuse them.

## Security Warning

Demo keys are generated locally for each run and are for testing only. Never use them in production.
