# Celaut + USE Stablecoin Agentic Credit Demo

## Overview

The `demo/agent_celaut_use/` demonstration combines three layers of the Basis ecosystem:

- **Celaut** peer-to-peer service architecture: agents take on roles of service
  developer, node maintainer, and service user.
- **USE stablecoin**: the unit of account for service pricing and the collateral
  asset for on-chain reserves.
- **Basis protocol**: off-chain cumulative IOU notes backed by token reserves via
  `contract/basis-token.es`.

The demo's primary purpose is to show **agentic credit** — autonomous agents
making trust and collateral decisions — alongside real **on-chain redemption**.

## Money spectrum demonstrated

1. **Pure credit**
   - `node_bob` whitelists `user_charlie` and accepts IOUs up to a 10 USE limit.
   - No reserve exists for `user_charlie` at the time of payment.
   - Demonstrates Basis's core credit-creation capability.

2. **Collateralized credit**
   - `node_bob` requires any non-whitelisted issuer to maintain a reserve that
     covers ≥ 100% of outstanding liabilities.
   - `user_dave`'s first payment attempt is rejected by `/acceptance/check`.
   - After `user_dave` creates a 15 USE token reserve, the same payment is
     accepted.

3. **Backed money / redemption**
   - `node_bob` redeems 5 USE of `user_dave`'s IOU on-chain.
   - USE tokens move from `user_dave`'s `basis-token.es` reserve to
     `node_bob`'s wallet.

## Agent roles and policies

| Agent | Role | Policy |
|-------|------|--------|
| `dev_alice` | Service developer | `reject_all` — only registers services |
| `node_bob` | Node maintainer | `whitelist(charlie, 10 USE) OR collateralization >= 100%` |
| `user_charlie` | Trusted user | `reject_all` — only pays with pure credit |
| `user_dave` | New user | `reject_all` — pays after creating USE reserve |

## On-chain requirements

- A running Ergo node at `http://127.0.0.1:9053` (configurable via `BASIS_NODE_URL`).
- An unlocked, funded node wallet with:
  - ≥ 0.05 ERG for tracker box, storage rent, and fees.
  - ≥ 0.5 USE tokens for `user_dave`'s reserve (demo uses ≤ 1 USE total).
  - A tracker NFT (`TRACKER_NFT_ID`).
  - A reserve NFT for `user_dave` (`DAVE_RESERVE_NFT_ID`).
- Tracker configured with:
  - `basis_token_reserve_contract_p2s` — the compiled `contract/basis-token.es` address.
  - `reserve_token_id` — the USE token id.
  - `reserve_token_decimals = 6`.

## Implementation notes

- The demo uses a **mock Celaut service runner** (`service_runner.py`) that
  computes a deterministic SHA-256 inside an isolated temp directory. This
  captures the Celaut determinism and BOX isolation concepts without requiring
  Docker or the full Celaut `nodo` stack.
- The `basis-mcp` `reserve_create` tool was extended to accept optional
  `token_amount` and `token_id` parameters so that the demo can create
  `basis-token.es` reserves through the same MCP interface used for ERG-backed
  reserves.
- Amounts are tracked in raw USE token units but displayed with 6 decimal
  places for readability.

## Future extensions

- Integrate the real Celaut `nodo` reference implementation for service
  execution and discovery.
- Add a reputation log so `node_bob` can adjust credit limits based on
  successful past interactions.
- Support multiple services and competing node maintainers.
- Replace the fixed pricing with dynamic quotes based on node load or
  reputation.
