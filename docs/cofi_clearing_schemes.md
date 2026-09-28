# Credit-Clearing and Netting Schemes from the Collaborative Finance Community, and Their Implementation on Basis

Source: full message history of the Telegram group "Collaborative Finance Official"
(`-1001363683628`, 315 participants, description "Credit where credit's due"),
8,127 messages spanning 2023-01-18 → 2026-09-02, exported 2026-09-02 via a
read-only Telethon session. Message references below (`[NNNNN]`) are Telegram
message ids in that group. The raw export and a keyword-filtered extract live in
the marketing repo under `raw/` (`cofi-export.json`, `cofi-netting-extract.md`).

Regular voices on clearing mechanics: Tomaž Fleischman (Informal Systems /
Cycles, Slovenian obligation-clearing research), Matthew Slater "matslats"
(Credit Commons), dil green (Mutual Credit Services), Will Ruddick (Grassroots
Economics / Sarafu), Hans-Florian Hoyer (TwinToken), Michiel de Jong
(LedgerLoops / cycles.money), Johan Nygren (Resilience / p2p-novation), Alex
Kampa (Sikoba), Michel Rauchs, Robin (GrowOperative/FOAF).

---

## 1. Scheme families discussed in the group

### 1.1 Obligation clearing / MTCS (Fleischman–Dini research line)

Obligations (invoices, payables/receivables) are edges in a directed graph;
clearing finds closed loops and settles them atomically, so only net residuals
move. This is the Slovenian government MTCS/TETRIS practice (max-flow netting of
outstanding invoices) and the JRFM papers (mdpi.com/1911-8074/13/12/295,
doi.org/10.3390/jrfm14090452) that the group treats as foundational [100, 170,
187]. Gridlock removal is framed as reaching "MATS — Maximum Amount To Settle",
eliminating every node whose input depends on its output [1373]. Combining
clearing with a mutual-credit liquidity source "doubles the effectiveness of
obligation-clearing" [178]. Tomaž's summary of the policy dimension [12527]:
"Unresolved legit cycle is systemic risk, resolved legit cycle is liquidity
saving and a form of endogenous liquidity. Resolved illicit cycle is money
laundering." Bosnia's MTCS was banned by the IMF after abuse; Slovenia shares
results with tax authorities.

### 1.2 Cycles Protocol (Buchman / Fleischman / Informal Systems spinout)

The most-discussed production system in 2026. Verified obligations become edges
of a common clearing graph; multilateral set-off settles closed chains
atomically, complementing (not replacing) CCPs and banks [11041]. Distinctive
features:

- **Privacy by default**: the obligation graph stays private; a solver analyses
  it inside a TEE with zero-knowledge proofs, emitting verifiable proofs that
  clearing followed the ruleset without revealing indirect participants
  [11309]. Motivation: itemized invoice data is MNPI/GDPR-sensitive; an open
  graph enables price undercutting, supplier poaching, and predatory behaviour
  toward liquidity-constrained firms [11309, 11312, 11324].
- **Acceptances**: participants pre-declare what they are willing to be owed
  (asset, amount band, counterparty quality — e.g. "up to $5k USDC at
  $0.98–1.02; JPM/BoA deposits up to $100k, not Revolut"), which "significantly
  expands the potential number and length of cycles in the graph" [11327,
  confirmed 11328].
- Universal obligation language: any balance-sheet asset/liability as edges
  [11324].
- Traction: $6.4M round (Blockchange, Coinbase Ventures, Compound VC) for an
  "open clearing network for crypto markets" [11401]; ERP/e-invoicing/factoring
  integration named as the key B2B adoption blocker [12284, 12285].
- Paper: arxiv.org/pdf/2605.02436 [11329, 11330].

### 1.3 TwinToken Clearing Community (Hans-Florian Hoyer)

Every sale on credit mints a cryptographically linked token pair: a credit
token in the creditor's wallet and a debit token in the debtor's wallet,
encoding amount, due date, parties. Pairs circulate as means of payment until a
synchronized clearing day, when the community builds the clearing matrix —
M(i,j) is entered only if buyer i and seller j report matching values
independently — computes each wallet's Net Internal Debt, and settles only
residuals [9508, 9566, 10003, 11321, 11375]. "Clearing takes place in the
wallets, not the ledgers or the matrix" [10003]. Claimed scaling: N
participants need at most N·(N−1) payments without clearing, half with mutual
compensation, N with the community as intermediary [9582]. Empirical test on
generated data (6 communities, 600 actors, ~2,000 contracts): single-hop cash =
2,000 payments; clearing = 599 hops moving 50% of volume; tokenized twins =
1,694 swaps using no currency [9627, 9642–9644]. The matrix is legally framed
as *information only* — not balances, money, or securities [11826]. Historical
anchor: medieval fair clearing ("scontration" = set-off among more than two
[1617]), split tally sticks, London Bankers' Clearing.

### 1.4 Credit Commons / Mutual Credit Services (matslats, dil green)

Nested tree of ledgers: leaves are persons, twigs their mutual-credit groups,
branches groups of groups; "privacy and autonomy are strongest at the leaf,
groups only see trunkward" [4300, 4301]. Per-issuer voucher ledgers (one
negative issuer account + holder accounts), pool ledgers with per-issuer
exchange rates and issuance limits, and **multilateral set-off between voucher
issuers via a loop-finding algorithm** so issuers can redeem stuck holdings of
each other's vouchers and recover issuance capacity [2159, 2161]. Protocol v0.9
ready [11360]; V1 transport is HTTP request/response between nodes with
bilateral hash chains (balances are computed from history, so tamper-evident
history is the integrity mechanism) [9904–9961]. Also: "credit clearing as a
service, incorporate mutual credit later" [11926, 11927].

### 1.5 Commitment pooling / SwapPools (Will Ruddick)

A SwapPool is "a programmable escrow account that can hold multiple assets and
have rules for enabled exchanges"; the network of pools *is* the obligation
matrix and a routing backend [1769]. With full on-chain information about
voucher issuers and liabilities, "clearing is possible without a second
record-keeping token" — multilateral netting of aggregated obligations into one
net payment per participant [1624, 1634]. Explicit business idea: "a service
that pulls all the Blockchain data and offers obligation clearing" [1854].
Matching via order-book / math-trade algorithms [2000, 2017].

### 1.6 Johan Nygren's line: triad novation, Resilience, Loopsilience

- **Triad (3-party) novation**: reduce clearing to a single operation among
  exactly three mutually trusting parties, membership-less, running on
  trustlines with a periodic "novation finding" routine; ~500 LOC
  (gitlab.com/bipedaljoe/p2p-novation) [9814–9854]. Proposed as the routing
  core of "a simpler Lightning Network" with staggered deposits instead of
  chained timeouts [9971–9973, 12444].
- **Resilience**: multi-hop payments with a gradual-penalty commit protocol
  (resilience.me/3phase.pdf), which he presents as solving Ryan Fugger's 2006
  multihop coordination problem [8851, 9130, 12541]; tax redistribution over
  the trust graph performs multilateral clearing automatically ("the tax starts
  to loop") [9746–9753].
- **Loopsilience**: standalone multilateral barter, ~300 LOC, a single
  `transfer` command doing "multilateral debt clearing via tax redistribution"
  [9746–9771].

### 1.7 GrowOperative / FOAF (Robin)

Mutual-credit marketplace for local food: bilateral trustlines (each side sets
the other's limit), multi-hop routing over the trust graph, and "credloop
detection — finding the cycles that net to zero so nobody actually has to pay"
[11360]. Seasonal example: fruit stands cross-stock, "credit loops form around
the ring and cancel each other down, and at the end of the season they pay out
whatever small remainder is left in cash" [12258]. Converged with matslats on
the formula **"bilateral trust, multilateral clearing"**: trustlines are
bilateral, but multi-hop clearing needs a shared unit and shared rules — one
logical (distributed, integrity-checked) ledger, not global consensus [12122,
12126, 12129]. Target architecture keeps bilateral state off-server with
probe-based loop discovery; open question: atomic commit when loop nodes are
offline (2PC? escrow?) [11360].

### 1.8 Other mentions

Greco's "Credit Clearing Cycle" / RVU mutual credit (credit limits as % of
forecast turnover) [9251, 10210–10222]; Sikoba IOU platform [19, 5773];
Komunitin (currency-to-currency trustlines on Stellar, CES adopting its code)
[11374]; LedgerLoops chunked clearing [9564]; math-trade/max-flow tooling
(github.com/gioannidis/mathtrader, TradeMaximizer) [2000, 2001]; Sardex, WIR,
Local Loop Merseyside as working precedents [11323, 12355]; People's Clearing
House (Mexico) on Interledger [4929]; Russia/China/Iran sanctions-era
multilateral barter with the dollar as mere numeraire [12523, 12524].

### 1.9 Recurring open problems in the group

- **Privacy of the obligation graph** vs transparency (the central 2026
  debate): ZKP+TEE (Cycles) vs information-only matrix (TwinToken) vs
  deliberate partial transparency to keep value in the community [11309–11327].
- **Defaults and credit assessment**: "clearing is not merely processing of
  ledger data" — participants have preferences over which obligations they
  hold; insurance/default pools needed (Kampa [10016]; Chris Cook's guarantee
  society [9773]; "insurance is just a contract that if X happens the
  signatories pay Y — no pool needed" (matslats [6456])).
- **Consent to clearing outcomes**: medieval fairs let merchants refuse cycles
  whose resulting exposure they disliked; some fairs forced acceptance [11301].
- **Atomicity and the do-nothing intermediary**: multihop coordination fails
  when an intermediary stalls; fixes are timeouts, locked liquidity (attack
  vector), or gradual penalties [12541].
- **Mesh vs tree federation** of clearing networks [11364, 7962].

Basis itself was presented in the group on 2026-09-01/02 [12535, 12536, 12543];
Johan criticized it (like Cycles) as reintroducing central intermediaries
[12537, 12539]; the stated open problems were double-spend prevention and
redemption-order preservation under pseudonymity (a reserve owner can drain
its own reserve first) [12543].

---

## 2. Mapping onto Basis

Basis primitives correspond one-to-one with the vocabulary above:

| CoFi concept | Basis mechanism | Status |
|---|---|---|
| Bilateral trustline + credit limit | Per-recipient acceptance predicates (`max_debt`, whitelist, collateralization floor) — functionally identical to Cycles' "acceptances" | Specified (`specs/acceptance_predicates.md`); acceptance-policy machinery in server |
| Bilateral clearing balance | Cumulative IOU note `hash(A‖B) → totalDebt`, one updatable note per pair, off-chain, tracker-countersigned | Implemented |
| Triad novation (Johan) | Debt transfer: debtor signs `hash(A‖B)‖hash(A‖C)‖amount` (72-byte message); tracker decreases debt(A→B) and increases debt(A→C) atomically | **Specified, not implemented** (`specs/REMAINING_ENHANCEMENTS.md` Enhancement 1, est. 2–3 days) |
| Loop/cycle clearing (Cycles, credloops, LedgerLoops) | The tracker sees the whole obligation graph → run cycle detection or max-flow (TETRIS/MTCS-style), execute each loop as an atomic chain of novations; the AVL+ root committed on Ergo makes every clearing run publicly verifiable | Simulated demo only (`demo/basis/circular/calculate_netting.py`, `settle_netting.py`); "automatic netting cycles" is a listed gap (`specs/informal_clearing_systems.md` §5) |
| TwinToken clearing day | Tracker snapshots already provide the periodic rhythm; at period end, run a multilateral novation burst to net positions, then settle residuals on-chain against reserves. A signed note *is* the credit/debit twin — one object both sides can verify | Needs the netting-cycle extension |
| Clearing matrix as pure information | Tracker state is exactly the matrix; its on-chain commitment is a hash root, so the chain itself publishes no trade detail | Implemented |
| Commitment pools / SwapPools | A pool account holding notes of many issuers; exchanges executed by novation; token-backed reserve (`contract/basis-token.es`) as the pool's settlement asset | Reserves implemented; pool logic is app-layer |
| Credit Commons tree federation | Cross-tracker federation: gateway entities holding reserves recognized by two trackers (`specs/cross_tracker_debt_transfer.md` options 1–3) | Off-chain novation spec'd; multi-tracker reserves future |
| Final settlement in outside money | On-chain redemption against ERG/token reserves, with double-redemption prevented by the reserve's cumulative-redeemed AVL tree | Implemented (`contract/basis.es`, `contract/basis-token.es`) |

### 2.1 Reference implementation path for a clearing cycle

1. Participants issue/accept notes as usual during the period (acceptance
   predicates = acceptances).
2. At period end, a clearing service reads the tracker's note state via the
   HTTP API (`openapi.yaml`: `/notes`, `/notes/state`), builds the obligation
   graph, and runs netting: cycle cancellation first (zero-net loops), then
   greedy debtor→creditor matching for residuals — the two stages already
   sketched in `demo/basis/circular/calculate_netting.py`.
3. The clearing service submits the resulting set of debt transfers; each
   debtor signs its novations (consent = the medieval-fair refusal right).
4. The tracker applies the approved transfers atomically and commits the new
   AVL+ root on-chain — a verifiable, timestamped record of the clearing run.
5. Residual net debtors' creditors redeem on-chain against reserves, or roll
   into the next cycle.

### 2.2 Where the group's critique lands on Basis

- **Tracker centrality**: Basis novations are tracker-atomic, which sidesteps
  the offline-node 2PC problem and the do-nothing-intermediary problem — at
  the cost of the central-operator trust Johan objects to. The honest framing:
  a Basis tracker is a *Cycles-style central solver with on-chain
  verifiability and built-in exit* (emergency redemption from the last
  committed state after ~3 days; tracker cannot steal or forge transfers), not
  a mesh protocol.
- **Privacy**: the tracker operator sees the full graph; pubkey pairs are only
  pseudonymous. Cycles' ZKP+TEE solver is ahead here. Possible Basis
  directions: TEE-based tracker operation, or separating the clearing-solver
  role from the ledger-keeper role so the solver learns only what the proofs
  require.
- **Defaults/credit assessment**: acceptance predicates cap exposure but do not
  assess creditworthiness; the group's consensus is that this is
  social/institutional, and Basis correctly leaves it to the policy layer.
  Insurance/default-pool overlays remain an open design space.
- **Redemption ordering under pseudonymity**: self-drain attack is a known open
  problem (stated in the group at [12543]); the FIFO redemption queue is
  spec'd (`specs/redemption_acceptance_policy.md`) but does not fully close it.

---

## 3. References

Group-internal: message ids cited inline; raw export at
`marketing/raw/cofi-export.json` (not public — private group content).

Basis: `specs/spec.md` (Debt Transfer), `specs/informal_clearing_systems.md`,
`specs/acceptance_predicates.md`, `specs/redemption_acceptance_policy.md`,
`specs/cross_tracker_debt_transfer.md`, `specs/REMAINING_ENHANCEMENTS.md`
(Enhancement 1), `demo/basis/circular/`, `contract/basis.es`,
`contract/basis-token.es`, `openapi.yaml`.

External (as cited in the group): Fleischman et al. JRFM 13(12):295 and
14(9):452; Cycles paper arxiv.org/pdf/2605.02436; Hoyer, "Slipchain"
(academia.edu/88801028); resilience.me/3phase.pdf;
gitlab.com/bipedaljoe/p2p-novation and .../loopsilience;
github.com/gioannidis/mathtrader; matslats.net/ripple-reciprocation-credit-commons.
