# SilverCents Demo Implementation

## Overview

SilverCents is a demonstration of the Basis protocol applied to silver-backed offchain cash. This implementation shows how constitutional silver coins (dimes and quarters from 1946-1964) can back digital notes traded using the Ergo blockchain and Basis Tracker system.

## Concept

**SilverCents** are on-chain tokens in a cryptocurrency run on the Ergo Platform, using the Basis protocol. SilverCents are exchangeable one-for-one with constitutional silver dimes and quarters that are suitable for circulation. There are billions of these coins distributed widely throughout the USA. The point of exchange is at the point of business of the vendors that participate in the SilverCents economy.

### Key Features

- **Physical Backing**: Each SilverCent note is backed 1:1 with physical constitutional silver coins (90% silver content)
- **Denominations**: 
  - Silver Dime (SC-D): 1.00 ERG = 0.0723 troy oz silver
  - Silver Quarter (SC-Q): 2.50 ERG = 0.1808 troy oz silver
- **Offchain Efficiency**: Notes are created and transferred offchain with zero fees
- **On-Chain Settlement**: Only redemptions touch the blockchain after 1-week maturation
- **Community Currency**: Designed for local economies and vendor-customer relationships

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     SILVERCENTS ECOSYSTEM                    │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────┐         Issues Note         ┌──────────┐      │
│  │  VENDOR  │ ─────────────────────────> │ CUSTOMER │      │
│  │  (Alice) │    (10 dimes = 10 ERG)     │   (Bob)  │      │
│  └──────────┘                              └──────────┘      │
│       │                                          │           │
│       │                                          │           │
│       │  Holds Physical                   Redeems Note      │
│       │  Silver Coins                     After 1 Week      │
│       │  (Dimes/Quarters)                       │           │
│       │                                          │           │
│       v                                          v           │
│  ┌──────────────────────────────────────────────────┐       │
│  │            BASIS TRACKER (Off-Chain)             │       │
│  │  • Stores note relationships (Alice -> Bob)      │       │
│  │  • Tracks amounts and timestamps                 │       │
│  │  • Signs updates with Schnorr signatures         │       │
│  │  • Commits digests to blockchain                 │       │
│  └──────────────────────────────────────────────────┘       │
│                           │                                  │
│                           v                                  │
│  ┌──────────────────────────────────────────────────┐       │
│  │        ERGO BLOCKCHAIN (On-Chain Reserves)       │       │
│  │  • Stores Alice's ERG reserves                   │       │
│  │  • Validates redemption proofs (AVL trees)       │       │
│  │  • Prevents double-spending                      │       │
│  │  • Enforces 1-week maturation period             │       │
│  └──────────────────────────────────────────────────┘       │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## CLI Commands

### 1. Show System Information

```bash
basis-cli silvercents info
```

Displays:
- SilverCents concept and purpose
- Denomination details (dimes and quarters)
- Technical implementation details
- Backing calculations (silver content)
- Usage instructions

### 2. Issue Silver-Backed Note

```bash
basis-cli silvercents issue \
  --recipient <customer_pubkey> \
  --dimes 10 \
  --quarters 4 \
  --description "Payment for groceries"
```

**Example Output:**
```
╔════════════════════════════════════════════════════════════════╗
║                  ISSUING SILVERCENTS NOTE                      ║
╚════════════════════════════════════════════════════════════════╝

📝 Note Details:
   Issuer (Vendor):  02a7b4c...
   Recipient:        03f1e2d...
   Description:      Payment for groceries

💰 Denominations:
   • 10 Silver Dimes   → 10 ERG
   • 4 Silver Quarters → 10 ERG
   ─────────────────────────────────
   Total Value:        20 ERG
   Silver Content:     1.4460 troy oz

📊 Vendor Reserve Status:
   On-chain Reserve: 100 ERG
   Outstanding Debt: 45 ERG
   Collateral Ratio: 222.2%

✅ SilverCents note issued successfully!
```

### 3. Redeem Note for Physical Silver

```bash
basis-cli silvercents redeem \
  --issuer <vendor_pubkey> \
  --dimes 5 \
  --quarters 2
```

**Example Output:**
```
╔════════════════════════════════════════════════════════════════╗
║                REDEEMING SILVERCENTS NOTE                      ║
╚════════════════════════════════════════════════════════════════╝

📝 Redemption Details:
   Issuer (Vendor):  02a7b4c...
   Recipient (You):  03f1e2d...

💰 Redeeming:
   • 5 Silver Dimes   → 5 physical dime coins
   • 2 Silver Quarters → 2 physical quarter coins
   ─────────────────────────────────
   Total Value:        10 ERG
   Silver Content:     0.7230 troy oz

📊 Note Status:
   Outstanding:      20 ERG
   After Redemption: 10 ERG

✅ Redemption request created!

📍 Next Steps:
   1. Present this redemption proof to vendor
   2. Vendor verifies note maturity (>1 week old)
   3. Vendor hands over physical silver coins:
      • 5 constitutional silver dimes (1946-1964)
      • 2 constitutional silver quarters (1946-1964)
   4. Transaction complete - you now hold physical silver!
```

### 4. Check Balance

```bash
basis-cli silvercents balance --detailed
```

**Example Output:**
```
╔════════════════════════════════════════════════════════════════╗
║                   SILVERCENTS BALANCE                          ║
╚════════════════════════════════════════════════════════════════╝

Account: 03f1e2d3c4b5a697...

💰 RECEIVABLE (Silver you can redeem):
   Total: 20.0 ERG (1.446 troy oz silver)

   Breakdown by vendor:
      02a7b4c5a3b1d2e3:
         20.0 ERG (10 dimes, 4 quarters)

📤 PAYABLE (Silver you owe):
   Total: 5.0 ERG (0.3615 troy oz silver)

   Breakdown by customer:
      03f1e2d3c4b5a697:
         5.0 ERG (5 dimes, 0 quarters)

📊 NET POSITION:
   +15.0 ERG (you can redeem 1.0845 troy oz)
```

### 5. Run Interactive Demo

```bash
basis-cli silvercents demo --interactive
```

Shows a complete walkthrough of:
1. Alice (vendor) issuing a note to Bob (customer)
2. One-week maturation period
3. Bob redeeming the note for physical silver
4. Benefits and real-world applications

## Usage Scenarios

### Scenario 1: Grocery Store Credit

**Setup:**
- Alice owns "Alice's Grocery Store"
- Alice has 100 physical silver dimes and 40 quarters in her safe
- Alice creates on-chain reserve: 100 ERG

**Transaction Flow:**

1. **Day 1: Bob buys groceries ($25)**
   ```bash
   # Alice runs:
   basis-cli silvercents issue \
     --recipient <bob_pubkey> \
     --dimes 10 \
     --description "Groceries: milk, eggs, bread"
   ```
   - Alice creates offchain note (zero fees)
   - Bob receives 10 ERG worth of SilverCents
   - No blockchain transaction needed

2. **Day 8: Bob redeems for physical silver**
   ```bash
   # Bob runs:
   basis-cli silvercents redeem \
     --issuer <alice_pubkey> \
     --dimes 10
   ```
   - Bob presents redemption proof to Alice
   - Alice verifies note is >7 days old
   - Alice hands Bob 10 physical silver dimes
   - On-chain redemption recorded

### Scenario 2: Community Currency

**Setup:**
- Small town with 10 participating vendors
- Each vendor holds physical silver inventory
- Customers can earn/spend SilverCents at any vendor

**Benefits:**
- Local economic stimulus
- No external payment processors
- Works with limited internet access
- Backed by tangible assets
- Privacy-preserving (offchain transactions)

### Scenario 3: Payroll System

**Setup:**
- Small business pays employees in SilverCents
- Employees redeem at local participating vendors

**Workflow:**
```bash
# Employer issues payroll:
basis-cli silvercents issue \
  --recipient <employee_pubkey> \
  --quarters 400 \
  --description "Weekly payroll"

# Employee spends at grocery store:
# (Note transfers offchain - details not shown in this demo)

# Eventually redeems for silver:
basis-cli silvercents redeem \
  --issuer <grocery_pubkey> \
  --quarters 100
```

## Technical Details

### Silver Content Calculations

**Constitutional Silver Coins (1946-1964):**
- **Composition**: 90% silver, 10% copper
- **Dime**: 2.5g total weight → 2.25g silver → 0.0723 troy oz
- **Quarter**: 6.25g total weight → 5.625g silver → 0.1808 troy oz

**Current Market Value** (as of 2024):
- Silver spot price: ~$35/oz
- Dime melt value: $2.53
- Quarter melt value: $6.33

### Denomination Mapping

| Physical Coin | SilverCents | ERG Value | Silver Content |
|---------------|-------------|-----------|----------------|
| 1 Dime        | SC-D        | 1.00 ERG  | 0.0723 troy oz |
| 1 Quarter     | SC-Q        | 2.50 ERG  | 0.1808 troy oz |

### Security Model

1. **Offchain Note Creation**:
   - Vendor signs note with private key (Secp256k1)
   - Basis Tracker verifies signature
   - Note stored in tracker's ledger

2. **Maturation Period**:
   - Notes redeemable after 1 week (604,800 seconds)
   - Prevents immediate redemption pressure
   - Allows notes to circulate locally

3. **On-Chain Redemption**:
   - Proof submitted to Ergo blockchain
   - Reserve contract validates:
     - Schnorr signature from issuer
     - Note age >7 days
     - Sufficient reserve backing
     - No double-spending (AVL tree check)

4. **Double-Spend Prevention**:
   - AVL tree stores hash(issuer || recipient) → timestamp
   - Each note can only be redeemed once
   - On-chain contract enforces uniqueness

## Installation & Setup

### Prerequisites

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone basis-tracker repository
git clone https://github.com/BetterMoneyLabs/basis-tracker.git
cd basis-tracker

# Build the project
cargo build --release
```

### Running the Demo

1. **Start Basis Tracker Server**:
   ```bash
   ./run_server.sh
   ```

2. **Create Vendor Account (Alice)**:
   ```bash
   cargo run --bin basis-cli -- account create --name alice
   cargo run --bin basis-cli -- account select --name alice
   ```

3. **Create Customer Account (Bob)**:
   ```bash
   cargo run --bin basis-cli -- account create --name bob
   ```

4. **Run Interactive Demo**:
   ```bash
   cargo run --bin basis-cli -- silvercents demo --interactive
   ```

5. **Issue Note (as Alice)**:
   ```bash
   cargo run --bin basis-cli -- silvercents issue \
     --recipient $(cargo run --bin basis-cli -- account list | grep bob | awk '{print $2}') \
     --dimes 10 \
     --description "Test transaction"
   ```

6. **Check Balance**:
   ```bash
   cargo run --bin basis-cli -- silvercents balance --detailed
   ```

7. **Redeem Note (as Bob)**:
   ```bash
   cargo run --bin basis-cli -- account select --name bob
   cargo run --bin basis-cli -- silvercents redeem \
     --issuer $(cargo run --bin basis-cli -- account list | grep alice | awk '{print $2}') \
     --dimes 5
   ```

## Testing

Run the test suite:

```bash
# Run all tests
cargo test

# Run SilverCents-specific tests
cargo test silvercents

# Run demo in test mode
cargo run --bin basis-cli -- silvercents demo
```

## Future Enhancements

1. **Mobile App**: NFC-based redemption at physical stores
2. **QR Codes**: Print SilverCents notes as QR codes
3. **Offline Mode**: Mesh network support for internet-limited areas
4. **Multi-Vendor**: Notes transferable between multiple vendors
5. **Fractional Denominations**: Support for smaller silver amounts
6. **Automated Market Maker**: Dynamic ERG/silver exchange rates
7. **Audit Trail**: Public transparency of total circulation

## References

- **Basis Protocol**: [basis.es contract](../contract/basis.es)
- **Ergo Platform**: https://ergoplatform.org
- **Constitutional Silver**: US coins 1946-1964 (90% silver content)
- **Schnorr Signatures**: Basis cryptographic primitive
- **AVL Trees**: Authenticated data structures for fraud prevention

## License

MIT License - See LICENSE file for details

## Support

- GitHub Issues: https://github.com/BetterMoneyLabs/basis-tracker/issues
- Documentation: https://basis-tracker.readthedocs.io
- Community: https://t.me/basis_cash

---

**Disclaimer**: This is a proof-of-concept demonstration. Physical silver redemptions require real-world vendor participation. Always verify note authenticity and vendor trustworthiness before transacting.
