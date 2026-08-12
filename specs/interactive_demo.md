# Basis Protocol Interactive Tutorial - Alice to Bob Payment & Redemption

A hands-on tutorial demonstrating the complete Basis protocol flow: reserve deployment, IOU note issuance (Alice → Bob), and on-chain redemption with a real tracker.

## Overview

This tutorial walks through the complete Basis protocol using real keys from `secrets/participants.csv` and a live tracker server connected to the Ergo blockchain.

**Prerequisites:**
- Ergo node access (public node or local)
- Tracker server running
- `basis_cli` compiled (see below)
- Alice has ERG for reserve collateral and fees
- Bob has an Ergo wallet for receiving payments

**Building the CLI:**

```bash
# Build debug version
cargo build -p basis_cli

# Or build release version (recommended for production)
cargo build --release -p basis_cli
```

**Using the CLI:**

The binary is located at `target/debug/basis_cli` (or `target/release/basis_cli`).
Run it from the project root directory:

```bash
# From project root
./target/debug/basis_cli --help

# Or add to your PATH
export PATH="$PATH:$PWD/target/debug"
./basis_cli --help
```

**Key Participants:**
| Role | Name | Address | Secret Key |
|------|------|---------|------------|
| Issuer | Alice | `9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ` | From `participants.csv` |
| Recipient | Bob | `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73` | N/A (receives only) |
| Tracker | tracker | `9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe` | From `participants.csv` |

> **Production tracker update:** the current deployment in `config/basis.toml` has switched the tracker to a wallet-owned address at `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73` with tracker NFT `b159ad5c9062ec4c3f83cc478f1580f8312fd13439868551cc5905bc3c0ef42f`. The table above reflects the historical demo `participants.csv` layout; update addresses and the tracker NFT in your own `config/basis.toml` to match your deployment.

**Bob does NOT need a secret key** for this tutorial because redemption generates an unsigned transaction that Bob's Ergo wallet signs.

---

## Quick Start - All Commands

Copy and run these commands in order:

```bash
# 1. Check environment
curl http://localhost:3048/health
curl http://localhost:9053/info | jq '.name'

# 2. Deploy reserve (Alice)
# Note: You need a Reserve NFT (not the tracker NFT).
# Create one via Ergo node or use an existing token ID.
./target/debug/basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 100000000 \
  --nft-id <YOUR_RESERVE_NFT_ID>
# Submit returned payload to Ergo node, wait for confirmation

# 3. Create IOU note (Alice → Bob)
./target/debug/basis_cli note create --demo --amount 50000000 --output alice_to_bob_note.json

# 4. Verify note
./target/debug/basis_cli note get \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea

# 5. Generate redemption transaction (Bob)
./target/debug/basis_cli transaction generate-redemption \
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
# Broadcast the signed transaction to the network
curl -X POST http://localhost:9053/transactions \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @signed_tx.json

# 8. Verify
./target/debug/basis_cli reserve status \
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
curl http://localhost:9053/info | jq '.name'
```

### 1.3 Verify Alice's Keys

```bash
# Show Alice's public key from participants.csv
grep "^alice," secrets/participants.csv
```

**Alice's public key:** `0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83`

---

## Step 2: Deploy Reserve (Alice)

Alice must create an on-chain reserve with collateral before issuing IOU notes.

### 2.1 Create Reserve

```bash
# Deploy reserve with 0.1 ERG (100M nanoERG) collateral
# IMPORTANT: --nft-id is the RESERVE NFT (not tracker NFT)
# You must create or obtain a reserve NFT first (see below)
./target/debug/basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 100000000 \
  --nft-id <YOUR_RESERVE_NFT_ID>
```

**Creating a Reserve NFT:**

You need an NFT to identify your reserve. Create one using the Ergo node:

```bash
# Create a new NFT (replace with your address)
curl -X POST http://localhost:9053/wallet/transaction/send \
  -H "api_key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "requests": [{
      "address": "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ",
      "value": 1000000,
      "assets": [{
        "tokenId": "<new-token-id>",
        "amount": 1
      }]
    }]
  }'
```

**Note:** The tracker NFT ID is configured on the server and goes into R6 register automatically. You only need to provide the reserve NFT ID here.

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
      { tokenId: "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b", amount: 1 },
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
./target/debug/basis_cli reserve status \
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
./target/debug/basis_cli note create \
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
./target/debug/basis_cli note get \
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
./target/debug/basis_cli reserve collateralization \
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

### 5.0 Ensure the Tracker Box Is Up to Date

Before redemption can succeed, the tracker server must have committed the note to its on-chain tracker box. The server's background updater submits a tracker box update transaction every 10 minutes whenever new notes are added. Verify the tracker box has the current state:

```bash
# Check the latest tracker box ID
curl http://localhost:3048/tracker/latest-box-id

# Confirm the note is in the tracker's AVL tree
curl "http://localhost:3048/notes?issuer=0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83&recipient=03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea"
```

If the tracker box has not been updated yet, wait up to 10 minutes or check the server logs for the updater confirmation. The contract verifies the note's total debt against the tracker box R5 AVL tree root, so redemption will fail if the tracker box is stale.

### 5.1 Generate Unsigned Transaction

Bob generates an unsigned redemption transaction using the CLI:

```bash
./target/debug/basis_cli transaction generate-redemption \
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
4. Fetches AVL proofs (tracker lookup proof, reserve insert/update proof)
5. Requests tracker signature from server
6. Fetches Bob's private key from the Ergo node wallet (`/wallet/getPrivateKey`) to include in `secrets.dlog`
7. Selects wallet-owned fee inputs from the node
8. Builds an unsigned transaction in the format expected by `/wallet/transaction/sign`, including `inputsRaw`, `dataInputsRaw`, and `secrets.dlog`

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
  "tx": {
    "inputs": [
      {
        "boxId": "reserve_box_id",
        "extension": {
          "0": "0200",
          "1": "0703receiver_pubkey_hex...",
          "2": "0e4102reserve_owner_sig_hex...",
          "3": "05long_to_vlq(totalDebt)",
          "5": "0e...insert_or_update_proof_hex...",
          "6": "0e4102tracker_sig_hex...",
          "8": "0e...tracker_lookup_proof_hex..."
        }
      },
      {
        "boxId": "fee_input_box_id",
        "extension": {}
      }
    ],
    "dataInputs": [
      { "boxId": "tracker_box_id" }
    ],
    "outputs": [
      {
        "value": 25000000,
        "ergoTree": "recipient_p2pk_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [],
        "additionalRegisters": {}
      },
      {
        "value": 74000000,
        "ergoTree": "basis_reserve_contract_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [{"tokenId": "...", "amount": 1}],
        "additionalRegisters": {
          "R4": "07issuer_pubkey_hex...",
          "R5": "hex_encoded_updated_avl_tree_root_digest",
          "R6": "0e20tracker_nft_id_hex"
        }
      },
      {
        "value": 1000000,
        "ergoTree": "standard_fee_contract_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [],
        "additionalRegisters": {}
      },
      {
        "value": 490000000,
        "ergoTree": "change_p2pk_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [],
        "additionalRegisters": {}
      }
    ]
  },
  "inputsRaw": ["hex_encoded_serialized_reserve_box_bytes", "hex_encoded_serialized_fee_input_bytes"],
  "dataInputsRaw": ["hex_encoded_serialized_tracker_box_bytes"],
  "secrets": {
    "dlog": ["recipient_private_key_hex"]
  }
}
```

---

## Step 6: Sign and Broadcast (Bob)

### 6.1 Sign with Ergo Node Wallet

Bob submits the unsigned redemption transaction to his Ergo node wallet:

```bash
# Sign the transaction (the node uses secrets.dlog to satisfy proveDlog(receiver))
curl -X POST http://localhost:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @redemption_tx.json > signed_tx.json
```

**Note:**
- Bob's wallet must be unlocked.
- The transaction JSON already contains `secrets.dlog` with Bob's private key, which the CLI fetched from the node wallet (`/wallet/getPrivateKey`) so the node can satisfy the `proveDlog(receiver)` condition.
- The transaction uses `inputsRaw`/`dataInputsRaw` to reference the reserve and tracker boxes by serialized bytes.

### 6.2 Broadcast Signed Transaction

```bash
# Broadcast the signed transaction
curl -X POST http://localhost:9053/transactions \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @signed_tx.json
```

### 6.3 Alternative: Sign via CLI

If Bob has configured his Ergo node in the CLI, the CLI command is still under development. Currently the CLI only generates the unsigned transaction.

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
./target/debug/basis_cli reserve status \
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
./target/debug/basis_cli note get \
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

## Automation Note

The `demo/run_full_tutorial.sh` automation script has been removed; the demo
directory now contains only the pure-credit `agent_coop` and `lets_tutorial`
demos. The command-by-command instructions above remain accurate for anyone who
wants to run the reserve/note/redemption flow manually.

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
./target/debug/basis_cli note create --demo --amount 50000000
```

### "Insufficient collateral"

**Cause:** Reserve collateral is less than redemption amount.

**Solution:**
```bash
# Top up reserve
./target/debug/basis_cli reserve topup \
  --amount 50000000 \
  --issuer 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
```

### "Script reduced to false"

**Cause:** Contract validation failed - usually signature, proof, or tracker box issue.

**Common fixes:**
1. Ensure the tracker box has been updated to include the note's debt; if not, wait for the tracker updater to commit the new AVL root
2. Ensure `secrets.dlog` contains the recipient's private key and the recipient address is in the node wallet
3. Ensure tracker signature is fresh (not expired)
4. Check AVL proofs are valid for current tree state
5. Verify reserve owner's signature uses correct message format
6. Confirm redemption amount ≤ (totalDebt - alreadyRedeemed)
7. Verify `inputsRaw` and `dataInputsRaw` match the reserve and tracker boxes referenced by ID

### "Tracker box not found"

**Cause:** Tracker hasn't created its commitment box on-chain.

**Solution:**
```bash
# Check tracker box updater status
curl http://localhost:3048/tracker/latest-box-id

# If empty, wait for tracker to create initial box
```

### Context Extension Format Issues

Context extensions are attached to the **reserve input** (the first input in `tx.inputs`), not at the top level. If you see errors about context extension variables:

- **#0 (action)**: Must be `0200` (Byte constant, value 0)
- **#1 (receiver)**: Must be `07` + 33-byte pubkey hex (GroupElement)
- **#2 (reserveSig)**: Must be `0e` + 2-byte length + 65-byte signature (Coll[Byte])
- **#3 (totalDebt)**: Must be `05` + 8-byte big-endian Long
- **#5 (insertOrUpdateProof)**: AVL proof for reserve tree insert/update (Coll[Byte])
- **#6 (trackerSig)**: Tracker's 65-byte Schnorr signature (Coll[Byte])
- **#8 (lookupProof)**: AVL proof for tracker tree lookup (Coll[Byte])

If the node returns a signing error such as "Script reduced to false" or "Cannot proveDlog", verify:
1. `secrets.dlog` contains the recipient's private key hex
2. `inputsRaw` and `dataInputsRaw` contain the correct serialized box bytes
3. The reserve output is at index 0 in `tx.outputs` (the contract action byte resolves to index 0)

---

## Advanced Topics

### Emergency Redemption

If tracker becomes unavailable, emergency redemption is possible after 3 days:

```bash
./target/debug/basis_cli transaction generate-redemption \
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
./target/debug/basis_cli transaction generate-redemption --amount 25000000 ...

# Second redemption: remaining 25M nanoERG
./target/debug/basis_cli transaction generate-redemption --amount 25000000 ...
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
