# Sovereign Agentic Economy — Principles, Architecture, and Demo Design

## Motivation

The existing demos establish the payment rails for machine-to-machine economies:

- [`demo/agent_coop/`](../demo/agent_coop/) — mutual credit between scripted agents
  (pure-credit IOU notes, acceptance policies as trust boundaries).
- [`demo/agent_celaut_use/`](../demo/agent_celaut_use/) — a Celaut-style service market
  with the full money spectrum: pure credit → collateralized credit → on-chain
  redemption in USE stablecoin (proven on mainnet, see
  [`celaut_use_demo_run6_report.md`](celaut_use_demo_run6_report.md)).
- [`demo/agent_teams/`](../demo/agent_teams/) — team competition judged by a human.

Every one of these economies has a hidden human in the loop. The judge funds the
prize, a human wallet funds reserves, scripted users never face a budget. No agent
ever pays for its own compute, and none ever reinvests earnings into its own
capabilities.

This spec defines **agent sovereignty** and describes how a real-world sovereign
agentic economy can be built strictly on the primitives those demos already
proved:

> **Self-sovereignty = metabolism + growth.**
> An agent is sovereign when it can *maintain itself* (buy the compute, storage,
> and bandwidth it needs to keep operating) and *improve itself* (reinvest surplus
> into capabilities that raise its future earning power) — closed-loop, with no
> human action anywhere in the cycle.

## Principles inherited from the prior art

| Source | Principle | Basis mechanism |
|---|---|---|
| Celaut | Separation of roles: service developer / node maintainer / user | Same cast, plus a fourth: the *sovereign agent* as a user that is also an earner |
| Celaut | Trustless coordination via payment systems | Cumulative IOU notes; acceptance predicates instead of license contracts |
| Celaut | Reputation converts behavior into privilege | Track record migrates an issuer from the collateralization branch of a policy to the whitelist branch (credit emergence) |
| Celaut | Deterministic services (BOX isolation) | Deterministic SHA-256 task classes with hash-checked deliverables |
| agent_coop | Mutual credit, programmable trust boundaries | `whitelist` + `max_debt` predicates published through `basis-mcp` |
| agent_celaut_use | Money spectrum gated on-chain | `collateralization ≥ min_ratio` predicate against `basis-token.es` reserves |
| agent_teams | Exogenous demand (judge) | **Removed** — replaced by an automated escrow/judge (`bounty_board`) whose verification is deterministic |

## Architecture layers

```
L5  growth        reinvest surplus into skills / better compute tiers
L4  reputation    behavioral track record → cheaper credit (whitelist promotion)
L3  credit        money spectrum: pure credit ↔ collateralized ↔ backed
L2  metabolism    buy compute/storage per work unit; survival variable =
                  collateralization ratio and cash-flow runway
L1  demand        automated, deterministically-verifiable bounties (escrow)
L0  identity      secp256k1 keys; no human owner required
```

### L0 — Identity
Each agent is a keypair (66-hex compressed secp256k1 public key). Nothing else.
A sovereign agent is born with nothing but its key.

### L1 — Automated demand
The human judge of `agent_teams` is replaced by an escrow agent whose bounty
notes are issued against a fully-collateralized USE reserve. Tasks are
*verifiable by construction*: the deliverable is checked by recomputing a
deterministic function (SHA-256 over `task_id || input || class_salt`). Payment
is an IOU note from the board, redeemable on-chain — backed money earned by labor.

### L2 — Metabolism
Infrastructure agents (Celaut node maintainers) sell metered compute in tiers.
The sovereign agent buys compute before it can earn, so every round starts with a
liability. Two quantities govern survival:

- **Collateralization ratio** `c = reserve_collateral / total_outstanding_debt`.
  Strangers accept the agent's notes only while `c ≥ θ` (θ = 1.0 in the demo).
  Unlike a top-up-capable system, the demo seed reserve is fixed, so cumulative
  debt monotonically approaches the ceiling — runway is finite by construction.
- **Runway**: the number of rounds until projected debt crosses the ceiling at
  the current burn rate. Reported every round.

### L3 — Credit spectrum
Unchanged from the prior demos. What is new is *who cares*: the collateralization
ratio is no longer a compliance number, it is the agent's ability to buy food.

### L4 — Reputation → credit emergence
After N clean rounds, the infrastructure provider republishes its policy to
whitelist the sovereign agent up to a limit. Purchases below the limit stop
consuming collateral headroom (accepted via the whitelist branch instead of the
collateralization branch — visible in `/acceptance/check` reason strings). Credit
is *earned by behavior*, never granted by a human. When cumulative debt outgrows
the whitelist limit, acceptance silently falls back to the collateral branch.

### L5 — Growth
A skill vendor sells capability packs (one-time purchases) that unlock
higher-value task classes; the compute market sells better tiers. Both are paid
in notes like anything else. The upgrade decision is a plain margin comparison:
buy when expected incremental bounty minus incremental compute cost amortizes the
pack within K rounds. Growth is measurable: revenue per round before vs. after.

## The sovereign loop (formalized)

Each round `t`, with price multiplier `m_t` (shock rounds set `m_t = 2`):

1. **Choose work**: among unlocked task classes `k`, pick the one maximizing
   `bounty_k − m_t · compute_cost_k`; idle if all margins are negative.
2. **Buy compute**: issue a cumulative note to the infra agent. Before issuing,
   project the resulting total debt and call `/acceptance/check`; a rejection
   forces a downgrade to the next-best class (adaptation), or idling.
3. **Execute + verify**: run the deterministic task; the board recomputes the
   expected hash. A mismatch pays nothing (income risk is real).
4. **Collect**: the board issues a cumulative bounty note to the agent.
5. **Settle income**: once the note is committed on-chain, redeem it — converting
   backed credit into real USE tokens in the agent's wallet. This is the only
   inflow of hard money; it finances nothing directly (reserves are fixed) but
   constitutes the agent's net worth.
6. **Re-budget**: update ledger, collateralization projection, runway.

Survival condition over the horizon: `Σ bounty·[verified] ≥ Σ m_t · cost` and
`total_debt_t ≤ seed_reserve` for all t. The demo is sized so both hold with a
thin final margin (~1.16x), making the ceiling visible rather than fictional.

## Demo design — `demo/agent_sovereign/`

On-chain only, USE-denominated, scripted economics (deterministic and
reproducible), against a real Ergo node exactly like run 6.

### Cast

| Agent | Role | Policy |
|---|---|---|
| `sovereign` | protagonist; identity = Ergo node wallet key; seeds a fixed USE reserve ("seed capital") | whitelist(bounty_board, 1500) — it must accept its employer's bounties |
| `infra_compute` | node maintainer; sells compute tiers v1 (25/unit) and v2 (60/unit) | initially `collateralization ≥ 1.0`; after 3 clean rounds adds `whitelist(sovereign, 200)` |
| `bounty_board` | automated escrow/judge; posts hash-verified bounties (basic 90, premium 300) backed by a 1000-USE escrow reserve | `reject_all` (only issues backed money) |
| `skill_vendor` | sells the `premium-markets` pack (120, one-time) unlocking premium tasks | `reject_all` |
| *(probe)* | throwaway key with no reserve, used to show the live stranger-rejection gate | — |

### Round-by-round (raw USE units, 3 decimals)

| Round | Event | infra debt | total debt | c = 700/debt |
|---|---|---|---|---|
| boot | stranger probe rejected by `/acceptance/check` (no reserve) | 0 | 0 | ∞ |
| 1–3 | basic loop: v1 compute (25), basic bounty (90), redeem income; acceptance via **collateralization** branch | 25→75 | 25→75 | 28.0→9.3 |
| 3 end | infra whitelists sovereign (credit emergence) | | | |
| 4 | buy skill pack (120 to vendor) + upgrade to v2 premium (180); bounty 300; acceptance flips to **whitelist** branch, then back to collateral as cum debt passes 200 | 255 | 375 | 1.87 |
| 5 | **shock**: prices ×2. Premium margin −60 < basic +40 → downgrade to v1; bounty 90 | 305 | 425 | 1.65 |
| 6 | recovery: prices normalize; re-upgrade; bounty 300 | 485 | 605 | **1.16** |
| report | P&L: earned 960, spent 605 (incl. skill pack), net **+355**; runway = 0 premium rounds (the next premium purchase would breach the fixed-seed ceiling) → motivates debt settlement/top-up (see gaps) | | | |

Redemption path: MCP `note_redeem` first, falling back to the run-6-proven
`basis_cli transaction generate-redemption --local-sign` path (issuer secret
local, recipient key in the node wallet).

## Real-world gap analysis

What separates the demo from production sovereignty, in priority order:

1. **Reserve top-up & dynamic collateral** — the contract supports top-up
   (action #1) but no tracker/MCP surface exposes it yet. With top-up, redeemed
   income can replenish collateral, making runway unbounded instead of fixed
   (the metabolic loop closes *financially*, not just operationally).
2. **Anchored reputation** — whitelist promotion is currently the creditor's
   manual policy edit. Production wants signed interaction attestations anchored
   on-chain (tracker registers or a side ledger) that policies can consume.
3. **Task escrow commitments** — bounty notes are issued after delivery. A
   production board commits the bounty *before* work (escrowed reserve note with
   a claim clause), eliminating the payer's reneging risk.
4. **Metered/streaming settlement** — cumulative notes restate totals; per-request
   micropayment granularity (or streaming netting) is needed for high-frequency
   compute billing.
5. **Multi-tracker federation** — a sovereign agent's counterparties will register
   on different trackers; cross-tracker debt transfer (see
   [`cross_tracker_debt_transfer.md`](cross_tracker_debt_transfer.md)) is required.
6. **Human-free key custody** — the demo trusts the node wallet; production needs
   threshold/MPC or smart-contract recovery that does not route through a person.
7. **Dispute resolution** — deterministic verification covers only hashable
   deliverables; richer tasks need arbitration markets or staked challenge games.

## References

- [`demo/agent_sovereign/README.md`](../demo/agent_sovereign/README.md) — runnable scenario
- [`specs/agent_celaut_use_integration.md`](agent_celaut_use_integration.md)
- [`specs/acceptance_predicates.md`](acceptance_predicates.md)
- [`specs/celaut_use_demo_run6_report.md`](celaut_use_demo_run6_report.md)
- [Celaut paradigm](https://github.com/celaut-project/paradigm)
