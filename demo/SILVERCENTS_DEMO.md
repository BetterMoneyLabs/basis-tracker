# SilverCents Demo - Basis Protocol on Ergo

## Overview

SilverCents are on-chain tokens on the Ergo Platform using the Basis protocol, exchangeable one-for-one with constitutional silver dimes and quarters. This demo shows a complete workflow of:

1. **Issuance**: A silver merchant (Alice) issues silver-backed notes to customers (Bob)
2. **Redemption**: Customers redeem notes for physical silver coins at the merchant's location

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  SILVERCENTS ECOSYSTEM                       │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐      ┌──────────────┐      ┌───────────┐  │
│  │   Ergo Node  │      │ Basis Server │      │  Scanner  │  │
│  │  (Blockchain)│      │  (Tracker)   │      │ (Monitor) │  │
│  └──────────────┘      └──────────────┘      └───────────┘  │
│         ▲                      ▲                     ▲        │
│         │                      │                     │        │
│         └──────────────────────┼─────────────────────┘        │
│                                │                              │
│         ┌──────────────────────┴──────────────────────┐       │
│         │                                              │       │
│    ┌─────────────┐                          ┌─────────────┐  │
│    │   Alice     │ ◄──── Debt Notes ────►  │    Bob      │  │
│    │ (Merchant)  │    (SilverCents)        │ (Customer)  │  │
│    │  Reserve: 1 │                          │             │  │
│    │  million$   │                          │             │  │
│    └─────────────┘                          └─────────────┘  │
│         │                                        │             │
│         │   CREATE RESERVE                      │             │
│         │   (On-chain collateral)               │             │
│         │                                        │             │
│         │   ISSUE NOTES                         │             │
│         │   (Off-chain debt tracking)           │             │
│         │                                   REDEEM NOTES      │
│         │                                   (Exchange for     │
│         │                                    physical coins)  │
│         │                                        │             │
│         └────────────────────────────────────────┘             │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Key Concepts

### 1. Reserve (On-Chain)
- Issued by Alice (merchant), locked on the Ergo blockchain
- Holds collateral (ERG, silver tokenized, etc.)
- Prevents over-issuance of notes
- Redeemable for physical silver via reserve contracts

### 2. Debt Notes (Off-Chain)
- Created by Alice to Bob with her signature
- Represent right to redeem for silver
- Tracked by the Basis server (tracker)
- Can be redeemed on-chain to settle the debt

### 3. Tracker
- Maintains AVL+ tree of all notes
- Publishes periodic commitments on-chain
- Enables light clients to verify notes
- Protects against censorship via on-chain fallback

## Demo Workflow

### Phase 1: Setup
```bash
# Terminal 1: Start the Basis server
cd basis-tracker
cargo run -p basis_server

# Terminal 2: Start the Ergo scanner
# (if using real blockchain integration)
cargo run -p basis_store -- --scan-mode
```

### Phase 2: Initialize Accounts
```bash
# Create accounts for Alice (merchant) and Bob (customer)
./demo/silvercents_setup.sh
```

### Phase 3: Issue Notes
```bash
# Alice creates a reserve (on-chain collateral)
# Alice issues debt notes to Bob (off-chain)
./demo/silvercents_issuer.sh
```

### Phase 4: Receive Notes
```bash
# Bob monitors for new notes
# Bob tracks his accumulated debt
./demo/silvercents_receiver.sh
```

### Phase 5: Redeem Notes
```bash
# Bob redeems notes for physical silver
./demo/silvercents_redeem.sh
```

## Running the Complete Demo

### Quick Start (All-in-One)
```bash
./demo/silvercents_complete_demo.sh
```

### Step-by-Step Manual Demo

**Terminal 1 - Setup and prepare:**
```bash
cd demo
./silvercents_setup.sh
```

**Terminal 2 - Alice (Merchant) - Issuance:**
```bash
cd demo
./silvercents_issuer.sh
```

**Terminal 3 - Bob (Customer) - Receiving:**
```bash
cd demo
./silvercents_receiver.sh
```

**Terminal 4 - Bob (Customer) - Redemption:**
```bash
# When ready to redeem
cd demo
./silvercents_redeem.sh
```

## Demo Configuration

### Alice (Merchant)
- **Role**: Issues SilverCents notes
- **Reserve**: 1,000,000 units
- **Issue Interval**: 30 seconds
- **Note Amount**: 100-1,000 units per note
- **Collateral**: Physical silver stored at business location

### Bob (Customer)
- **Role**: Receives and redeems notes
- **Poll Interval**: 10 seconds
- **Min Collateralization**: 100% (1.0)
- **Redemption Frequency**: Upon request
- **Preferred Location**: Alice's business for in-person redemption

## Monitoring & Verification

### Check Note Status
```bash
# Check all notes for Alice (issuer)
curl http://localhost:3048/notes/issuer/[alice_pubkey]

# Check Bob's total notes
curl http://localhost:3048/notes/recipient/[bob_pubkey]
```

### Monitor Collateralization
```bash
# Get reserve status for Alice
curl http://localhost:3048/reserve/status/[alice_pubkey]

# Calculate collateralization ratio
collateralization = reserve_balance / total_notes_issued
```

### On-Chain Verification
```bash
# Check reserve on Ergo blockchain
curl http://localhost:9053/blockchain/boxes/[reserve_box_id]

# Verify tracker commitment
curl http://localhost:9053/blockchain/boxes/[tracker_box_id]
```

## Security Considerations

### 1. Signature Verification
- All notes signed by Alice using secp256k1
- Bob verifies signature before accepting notes
- Prevents note forgery and issuer repudiation

### 2. Collateralization
- Reserve balance must exceed total issued notes
- Automatic halt when ratio drops below 100%
- Periodic on-chain verification

### 3. Redemption Protection
- One-week timelock after note creation
- Prevents immediate reversal after issuance
- Allows for dispute resolution period

### 4. Tracker Accountability
- Periodic on-chain commitments
- AVL+ tree root hash published on-chain
- Light clients can verify tracker honesty

## Troubleshooting

### Notes Not Appearing
1. Check Basis server is running: `curl http://localhost:3048/status`
2. Verify note signatures: `openssl dgst -sha256 -verify pubkey.pem -signature sig.bin msg.bin`
3. Check tracker state: Look at recent events in server logs

### Redemption Failing
1. Verify reserve has sufficient balance
2. Check note timestamp meets one-week requirement
3. Ensure reserve box exists on-chain

### Collateralization Drop
1. Monitor Alice's reserve balance
2. Alert when ratio drops below 100%
3. Stop accepting new notes to prevent over-leverage

## Advanced Features

### Multi-Issuer Support
```bash
# Create multiple issuers with different reserves
./demo/silvercents_setup.sh --issuers alice,merchant2,merchant3
```

### Stress Testing
```bash
# Issue notes rapidly to test tracker capacity
./demo/silvercents_stress_test.sh --duration 5m --rate 100/s
```

### Historical Analysis
```bash
# Export and analyze all transactions
./demo/silvercents_export.sh > silvercents_ledger.csv
```

## Real-World Deployment

For production deployment of SilverCents:

1. **Physical Verification**: Actual silver coins stored in secure vault
2. **Regulatory Compliance**: State and federal money transmitter licenses
3. **Audit Trail**: Complete on-chain and off-chain audit logs
4. **Insurance**: Coverage for stored silver reserves
5. **Multi-Signature**: Require multiple merchant authorizations
6. **Geographic Distribution**: Multiple redemption locations

## References

- Basis Protocol: [specs/spec.md](../specs/spec.md)
- Server API: [specs/server/basis_server_spec.md](../specs/server/basis_server_spec.md)
- Client Reference: [specs/client/basis_cli_analysis.md](../specs/client/basis_cli_analysis.md)
- Ergo Documentation: [https://ergoplatform.org/docs/](https://ergoplatform.org/docs/)

## Next Steps

1. **Run the demo** to see SilverCents in action
2. **Modify parameters** in demo scripts for different scenarios
3. **Integrate with real Ergo node** for blockchain verification
4. **Deploy to production** with proper legal and security measures
5. **Build merchant and customer UIs** for user-friendly interaction
