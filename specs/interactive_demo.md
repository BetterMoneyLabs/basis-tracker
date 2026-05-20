# Basis Protocol Interactive Tutorial - Alice to Bob Payment & Redemption

A hands-on tutorial demonstrating the complete Basis protocol flow: reserve deployment, IOU note issuance (Alice → Bob), and on-chain redemption with a real tracker.

## Overview

This tutorial walks through the complete Basis protocol using real keys from `secrets/participants.csv` and a live tracker server connected to the Ergo blockchain.

**Prerequisites:**
- Ergo node access (public node or local)
- Tracker server running
- `basis_cli` compiled
- Alice has ERG for reserve collateral and fees
- Bob has an Ergo wallet for receiving payments

**Key Participants:**
| Role | Name | Address | Secret Key |
|------|------|---------|------------|
| Issuer | Alice | `9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ` | From `participants.csv` |
| Recipient | Bob | `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73` | N/A (receives only) |
| Tracker | tracker | `9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe` | From `participants.csv` |

**Bob does NOT need a secret key** for this tutorial because redemption generates an unsigned transaction that Bob's Ergo wallet signs.

---

## Quick Start - All Commands

Copy and run these commands in order:

```bash
# 1. Check environment
curl http://localhost:3048/health
curl http://159.89.116.15:11088/info | jq '.name'

# 2. Deploy reserve (Alice)
basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 100000000 \
  --nft-id <TRACKER_NFT_ID>
# Submit returned payload to Ergo node, wait for confirmation

# 3. Create IOU note (Alice → Bob)
basis_cli note create --demo --amount 50000000 --output alice_to_bob_note.json

# 4. Verify note
basis_cli note get \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea

# 5. Generate redemption transaction (Bob)
basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 25000000 \
  --output-file redemption_tx.json

# 6. Sign transaction (Bob's Ergo wallet)
curl -X POST http://localhost:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @redemption_tx.json > signed_tx.json

# 7. Broadcast
curl -X POST http://localhost:9053/wallet/transaction/send \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @signed_tx.json

# 8. Verify
basis_cli reserve status \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
```

---

## Step 1: Verify Environment

### 1.1 Check Tracker Server

```bash
# Server should be running on localhost:3048
curl http://localhost:3048/health
```

Expected response:
```json
{"status":"ok","tracker_connected":true}
```

### 1.2 Check Ergo Node

```bash
# Check node connectivity (using public testnet node)
curl http://159.89.116.15:11088/info | jq '.name'
```

### 1.3 Verify Alice's Keys

```bash
# Show Alice's public key (from participants.csv)
basis_cli key info --name alice
```

**Alice's public key:** `0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83`

---

## Step 2: Deploy Reserve (Alice)

Alice must create an on-chain reserve with collateral before issuing IOU notes.

### 2.1 Create Reserve

```bash
# Deploy reserve with 0.1 ERG (100M nanoERG) collateral
# Using tracker NFT from configuration
basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 100000000 \
  --nft-id <TRACKER_NFT_ID>
```

**Note:** The tracker NFT ID is configured on the server. Check server logs or ask the tracker operator.

**Example output:**
```
Creating reserve with:
  NFT ID: 69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b
  Owner: 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
  Amount: 100000000 nanoERG

✅ Reserve creation payload created successfully!

Requests:
  Request 1: {
    address: "2iHkR7CWvD1R4j1yZg5bkeDRQavjAaVPeTDFGGLZduHyfWMuYpmhHocX8GJoaieTx78FntzJbCBVL6rf96ocJoZdmWBL2fci7NqWgAirppPQmZ7fN9V6z13Ay6brPriBKYqLp1bT2Fk4FkFLCfdPpe"
    value: 100000000
    assets: [
      { token_id: "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b", amount: 1 },
    ]
    registers: {
      "R4": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
      "R5": "...",
      "R6": "0e2069c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b",
    }
  }

Fee: 1000000 nanoERG
Change address: 9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ
```

### 2.2 Submit Reserve Transaction

**Option A: Via Ergo Wallet API**
```bash
# Submit the generated payload to your Ergo node
curl -X POST http://localhost:9053/wallet/transaction/send \
  -H "Content-Type: application/json" \
  -H "api_key: your-api-key" \
  -d @reserve_payload.json
```

**Option B: Via Ergo Node UI**
- Open Ergo node UI
- Navigate to Wallet → Send
- Paste the generated request JSON

### 2.3 Wait for Confirmation

```bash
# Check reserve status
basis_cli reserve status \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
```

Expected:
```
Reserve Status for 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83:
  Total Debt: 0 nanoERG
  Collateral: 100000000 nanoERG
  Collateralization Ratio: inf
  Note Count: 0
  Last Updated: 1234567890
```

---

## Step 3: Create IOU Note (Alice → Bob)

### 3.1 Create Payment Note

Alice creates an IOU note for Bob using demo mode (uses keys from `participants.csv`):

```bash
# Create IOU note for 0.05 ERG (50M nanoERG)
basis_cli note create \
  --demo \
  --amount 50000000 \
  --output alice_to_bob_note.json
```

**Output file:** `alice_to_bob_note.json`
```json
{
  "payerKey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
  "payeeKey": "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
  "totalDebt": 50000000,
  "totalDebtERG": 0.05,
  "timestamp": 1775924356220,
  "payerSignature": {
    "a": "...",
    "z": "..."
  },
  "trackerSignature": {
    "a": "...",
    "z": "..."
  },
  "message": "...",
  "noteKey": "..."
}
```

### 3.2 Submit Note to Tracker

```bash
# The note is automatically sent to the tracker server during creation
# Verify it was accepted:
basis_cli note get \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea
```

---

## Step 4: Verify Note State

### 4.1 Check Tracker State

```bash
# Query tracker for the note
curl "http://localhost:3048/notes?issuer=0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83&recipient=03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea"
```

### 4.2 Verify Reserve Collateralization

```bash
basis_cli reserve collateralization \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
```

Expected:
```
Collateralization for 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83:
  Ratio: 2.0000
  Status: GOOD
```

### 4.3 Get Tracker Proof

```bash
# Get AVL proof for redemption preparation
curl "http://localhost:3048/proof/redemption?issuer_pubkey=0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83&recipient_pubkey=03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea"
```

---

## Step 5: Generate Redemption Transaction (Bob)

### 5.1 Generate Unsigned Transaction

Bob generates an unsigned redemption transaction using the CLI:

```bash
basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 25000000 \
  --output-file redemption_tx.json
```

**Parameters:**
- `--issuer-pubkey`: Alice's public key (reserve owner)
- `--recipient-pubkey`: Bob's public key (payment recipient)
- `--amount`: Amount to redeem in nanoERG (must be ≤ outstanding debt)
- `--output-file`: Where to save the unsigned transaction JSON

**What happens internally:**
1. CLI queries tracker server for note details and outstanding debt
2. Retrieves Alice's reserve box from the tracker
3. Gets latest tracker box ID
4. Fetches AVL proofs (tracker lookup proof, reserve insert proof)
5. Requests tracker signature from server
6. Builds unsigned transaction with proper context extension variables

**Example output:**
```
🔍 Retrieving note information...
🔍 Retrieving issuer's reserve box...
🔍 Retrieving latest tracker box...
🔗 Converting public keys to addresses...
🔍 Retrieving tracker lookup proof from server...
🔍 Retrieving reserve proofs from server...
🔑 Signing redemption with issuer key...
📝 Generating unsigned transaction...

✅ Transaction JSON written to: redemption_tx.json

📋 Transaction details:
   Issuer: 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
   Recipient: 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea
   Redemption amount: 25000000 nanoERG
   Total debt: 50000000 nanoERG
   Already redeemed: 0 nanoERG
   Reserve box ID: abcdef...
   Tracker box ID: fedcba...
   Transaction fee: 1000000 nanoERG
   Emergency redemption: false
   First redemption: true
```

### 5.2 Inspect Generated Transaction

```bash
# View the generated transaction
cat redemption_tx.json | jq .
```

**Key fields:**
```json
{
  "requests": [
    {
      "address": "9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73",
      "value": 25000000,
      "assets": [],
      "registers": {}
    },
    {
      "address": "2iHkR7CWvD1R4j1yZg5bkeDRQavjAaVPeTDFGGLZduHyfWMuYpmhHocX8GJoaieTx78FntzJbCBVL6rf96ocJoZdmWBL2fci7NqWgAirppPQmZ7fN9V6z13Ay6brPriBKYqLp1bT2Fk4FkFLCfdPpe",
      "value": 74000000,
      "assets": [{"tokenId": "...", "amount": 1}],
      "registers": {
        "R4": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
        "R5": "...",
        "R6": "..."
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": ["..."],
  "dataInputsRaw": ["..."],
  "contextExtension": {
    "0": "0200",
    "1": "07...",
    "2": "0e...",
    "3": "05...",
    "4": "05...",
    "5": "0e...",
    "6": "0e...",
    "8": "0e..."
  }
}
```

---

## Step 6: Sign and Broadcast (Bob)

### 6.1 Sign with Ergo Wallet

Bob submits the unsigned transaction to his Ergo node wallet:

```bash
# Sign the transaction
curl -X POST http://localhost:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @redemption_tx.json
```

**Note:** Bob's wallet must:
- Be unlocked
- Hold the fee boxes referenced in the transaction (if any)
- For this tutorial, the transaction uses inputsRaw/dataInputsRaw which reference boxes by ID

### 6.2 Broadcast Signed Transaction

```bash
# Broadcast the signed transaction
curl -X POST http://localhost:9053/wallet/transaction/send \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @signed_tx.json
```

### 6.3 Alternative: Sign via CLI

If Bob has configured his Ergo node in the CLI:

```bash
basis_cli transaction sign \
  --input redemption_tx.json \
  --node-url http://localhost:9053 \
  --api-key bob-api-key
```

---

## Step 7: Verify Redemption

### 7.1 Check Bob's Balance

```bash
# Check Bob's wallet balance
curl http://localhost:9053/wallet/balances \
  -H "api_key: bob-api-key" | jq '.balance'
```

### 7.2 Check Reserve Status

```bash
basis_cli reserve status \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
```

Expected after redemption:
```
Reserve Status for 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83:
  Total Debt: 50000000 nanoERG
  Collateral: 74000000 nanoERG
  Collateralization Ratio: 1.4800
  Note Count: 1
  Last Updated: 1234567900
```

### 7.3 Verify Note Updated

```bash
basis_cli note get \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea
```

Expected:
```
Note found:
  Issuer: 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
  Recipient: 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea
  Amount: 50000000 nanoERG
  Redeemed: 25000000 nanoERG
  Outstanding: 25000000 nanoERG
```

---

## Complete Workflow Summary

```
Alice (Issuer)                    Tracker Server                    Bob (Recipient)
     │                                   │                                   │
     │ 1. Deploy Reserve (0.1 ERG)       │                                   │
     │ ─────────────────────────────────>│                                   │
     │                                   │                                   │
     │ 2. Create IOU Note (0.05 ERG)     │                                   │
     │ ─────────────────────────────────>│                                   │
     │                                   │                                   │
     │ 3. Send Note to Bob               │                                   │
     │ ──────────────────────────────────────────────────────────────────> │
     │                                   │                                   │
     │                                   │ 4. Bob Generates Unsigned Tx      │
     │                                   │ <─────────────────────────────────│
     │                                   │                                   │
     │                                   │ 5. Bob Signs & Broadcasts Tx      │
     │                                   │ <─────────────────────────────────│
     │                                   │                                   │
     │                                   │ 6. Reserve Updated                │
     │                                   │ ─────────────────────────────────>│
     │                                   │                                   │
     │                                   │ 7. Bob Receives 0.025 ERG         │
     │                                   │ ─────────────────────────────────>│
```

---

## Automation Script

For convenience, use the provided automation script:

```bash
# Run the complete tutorial
./demo/run_full_tutorial.sh

# Or step-by-step:
./demo/run_full_tutorial.sh --step reserve    # Deploy reserve
./demo/run_full_tutorial.sh --step note       # Create note
./demo/run_full_tutorial.sh --step redeem     # Generate redemption tx
```

---

## Troubleshooting

### "No reserve box found"

**Cause:** Reserve hasn't been created or confirmed yet.

**Solution:**
```bash
# Check if reserve transaction is confirmed
curl http://localhost:9053/transactions/pool | grep <tx_id>

# Wait for confirmation and retry
```

### "Note not found"

**Cause:** Note wasn't submitted to tracker or tracker hasn't processed it.

**Solution:**
```bash
# Check tracker server health
curl http://localhost:3048/health

# Re-submit note
basis_cli note create --demo --amount 50000000
```

### "Insufficient collateral"

**Cause:** Reserve collateral is less than redemption amount.

**Solution:**
```bash
# Top up reserve
basis_cli reserve topup \
  --amount 50000000 \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
```

### "Script reduced to false"

**Cause:** Contract validation failed - usually signature or proof issue.

**Common fixes:**
1. Ensure tracker signature is fresh (not expired)
2. Check AVL proofs are valid for current tree state
3. Verify reserve owner's signature uses correct message format
4. Confirm redemption amount ≤ (totalDebt - alreadyRedeemed)

### "Tracker box not found"

**Cause:** Tracker hasn't created its commitment box on-chain.

**Solution:**
```bash
# Check tracker box updater status
curl http://localhost:3048/tracker/latest-box-id

# If empty, wait for tracker to create initial box
```

### Context Extension Format Issues

If you see errors about context extension variables:
- **#0 (action)**: Must be `0200` (Byte constant, value 0)
- **#1 (receiver)**: Must be `07` + 33-byte pubkey hex (GroupElement)
- **#2 (reserveSig)**: Must be `0e` + 2-byte length + 65-byte signature (Coll[Byte])
- **#3 (totalDebt)**: Must be `05` + 8-byte big-endian Long
- **#5 (insertProof)**: AVL proof for reserve tree insert
- **#6 (trackerSig)**: Tracker's 65-byte Schnorr signature
- **#8 (lookupProof)**: AVL proof for tracker tree lookup

---

## Advanced Topics

### Emergency Redemption

If tracker becomes unavailable, emergency redemption is possible after 3 days:

```bash
basis_cli transaction generate-redemption \
  --issuer-pubkey <ALICE_PUBKEY> \
  --recipient-pubkey <BOB_PUBKEY> \
  --amount <AMOUNT> \
  --emergency \
  --output-file emergency_redemption.json
```

**Requirements:**
- 3 days (2160 blocks) must pass since tracker box creation
- Only reserve owner's signature required (no tracker signature)
- Uses last committed tracker state

### Partial Redemption

Bob can redeem partial amounts multiple times:

```bash
# First redemption: 25M nanoERG
basis_cli transaction generate-redemption --amount 25000000 ...

# Second redemption: remaining 25M nanoERG
basis_cli transaction generate-redemption --amount 25000000 ...
```

Each redemption updates the reserve's AVL tree to track cumulative redeemed amounts.

### Debt Transfer (Novation)

Bob can transfer his debt claim to Charlie (with Alice's consent):

```bash
# This feature requires server support for debt transfers
# Contact tracker operator for debt transfer API
```

---

## Security Notes

- **Demo keys** in `participants.csv` are for testing only - never use in production
- **Alice's secret key** signs IOU notes - keep secure
- **Tracker's secret key** signs redemption authorizations - must be protected
- **Bob only needs** his public key for receiving payments
- **Ergo wallet** handles Bob's signing for transaction broadcast
- **Reserve collateral** should be monitored to maintain healthy collateralization ratio

---

## References

- [Protocol Specification](spec.md)
- [Redemption CLI Specification](redemption_cli_spec.md)
- [Tracker Box Setup Guide](../docs/TRACKER_BOX_SETUP.md)
- [Scala Demo](../scala/demo/README.md)
- [Ergo Documentation](https://docs.ergoplatform.com/)
