# Basis in Hawala, Parallel Settlement, and Informal Clearing Systems

## Executive Summary

Basis is an off-chain IOU-note protocol with optional on-chain Ergo reserves. Its
building blocks — issuer-signed cumulative debt notes, a tracker that commits
state to an AVL+ tree, per-recipient acceptance policies, and on-chain
redemption through `contract/basis.es` / `contract/basis-token.es` — map
naturally onto informal value-transfer and clearing systems that have existed
for centuries.

This document explains how Basis can be understood as, and extended into:

* a **digital Hawala/Hundi layer**, where brokers replace oral promises with
cryptographically signed IOUs;
* a **parallel settlement infrastructure** that operates alongside conventional
correspondent banking and can serve corridors that are de-risked,
underbanked, or prohibitively expensive for small transfers;
* a **regional clearing platform**, where banks or trade entities in different
jurisdictions clear bilateral balances through token-backed reserves and
tracker federation;
* a **community clearing network** (LETS, mutual credit, agent economies) with
programmable trust rules instead of institutional gatekeepers.

The emphasis is on settlement mechanics: how value is issued, circulated,
netted, and finally redeemed or settled. The protocol is neutral infrastructure;
its participants decide whom to trust through acceptance predicates, collateral
requirements, and reputation systems.

---

## 1. Hawala Architecture and Basis Mapping

### 1.1 Classic Hawala Mechanics

In a simple Hawala transfer:

1. A **sender** in country A gives cash to **hawaladar A**.
2. Hawaladar A instructs his counterpart **hawaladar B** in country B to pay the
   **recipient** an equivalent amount.
3. Hawaladar B pays the recipient from his own cash pool.
4. Over time, A and B settle their bilateral position through goods, gold,
   bank wires, or reciprocal flows. No actual money moves during the original
   payment.

The system works on trust, reputation, and informal netting between brokers.
Records may be oral, paper-based, or maintained only in the broker's ledger.

### 1.2 Mapping Hawala Roles to Basis

| Hawala Concept | Basis Equivalent | Implementation |
|---|---|---|
| Hawaladar A (debtor broker) | Issuer / reserve owner | Signs cumulative IOU note to another broker |
| Hawaladar B (creditor broker) | Recipient / creditor | Holds signed note redeemable from issuer's reserve |
| Informal broker ledger | Tracker server | Stores `hash(issuer\|\|recipient) → totalDebt` in AVL+ tree |
| Cash pool / collateral | On-chain reserve | `basis.es` reserve box backed by ERG or `basis-token.es` box backed by a custom token |
| Broker reputation | Acceptance policy | Whitelist, blacklist, collateralization predicates |
| Settlement / netting | Debt transfer (novation) | Transfer part of A→B debt to A→C with issuer consent |

A Hawala broker can run a Basis tracker and issue signed notes to his
counterparties. The note `A → B` with cumulative debt `D` and timestamp `T` is
exactly the Hawala "I owe you" between the two brokers, but it is:

* **Cryptographically signed** by the issuer (Schnorr signature over
  `key || totalDebt || timestamp`, see `specs/SCHNORR_SIGNATURE_SPEC.md`);
* **Countersigned** by the tracker, making it redeemable on-chain;
* **Committed on-chain** through the tracker's AVL+ tree root digest in a tracker
  box (`specs/server/tracker_box_update_spec.md`).

### 1.3 From Oral Trust to Collateralized Trust

Classic Hawala relies on reputation and kinship networks; settlement failure is
settled socially or through community pressure. Basis preserves the reputation
dimension through acceptance predicates, but adds an optional collateral layer:
a broker can lock funds in a reserve box so that creditors can redeem notes
directly on-chain if off-chain settlement fails.

This does not eliminate trust — creditors must still trust the tracker and the
issuer's willingness to maintain collateral — but it replaces some social
enforcement with cryptographic and economic enforcement:

* **Double-redemption is impossible**: the reserve contract tracks
  `hash(owner||receiver) → cumulativeRedeemedAmount` in its own AVL tree
  (`specs/spec.md`).
* **Tracker cannot steal**: redemption requires both the issuer's signature and
  the tracker's signature.
* **Tracker cannot censor forever**: after the refund time lock, the reserve
  owner can exit, and emergency redemption uses the last committed tracker state
  (`contract/basis.es`).

### 1.4 Multilateral Netting via Debt Transfer

Hawala brokers settle positions through bilateral or multilateral netting.
Basis supports this directly through **debt transfer** (novation):

* A owes B 10 ERG.
* B wants to pay C 5 ERG.
* With A's signature, the tracker atomically decreases `A→B` by 5 ERG and
  increases `A→C` by 5 ERG.

The result is triangular settlement without on-chain transactions, exactly the
kind of netting Hawala brokers perform over days or weeks, but in real time and
with a verifiable audit trail (`specs/spec.md`, "Debt Transfer").

---

## 2. Parallel Settlement Infrastructure

### 2.1 What "Parallel Settlement" Means

Conventional cross-border settlement depends on correspondent banking: bank A
holds an account at bank B, which holds an account at a clearing bank in the
currency's home jurisdiction. When this chain breaks — because a bank is
de-risked, a corridor is too small, or fees make retail transfers uneconomic —
payment flows move into informal channels.

Basis offers a **parallel settlement layer** that sits next to this system:

* **Issuance** happens off-chain, so small-value notes are not bottlenecked by
  on-chain fees or bank working hours.
* **Circulation** happens through trackers that any operator can run, avoiding
  single points of failure.
* **Redemption** settles against on-chain reserves on Ergo, a public blockchain
  that is itself outside conventional banking rails.
* **Debt transfer** lets obligations circulate and net before anyone touches a
  bank account or blockchain transaction.

This is not a replacement of formal banking; it is an alternative rail that can
interconnect with banks at on/off-ramp points (reserve top-ups, redemptions to
banked addresses) while operating independently in between.

### 2.2 Settlement Node as Tracker Operator

A Hawaladar, community treasury, or remittance cooperative can run a Basis
tracker as a **settlement node**. The node:

1. Accepts signed IOU notes from issuers it recognizes.
2. Maintains the cumulative-debt ledger.
3. Commits the ledger state on-chain periodically.
4. Provides redemption signatures to creditors whose notes meet its policy.
5. Participates in debt-transfer netting with other nodes.

Each node sets its own acceptance policy. A node may accept:

* **Pure credit** from long-standing counterparties (whitelist with `max_debt`).
* **Fully collateralized** notes from new issuers.
* **Fractionally collateralized** notes when the collateralization ratio is above
  a configured floor.

This mirrors how correspondent banks choose which counterparties to hold
balances with and under what terms.

### 2.3 On-Chain Reserves as Settlement Finality

In conventional banking, final settlement occurs in central-bank money or a
major correspondent account. In Basis, final settlement occurs on the Ergo
blockchain when a creditor redeems a note against a reserve.

Two reserve variants exist:

* `contract/basis.es` — backed by ERG.
* `contract/basis-token.es` — backed by a custom token.

A settlement node can collateralize notes in ERG, in a local stable-token, or in
a commodity-backed token (gold, silver). The reserve contract enforces the same
four actions regardless of backing: redeem, top-up, initiate refund, complete
refund (`specs/basis_protocol.md`).

The two-phase refund (initiate, wait ~2 months, complete) gives creditors a
window to redeem before the owner can withdraw collateral, functioning like a
settlement notice period.

### 2.4 Settlement Records and Auditability

Every note is signed, every tracker state is committed on-chain, and every
reserve redemption updates an on-chain AVL tree. This gives parallel settlement
a property that informal systems traditionally lack: **immutable, verifiable
records**. Participants can:

* Prove a note exists at a given tracker state.
* Prove a redemption was authorized.
* Reconstruct the cumulative debt history for any `(issuer, recipient)` pair.

Trackers publish events (note updates, redemptions, top-ups, commitments) over
NOSTR (`specs/basis_protocol.md`), creating a public audit feed that any
counterparty can monitor without trusting the node operator.

---

## 3. Regional Clearing Platforms

### 3.1 What a Regional Clearing Platform Does

Regional clearing platforms let banks, trade entities, or treasury vehicles in
different jurisdictions clear trade balances without routing every payment
through the global correspondent-banking network. A typical platform provides:

* **Bilateral or multilateral trade-balance tracking.**
* **Non-cash netting**, so only the net difference is settled periodically.
* **A settlement asset** acceptable to all participants, often a trade currency,
  commodity, or tokenized unit.
* **Periodic reconciliation** to true-up balances.

These platforms arise naturally when conventional rails are expensive,
de-risked, or simply unavailable for a corridor — for example, trade settlement
between countries that do not share a major correspondent-bank relationship.

### 3.2 Basis Mapping for Regional Clearing

| Regional Platform Concept | Basis Equivalent | Implementation |
|---|---|---|
| Participating bank / trade entity | Reserve owner and issuer | Locks collateral, issues notes to counterparties |
| Bilateral clearing balance | Cumulative `issuer → recipient` IOU note | One note per counterparty pair |
| Settlement asset | Token-backed reserve (`contract/basis-token.es`) | Trade token, stable unit, or commodity token |
| Shared clearing ledger | Tracker or federation of trackers | AVL+ tree commitments on Ergo |
| Periodic reconciliation | Debt transfer + on-chain redemption | Netting via novation, final settlement on-chain |

A Russian importer and a Chinese exporter, for instance, could settle through a
regional tracker: the importer's bank issues notes to the exporter's bank,
tracker events record the accumulating balance, and at the end of a clearing
cycle the net position is either settled through debt transfer to a third
participant or redeemed against a token-backed reserve.

### 3.3 Cross-Border Federation Pattern

A single national tracker creates a centralization risk and a jurisdiction
bottleneck. A more robust design uses a **federation of trackers**:

* Each country or economic bloc runs its own tracker.
* Gateway entities are trusted by multiple trackers and hold reserves recognized
  on both sides.
* Cross-tracker debt recognition lets obligations circulate between
  jurisdictions without every transaction hitting the blockchain.

This is the regional-platform equivalent of inter-clearinghouse links. Basis's
future cross-tracker federation and multi-tracker reserve extensions
(`specs/basis_protocol.md`) are the direct technical path to this architecture.

### 3.4 Netting and Settlement Cycles

Regional platforms operate in cycles:

1. **Intra-cycle:** participants issue and accept notes, building bilateral
   balances. Debt transfer can re-route obligations to optimize the network.
2. **End-of-cycle:** the tracker computes net positions per participant.
3. **Settlement:** net creditors redeem notes against token reserves; net debtors
   top up reserves or roll balances into the next cycle.

The tracker's FIFO fallback and collateralization checks
(`specs/redemption_acceptance_policy.md`) provide orderly settlement even when a
reserve becomes distressed.

### 3.5 Technical Trade-Offs

* **Auditability vs. confidentiality.** Signed notes and on-chain commitments
  create a verifiable record, which is valuable for reconciliation but may
  conflict with the desire to keep trade flows private.
* **Reserve denomination.** A single trade token is simplest; a basket of
  currencies or commodities better reflects multi-country trade but requires
  pricing oracles.
* **Tracker trust model.** A single operator is easiest to deploy; a federation
  or sidechain reduces jurisdiction risk but adds coordination complexity.

---

## 4. Other Informal Clearing Systems

### 4.1 LETS and Mutual Credit

A Local Exchange Trading System (LETS) is a closed community in which members
extend credit to each other and balances sum to zero. Basis already models this
with the LETS demo (`specs/tui_wallet_lets.md`):

* Members whitelist each other.
* `max_debt` caps negative balances.
* No reserves are required; notes circulate on pure trust.

The same architecture can scale to larger informal clearing circles: villages,
cooperatives, diaspora associations, or supply-chain networks.

### 4.2 Community Currencies

A community can issue a token-backed reserve (`contract/basis-token.es`) and
run a local tracker. The token serves as the community's unit of account while
Basis provides the clearing machinery. Because notes are off-chain, the system
can run on minimal infrastructure and even mesh networks with only occasional
connectivity to the global Ergo chain.

### 4.3 Agentic and Machine-to-Machine Clearing

Autonomous agents can use Basis as a settlement layer for resource sharing,
compute markets, and service payments. The acceptance policy becomes a
machine-readable trust rule: an agent accepts notes only from agents or
managers that meet its collateral or whitelist conditions. This is already
demonstrated in `demo/agent_teams/` and `demo/agent_coop/`
(`docs/AGENT_INTERFACE.md`).

### 4.4 Cross-Tracker Gateways

Multiple trackers can be linked through **multi-tracker reserves** or cross-tracker
debt recognition. A broker who is trusted by two trackers can act as a bridge:
he holds a reserve recognized by tracker A and issues notes accepted by tracker
B. This is the Basis equivalent of inter-clearinghouse settlement and is listed
as a future extension in `specs/basis_protocol.md`.

---

## 5. Protocol Extensions Required

To mature Basis as an informal clearing platform, the following extensions are
useful:

| Extension | Purpose | Status |
|---|---|---|
| **Token-backed reserves** | Collateral in local currencies, stable tokens, or commodities | Implemented (`contract/basis-token.es`) |
| **Cross-tracker federation** | Inter-clearinghouse settlement and trust delegation | Future (`specs/basis_protocol.md`) |
| **Cross-tracker state proofs** | Prove a note exists in another tracker's AVL tree | Not implemented |
| **Multi-tracker reserves** | Gateways between tracker networks | Future |
| **Multi-currency / basket reserves** | Denominate collateral in trade-currency baskets | Not implemented |
| **Identity/reputation predicates** | Acceptance based on verifiable credentials or history | Not implemented |
| **Offline/mesh sync** | Notes propagate without live internet, reconcile later | Not implemented |
| **Stealth redemption addresses** | Privacy for final payout | Future (`specs/basis_protocol.md`) |
| **Audit exports** | Operator-readable settlement reports from tracker events | Not implemented |
| **Automatic netting cycles** | Periodic multilateral debt transfer across many parties | Not implemented |
| **Privacy-preserving balance reporting** | Reveal net positions without exposing individual trades | Not implemented |

---

## 6. Operational Considerations

### 6.1 Capital and Backing

A settlement node that issues notes must decide on backing strategy:

* **Fully backed**: every issued note is covered by on-chain collateral. Lowest
trust requirement, highest capital cost.
* **Fractionally backed**: collateral covers a fraction of liabilities, with
acceptance predicates enforcing floors. Matches historical free-banking
practice (`docs/free_banking.md`).
* **Pure credit**: no collateral; relies entirely on whitelist and reputation.
Lowest cost, highest trust requirement.

### 6.2 Redemption Ordering and Distress

When a reserve is undercollateralized, the tracker can enforce a FIFO queue:
only the oldest outstanding note may redeem (`specs/redemption_acceptance_policy.md`).
This converts a disorderly run into an orderly settlement queue. On-chain,
first-come-first-served still applies if the tracker is bypassed.

### 6.3 Refund Lockup as Settlement Notice

The reserve owner's two-phase refund (`specs/basis_protocol.md`) acts like a
settlement notice. After initiating a refund, the owner must wait ~2 months
before withdrawing collateral, giving creditors time to redeem or negotiate
off-chain settlement.

### 6.4 On/Off Ramps

Parallel settlement is most useful when it can connect to conventional finance
at the edges:

* **On-ramp**: a broker deposits fiat/bank funds, buys ERG or a reserve token,
  tops up a reserve, and issues notes.
* **Off-ramp**: a creditor redeems a note to an on-chain address, then a local
  broker or exchange converts to fiat.

Basis does not prescribe these interfaces; they are business relationships built
around the protocol.

---

## 7. Implementation Roadmap

1. **Document and demonstrate Hawala mapping**
   - Add acceptance-policy examples for broker networks.
   - Write a tutorial showing debt transfer between three brokers.

2. **Extend token-reserve experiments**
   - Test `contract/basis-token.es` with community-currency tokens.
   - Evaluate pegging and redemption UX.

3. **Prototype regional clearing platform**
   - Deploy token-backed reserves for a bilateral trade corridor.
   - Demonstrate cross-tracker balance netting and end-of-cycle settlement.

4. **Prototype cross-tracker settlement**
   - Design federation protocol (Oracle Pool or Rosen-style).
   - Implement multi-tracker reserve or gateway contract.

5. **Add operator tooling**
   - Audit exports from tracker event store.
   - Netting-cycle automation for broker networks.

6. **Pilot with a real informal clearing use case**
   - Diaspora remittance circle, community currency, regional trade corridor, or
     agent economy.

---

## 8. References

* `specs/basis_protocol.md` — Basis protocol overview and contract actions.
* `specs/spec.md` — Detailed payment, redemption, and debt-transfer flows.
* `specs/acceptance_predicates.md` — Acceptance policy language.
* `specs/redemption_acceptance_policy.md` — Redemption-time policy checks and
  FIFO fallback.
* `docs/free_banking.md` — Free-banking analogy for Basis.
* `specs/tui_wallet_lets.md` — LETS mutual-credit mapping.
* `docs/AGENT_INTERFACE.md` — Machine-readable wallet and MCP interfaces.
* `contract/basis.es` — ERG-backed reserve contract.
* `contract/basis-token.es` — Token-backed reserve contract.
* `specs/SCHNORR_SIGNATURE_SPEC.md` — Signature format used for IOU notes.
* [Russia and China’s Regional Clearing Platforms](https://fincrimecentral.com/russia-china-clearing-platforms-sanctions/) —
  Example of bilateral regional clearing architecture outside conventional
  correspondent banking.
