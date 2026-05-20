# Basis Protocol Demo

This directory contains demonstrations and tutorials for the Basis protocol.

## Available Tutorials

### 1. Full Interactive Tutorial (Recommended)

**File:** `run_full_tutorial.sh`
**Documentation:** [specs/interactive_demo.md](../specs/interactive_demo.md)

A comprehensive hands-on tutorial demonstrating the complete Basis protocol flow:
- **Reserve Deployment** - Create on-chain reserve with collateral
- **IOU Note Issuance** - Alice pays Bob via tracker-signed note
- **Redemption** - Bob generates unsigned transaction and redeems on-chain

**Features:**
- Uses real keys from `secrets/participants.csv`
- Connects to live tracker server
- Generates real unsigned Ergo transactions
- Step-by-step with troubleshooting guide

**Quick Start:**
```bash
# Run the complete tutorial
./demo/run_full_tutorial.sh

# Or step-by-step
./demo/run_full_tutorial.sh --step reserve    # Deploy reserve
./demo/run_full_tutorial.sh --step note       # Create IOU note
./demo/run_full_tutorial.sh --step redeem     # Generate redemption tx
```

### 2. Simple Note Creation Demo

**File:** `run_demo.sh`

A minimal demo for creating IOU notes only (no redemption):
```bash
./demo/run_demo.sh
```

This creates a demo note (Alice → Bob) with hardcoded test keys and saves it to `demo/output/note.json`.

## Configuration

Edit `demo/config.toml` to customize demo parameters:

```toml
[demo]
# Default debt amount: 0.05 ERG (50M nanoERG)
default_debt_amount = 50000000

# Fee configuration
fee_box_value = 250000
fee_box_count = 4

# Reserve initial collateral: 0.1 ERG (100M nanoERG)
reserve_initial_collateral = 100000000
```

## Key Participants

| Role | Address | Description |
|------|---------|-------------|
| **Alice** (Issuer) | `9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ` | Creates reserves and IOU notes |
| **Bob** (Recipient) | `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73` | Receives and redeems notes |
| **Tracker** | `9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe` | Off-chain state tracking |

**Note:** Bob does NOT need a secret key for redemption because the unsigned transaction is signed by Bob's Ergo wallet.

## Prerequisites

### For Full Tutorial

1. **Build the CLI:**
   ```bash
   cargo build -p basis_cli
   ```

2. **Start Tracker Server:**
   ```bash
   cargo run -p basis_server
   ```

3. **Ergo Node Access:**
   - Public testnet: `http://159.89.116.15:11088`
   - Or local node: `http://localhost:9053`

4. **Alice needs ERG** for reserve collateral and transaction fees

### For Simple Demo

Just the CLI:
```bash
cargo build -p basis_cli
./demo/run_demo.sh
```

## Tutorial Steps

### Step 1: Deploy Reserve

Alice creates an on-chain reserve with 0.1 ERG collateral:

```bash
basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 100000000 \
  --nft-id <TRACKER_NFT_ID>
```

### Step 2: Create IOU Note

Alice issues a note to Bob for 0.05 ERG:

```bash
basis_cli note create \
  --demo \
  --amount 50000000 \
  --output alice_to_bob_note.json
```

### Step 3: Generate Redemption Transaction

Bob creates an unsigned transaction to redeem 0.025 ERG:

```bash
basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 25000000 \
  --output-file redemption_tx.json
```

### Step 4: Sign and Broadcast

Bob signs the transaction with his Ergo wallet:

```bash
curl -X POST http://localhost:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @redemption_tx.json
```

Then broadcasts:

```bash
curl -X POST http://localhost:9053/wallet/transaction/send \
  -H "Content-Type: application/json" \
  -H "api_key: bob-api-key" \
  -d @signed_tx.json
```

## Troubleshooting

See [specs/interactive_demo.md](../specs/interactive_demo.md) for detailed troubleshooting guide.

## References

- [Protocol Specification](../specs/spec.md)
- [Redemption CLI Specification](../specs/redemption_cli_spec.md)
- [Tracker Box Setup Guide](../docs/TRACKER_BOX_SETUP.md)
- [Scala Reference Demo](../scala/demo/README.md)

## Security Warning

Demo keys in `secrets/participants.csv` are for testing only. Never use them in production. In production:
- Generate secure keypairs
- Use hardware wallets or HSMs
- Protect private keys
- Monitor reserve collateralization
