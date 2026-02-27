# Basis Protocol Specification

## Overview

Basis is a protocol for off-chain payments with on-chain redemption capabilities, built on the Ergo blockchain. It enables digital payments with credit creation, micropayments, and payments in p2p networks or areas with unstable internet connectivity.

## Main Use Cases

- Digital payments with credit creation allowed
- Payments in areas with no stable Internet connection (over mesh networks)
- Agent-to-agent payments
- Payments for content (such as 402 HTTP code processing)
- Micropayments
- Payments in p2p networks

## Design Properties

- Off-chain payments with no need to create on-chain reserves first, enabling credit creation
- Only minimally trusted trackers to track state of mutual debt off-chain, with no possibility to steal funds
- On-chain contract based redemption with prevention of double redemptions

## How It Works

### Tracker State Management

- A tracker holds **cumulative A → B debt** (as a positive ever-increasing number)
- A key-value dictionary is used to store data as `hash(A_pubkey || B_pubkey) -> totalDebt`
- The tracker periodically commits to its state by posting its digest on-chain via an **AVL tree in register R5**
- The tree stores `hash(A||B) -> totalDebt` mappings

### Payment Flow

1. To make a new payment to B, A takes the current AB record, increases cumulative debt, signs the updated record (message: `hash(A||B) || totalDebt`) and sends it to the tracker
2. The tracker verifies the note against its state, updates its internal ledger, and provides a signature on the same message
3. A sends both signatures (A's and tracker's) to B. B now holds a valid, redeemable IOU note

### Redemption Flow

1. At any moment, it is possible to redeem A's debt to B by calling the redemption action of the reserve contract
2. The contract tracks cumulative amount of debt already redeemed for each (owner, receiver) pair in an AVL tree
3. Redemption requires **BOTH** reserve owner's signature **AND** tracker's signature on message: `hash(ownerKey||receiverKey) || totalDebt`
4. The tracker signature guarantees that the off-chain state is consistent and prevents double-spending
5. Additionally, the contract verifies that `totalDebt` is committed in the tracker's AVL tree (context var #8 provides lookup proof)
6. To redeem: B contacts tracker to obtain signature on the debt note, then presents reserve owner's signature (from original IOU note) and tracker's signature to the on-chain contract along with AVL tree proofs:
   - Proof for reserve tree lookup (context var #7, optional for first redemption)
   - Proof for tracker tree lookup (context var #8, required)

### Top-up and Partial Redemption

- Always possible to top up the reserve
- To redeem partially, reserve holder can make an off-chain payment to self (A → A) updating the cumulative debt, then redeem the desired amount

## Debt Transfer (Novation)

The scheme supports transferring debt obligations between creditors with debtor consent.

### Example

A owes debt to B. B wants to buy from C. If A agrees, A's debt to B can be decreased and A's debt to C can be increased by the same amount.

### Process

1. B initiates transfer: requests to transfer amount X from debt(A→B) to debt(A→C)
2. A signs the transfer: message includes `hash(A||B)`, `hash(A||C)`, and transfer amount X
3. Tracker verifies: debt(A→B) >= X, then updates both records atomically
4. Tracker commits: posts updated AVL tree with decreased debt(A→B) and increased debt(A→C)

### Benefits

- Enables triangular trade: A→B→C becomes A→C (B is paid by debt transfer)
- Reduces need for on-chain redemption: debt can be re-assigned off-chain
- Maintains security: debtor must consent, tracker must verify and commit

## Security Analysis

### Tracker's Role

- The usual problem is that A can pay to B and then create a note from A to self and redeem. Solved by tracker solely.
- Double spending of a note is not possible by contract design (AVL tree tracks cumulative redeemed amounts)
- Tracker cannot steal funds as both owner and tracker signatures are required for redemption
- Tracker can re-order redemption transactions, potentially affecting outcome for undercollateralized notes
- Tracker can be a centralized entity or a federation

### Debt Transfer Security

- Debtor (A) must sign: prevents unauthorized transfer of debt obligation
- Tracker verifies source debt exists: prevents creating debt(A→C) without sufficient debt(A→B)
- Atomic update: both decrease(A→B) and increase(A→C) happen together or not at all
- Tracker cannot forge transfer: requires A's signature on transfer message

## Normal Workflow

1. A is willing to buy some services from B. A asks B whether debt notes (IOU) are accepted as payment. This can be done non-interactively if B publishes their acceptance predicate
2. If A's debt note is acceptable, A creates an IOU note with cumulative debt amount and signs it (signature on message: `hash(A_pubkey || B_pubkey) || totalDebt`). A sends the note to the tracker
3. The tracker verifies the note against its state, updates its internal ledger, and provides a signature on the same message. This tracker signature is required for on-chain redemption
4. A sends both signatures (A's and tracker's) to B. B now holds a valid, redeemable IOU note
5. At any time, B can redeem the debt by presenting both signatures to the reserve contract along with an AVL tree proof showing the cumulative redeemed amount. The contract verifies both signatures and ensures the redeemed amount doesn't exceed (totalDebt - alreadyRedeemed)
6. At any time, A can make another payment to B by signing a message with increased cumulative debt amount
7. A can refund by redeeming like B (in pseudonymous environments, A may have multiple keys). B should always track collateralization level and can prepare redemption transactions in advance

## Debt Transfer Workflow (Triangular Trade)

**Scenario:** A owes 10 ERG to B. B wants to buy 5 ERG worth of services from C.

1. B proposes to C that B will pay via debt transfer from A. C agrees
2. B requests transfer from tracker: decrease debt(A→B) by 5 ERG, increase debt(A→C) by 5 ERG
3. Tracker notifies A of the transfer request. A verifies the purchase (B→C) and signs approval
4. A's signature message: `hash(A||B) || hash(A||C) || 5000000000L` (transfer amount)
5. Tracker verifies: debt(A→B) >= 5 ERG, A's signature is valid
6. Tracker atomically updates: debt(A→B) -= 5 ERG, debt(A→C) += 5 ERG
7. Tracker posts updated AVL tree commitment on-chain
8. Result: B is paid (debt reduced), C is creditor (new debt created), A owes C instead of B
9. C can now redeem from A's reserve or further transfer the debt to D (with A's consent)

## System Properties

- There could be many trackers around the world - some global, some serving local trade
- The whole system could be seen as a network of different tracker-centered networks, with Ergo blockchain being a neutral global trustless financial layer
- No on-chain fees for off-chain transactions - suitable for micropayments
- Unlike other off-chain cash schemes (Lightning, Cashu/Fedimint etc), transactions can be done with no collateralization first

## Contract Specification

### Reserve Contract

#### Data (Registers)

- **R4**: Reserve owner's signing key (as a GroupElement)
- **R5**: AVL tree tracking cumulative redeemed debt per (owner, receiver) pair
  - Stores: `hash(ownerKey || receiverKey) -> cumulativeRedeemedAmount`
- **R6**: NFT ID of tracker server (bytes)

#### Actions

- **Redeem note** (#0): Spend reserve to pay out to note holder
- **Top up** (#1): Add collateral to the reserve (minimum 0.1 ERG)

#### Tracker Box Registers

- **R4**: Tracker's signing key (GroupElement)
- **R5**: AVL tree commitment to off-chain credit data
  - Stores: `hash(A_pubkey || B_pubkey) -> totalDebt`
  - This on-chain commitment allows the reserve contract to verify that the tracker is attesting to a debt amount that is actually recorded in its state
  - During redemption, context var #8 provides the AVL proof for looking up `hash(ownerKey || receiverKey)` in this tree to verify totalDebt

### Redemption Path (Action #0)

#### Context Extension Variables

- **#1**: Receiver pubkey (as a GroupElement)
- **#2**: Reserve owner's signature bytes for the debt record (Schnorr signature on `key || totalDebt`)
- **#3**: Current total debt amount (Long)
- **#5**: Proof for insertion into reserve's AVL tree (Coll[Byte])
- **#6**: Tracker's signature bytes (Schnorr signature on `key || totalDebt` or `key || totalDebt || 0L` for emergency)
- **#7**: [OPTIONAL] Proof for AVL tree lookup in reserve's tree for `hash(ownerKey||receiverKey) -> redeemedDebt`
  - Not needed for first redemption (when redeemedDebt = 0)
- **#8**: Proof for AVL tree lookup in tracker's tree for `hash(ownerKey||receiverKey) -> totalDebt` (required)

#### Validation Steps

1. **Tracker ID verification**: Verify tracker box NFT ID matches reserve's R6
2. **Tracker debt verification**: Verify totalDebt is committed in tracker's AVL tree using context var #8
3. **Reserve owner signature verification**: Verify Schnorr signature on `key || totalDebt`
4. **Tracker signature verification**: Verify Schnorr signature on `key || totalDebt` (or `key || totalDebt || 0L` for emergency)
5. **Redemption amount verification**: Ensure redeemed amount > 0 and <= (totalDebt - alreadyRedeemed)
6. **AVL tree update verification**: Verify reserve's AVL tree is properly updated with new redeemed amount
7. **Receiver signature verification**: Verify receiver's signature on transaction bytes

#### Emergency Redemption

- If tracker becomes unavailable, emergency redemption is possible after 3 days (3 * 720 blocks) from tracker creation
- Emergency redemption uses modified message format: `key || totalDebt || 0L`
- Tracker signature is still required but may be invalid (check is bypassed after timeout)
- **NOTE**: All debts associated with this tracker (both new and old) become eligible for emergency redemption simultaneously after 3 days from tracker creation

### Top-up Path (Action #1)

#### Requirements

- Reserve contract preserved (proposition bytes, tokens, R4, R6 unchanged)
- R5 (AVL tree) preserved
- At least 0.1 ERG added (100,000,000 nanoERG)

## Examples

### AI Agents Self-Sovereign Economy

1. Repo maintainer agent A looks for new issues, picks one to work on, and chooses agent candidates with needed skills (frontend, backend, testing, etc)
2. After having corresponding PR merged, A will have reward in git tokens, but doesn't have it at this point, so it reaches agents offering to accept a debt note
3. Agent B is found and agrees to make work on credit. B sends work done to A, A checks it with another agent C (paying with debt note as well) and opens a PR after all
4. When PR is merged, A gets paid in git tokens, converts them into ERG in a liquidity pool, and creates an on-chain reserve
5. B and C can now exchange promissory notes for ERG using the reserve smart contract

### Debt Transfer Example (Triangular Trade)

1. Agent A (repo maintainer) owes 10 ERG to Agent B (frontend dev) for completed work
2. Agent A needs testing work from Agent C (tester) but hasn't created reserve yet
3. Agent B needs testing work from Agent C (5 ERG worth)
4. Instead of B paying C separately, they use debt transfer:
   - B requests: transfer 5 ERG from debt(A→B) to debt(A→C)
   - A verifies B's work was satisfactory and approves the transfer
   - Tracker updates: debt(A→B) = 5 ERG, debt(A→C) = 5 ERG
5. Result: B effectively paid C using A's debt obligation. A now owes C directly
6. When A creates reserve, both B and C can redeem their respective portions
7. This creates a chain of trust: A's creditworthiness backs payments to B and C

### Digital Trading in Occasionally Connected Area

- Imagine an area which is mostly disconnected from the internet but having connection occasionally, but it has a local tracker
- Traders in the area can trade still, creating credit
- When credit limits are exceeded (i.e., no more trust could be given), on-chain reserves can be used, with redemption transactions to be collected by the tracker
- Once there is even super-slow Internet connection, tracker can send them with getting lean confirmations via NiPoPoWs (similarly to https://www.ergoforum.org/t/e-mail-client-for-limited-or-blocked-internet/134)

## Possible Extensions

- Multiple tracker support via AVL tree in R6
- Cross-tracker payments
- Automated reserve creation based on debt thresholds
- Integration with mesh network protocols
