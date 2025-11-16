# Alice → Bob Debt & Redemption Workflow

## Overview
Complete step-by-step instructions for Alice issuing debt to Bob, creating on-chain reserves, and Bob redeeming the debt.

## Prerequisites
- Basis Tracker server running on `http://127.0.0.1:3048`
- Basis CLI client built and available
- Ergo node accessible for blockchain operations

## Step 1: Setup Accounts

### 1.1 Create Alice's Account
```bash
basis-cli account create alice
```
**Expected Output:**
```
✅ Created account 'alice'
  Public Key: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
  Created at: 1759593034
```

**Save Alice's Public Key:** `ALICE_PUBKEY=03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f`

### 1.2 Create Bob's Account
```bash
basis-cli account create bob
```
**Expected Output:**
```
✅ Created account 'bob'
  Public Key: 02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff
  Created at: 1759593177
```

**Save Bob's Public Key:** `BOB_PUBKEY=02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff`

### 1.3 Verify Accounts
```bash
basis-cli account list
```
**Expected Output:**
```
Accounts:
  alice: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f (current)
  bob: 02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff
```

## Step 2: Alice Issues Debt to Bob

### 2.1 Switch to Alice's Account
```bash
basis-cli account switch alice
```

### 2.2 Alice Creates First Debt Note to Bob (1000 nanoERG)
```bash
basis-cli note create --recipient $BOB_PUBKEY --amount 1000
```
**Expected Output:**
```
✅ Note created successfully
```

### 2.3 Alice Creates Second Debt Note to Bob (1500 nanoERG)
```bash
basis-cli note create --recipient $BOB_PUBKEY --amount 1500
```

### 2.4 Alice Creates Third Debt Note to Bob (2000 nanoERG)
```bash
basis-cli note create --recipient $BOB_PUBKEY --amount 2000
```

### 2.5 Verify Alice's Issued Notes
```bash
basis-cli note list --issuer
```
**Expected Output:**
```
Notes where you are the issuer:
  To: 02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff
    Amount: 1000 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 1000 nanoERG
    Created: 1759593288
  To: 02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff
    Amount: 1500 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 1500 nanoERG
    Created: 1759593290
  To: 02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff
    Amount: 2000 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 2000 nanoERG
    Created: 1759593292
```

**Total Debt Issued:** 4500 nanoERG (4.5 ERG)

## Step 3: Bob Views Received Debt

### 3.1 Switch to Bob's Account
```bash
basis-cli account switch bob
```

### 3.2 Bob Views Notes Received from Alice
```bash
basis-cli note list --recipient
```
**Expected Output:**
```
Notes where you are the recipient:
  From: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
    Amount: 1000 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 1000 nanoERG
    Created: 1759593288
  From: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
    Amount: 1500 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 1500 nanoERG
    Created: 1759593290
  From: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
    Amount: 2000 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 2000 nanoERG
    Created: 1759593292
```

### 3.3 Bob Checks Specific Note
```bash
basis-cli note get --issuer $ALICE_PUBKEY --recipient $BOB_PUBKEY
```

## Step 4: Check Reserve Status (Before Reserve Creation)

### 4.1 Check Alice's Reserve Status
```bash
basis-cli reserve status --issuer $ALICE_PUBKEY
```
**Expected Output (Before Reserve):**
```
Reserve Status for 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f:
  Total Debt: 4500 nanoERG
  Collateral: 0 nanoERG
  Collateralization Ratio: 0.00
  Note Count: 3
  Last Updated: 1759593292

In ERG:
  Total Debt: 0.0000045 ERG
  Collateral: 0.0000000 ERG
```

**⚠️ WARNING:** Alice is **UNDER-COLLATERALIZED** (0% collateralization)

### 4.2 Check Collateralization Status
```bash
basis-cli reserve collateralization --issuer $ALICE_PUBKEY
```
**Expected Output:**
```
Collateralization for 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f:
  Ratio: 0.0000
  Status: UNDER-COLLATERALIZED
⚠️  WARNING: Under-collateralized!
```

## Step 5: Alice Creates On-Chain Reserve

### 5.1 Deploy Basis Reserve Contract
*(This step requires Ergo blockchain interaction - actual deployment steps depend on your setup)*

**Prerequisites:**
- Configure `tracker_nft_id` in `config/basis.toml`
- This NFT identifies your tracker server and must be set in the reserve contract's R6 register

**Example using Ergo AppKit or similar:**
```scala
// Deploy Basis reserve contract with 10 ERG collateral
val reserveContract = BasisReserveContract.deploy(
  issuerPubKey = ALICE_PUBKEY,
  collateralAmount = 10000000000L, // 10 ERG in nanoERG
  trackerNftId = TRACKER_NFT_ID,   // From config/basis.toml
  minCollateralRatio = 1.5
)
```

**Reserve Box Structure:**
- **Value**: Collateral amount (10 ERG)
- **Tokens**: Reserve singleton token
- **R4**: Issuer's public key
- **R5**: Empty AVL tree for redeemed timestamps  
- **R6**: Tracker NFT ID (from configuration)

**Expected On-Chain Events:**
- Reserve box created with 10 ERG collateral
- Reserve creation event emitted
- Tracker server detects and processes the event

### 5.2 Verify Reserve Creation
```bash
basis-cli status
```
**Expected Output:**
```
✅ Server is healthy

Recent Events (last 1):
  [1759593300] Reserve created: box1234567890abcdef (10000000000 nanoERG) - height 1500
```

### 5.3 Check Updated Reserve Status
```bash
basis-cli reserve status --issuer $ALICE_PUBKEY
```
**Expected Output (After Reserve):**
```
Reserve Status for 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f:
  Total Debt: 4500 nanoERG
  Collateral: 10000000000 nanoERG
  Collateralization Ratio: 2222222.22
  Note Count: 3
  Last Updated: 1759593300

In ERG:
  Total Debt: 0.0000045 ERG
  Collateral: 10.0000000 ERG
```

### 5.4 Check Improved Collateralization
```bash
basis-cli reserve collateralization --issuer $ALICE_PUBKEY
```
**Expected Output:**
```
Collateralization for 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f:
  Ratio: 2222222.2222
  Status: EXCELLENT
```

## Step 6: Bob Redeems Debt

### 6.1 Switch to Bob's Account
```bash
basis-cli account switch bob
```

### 6.2 Bob Redeems First Note (500 nanoERG)
```bash
basis-cli note redeem --issuer $ALICE_PUBKEY --amount 500
```
**Expected Output:**
```
✅ Redemption initiated
  Redemption ID: redemption_123456
  Amount: 500 nanoERG
  Proof available: true
✅ Redemption completed
```

### 6.3 Bob Redeems Second Note (1000 nanoERG)
```bash
basis-cli note redeem --issuer $ALICE_PUBKEY --amount 1000
```

### 6.4 Verify Bob's Updated Notes
```bash
basis-cli note list --recipient
```
**Expected Output:**
```
Notes where you are the recipient:
  From: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
    Amount: 1000 nanoERG
    Redeemed: 500 nanoERG
    Outstanding: 500 nanoERG
    Created: 1759593288
  From: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
    Amount: 1500 nanoERG
    Redeemed: 1000 nanoERG
    Outstanding: 500 nanoERG
    Created: 1759593290
  From: 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f
    Amount: 2000 nanoERG
    Redeemed: 0 nanoERG
    Outstanding: 2000 nanoERG
    Created: 1759593292
```

**Total Redeemed:** 1500 nanoERG
**Remaining Debt:** 3000 nanoERG

## Step 7: Final Status Check

### 7.1 Check Alice's Final Reserve Status
```bash
basis-cli reserve status --issuer $ALICE_PUBKEY
```
**Expected Final Output:**
```
Reserve Status for 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f:
  Total Debt: 3000 nanoERG
  Collateral: 10000000000 nanoERG
  Collateralization Ratio: 3333333.33
  Note Count: 3
  Last Updated: 1759593350

In ERG:
  Total Debt: 0.0000030 ERG
  Collateral: 10.0000000 ERG
```

### 7.2 Check All Events
```bash
basis-cli status
```
**Expected Events Summary:**
```
✅ Server is healthy

Recent Events (last 6):
  [1759593350] Redemption: Alice -> Bob (500 nanoERG)
  [1759593345] Redemption: Alice -> Bob (1000 nanoERG)
  [1759593300] Reserve created: box1234567890abcdef (10000000000 nanoERG) - height 1500
  [1759593292] Note: Alice -> Bob (2000 nanoERG)
  [1759593290] Note: Alice -> Bob (1500 nanoERG)
  [1759593288] Note: Alice -> Bob (1000 nanoERG)
```

## Step 8: Generate Proof (Optional)

### 8.1 Generate Proof for Specific Note
```bash
basis-cli proof --issuer $ALICE_PUBKEY --recipient $BOB_PUBKEY
```
**Expected Output:**
```
Proof generated for 03f576b9aa524ed4b1eca8478489937fe84aa7314c0c73c4b73cef0a0c6c86240f -> 02e58b5f80040bbd8ef6a3416a36da1fea1eea9a3922ea2417aa24942e0814bcff
  Proof Data: proof_03f576b9aa524e_02e58b5f80040bbd
  Tracker State Digest: mock_digest_1234567890abcdef
  Block Height: 1500
  Timestamp: 1759593350
```

## Summary

### ✅ Workflow Completed Successfully:

1. **Account Setup** - Alice and Bob accounts created
2. **Debt Issuance** - Alice issued 4500 nanoERG debt to Bob across 3 notes
3. **Reserve Creation** - Alice deployed 10 ERG collateral on-chain
4. **Redemption** - Bob redeemed 1500 nanoERG of debt
5. **Verification** - All operations verified through CLI commands

### 📊 Final State:
- **Alice's Debt**: 3000 nanoERG outstanding
- **Alice's Collateral**: 10 ERG (excellent collateralization)
- **Bob's Holdings**: 1500 nanoERG redeemed, 3000 nanoERG remaining
- **Blockchain**: Reserve contract deployed and operational

### 🔄 Next Steps:
- Bob can redeem remaining debt
- Alice can issue more debt to other parties
- Monitor collateralization ratios
- Generate proofs for audit purposes