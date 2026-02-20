# SilverCents - Basis Protocol Demo Suite

> **SilverCents**: Silver-backed cryptocurrency using the Basis protocol on the Ergo Platform

## 🎯 What is This?

A complete, production-quality demonstration of the **Basis protocol** applied to a real-world use case: silver-backed digital notes that can be issued, tracked, and redeemed for physical silver coins.

The demo shows:
- **Issuance**: A merchant (Alice) creates digital notes backed by physical silver
- **Tracking**: Off-chain ledger maintains all debt relationships
- **Verification**: Cryptographic signatures and on-chain commitments prove authenticity
- **Redemption**: Notes exchanged for physical silver at merchant locations
- **Risk Management**: Collateralization ratios prevent over-issuance

## 📚 Documentation Roadmap

### For Different Audiences

| I want to... | Read this... | Time |
|---|---|---|
| **Run the demo immediately** | [QUICKSTART.md](demo/QUICKSTART.md) | 5 min |
| **Understand how it works** | [SILVERCENTS_DEMO.md](demo/SILVERCENTS_DEMO.md) | 20 min |
| **Learn the technical details** | [SILVERCENTS_IMPLEMENTATION.md](demo/SILVERCENTS_IMPLEMENTATION.md) | 30 min |
| **Get implementation overview** | [SILVERCENTS_IMPLEMENTATION_SUMMARY.md](SILVERCENTS_IMPLEMENTATION_SUMMARY.md) | 15 min |
| **Review all scripts** | [demo/README_SILVERCENTS.md](demo/README_SILVERCENTS.md) | 10 min |
| **Study Basis protocol** | [specs/spec.md](specs/spec.md) | 45 min |
| **Explore server API** | [specs/server/basis_server_spec.md](specs/server/basis_server_spec.md) | 30 min |

## 🚀 Quick Start

### Run the Demo (5 minutes)

```bash
# Terminal 1: Start the server
cd basis-tracker
cargo run -p basis_server

# Terminal 2: Run the complete demo
cd demo
chmod +x silvercents_*.sh
./silvercents_complete_demo.sh
```

That's it! The demo will:
1. ✅ Create merchant and customer accounts
2. ✅ Alice issues silver-backed notes
3. ✅ Bob receives and verifies notes
4. ✅ Monitor collateralization in real-time
5. ✅ Bob redeems notes for physical silver
6. ✅ Generate comprehensive logs

## 📋 What's Included

### Demo Scripts (in `demo/` folder)

```
silvercents_setup.sh          # Initialize accounts
silvercents_issuer.sh         # Alice issues notes
silvercents_receiver.sh       # Bob receives notes
silvercents_redeem.sh         # Bob redeems notes
silvercents_complete_demo.sh  # Run everything automatically

alice_issuer.sh               # Original demo (simpler)
bob_receiver.sh               # Original demo (simpler)
```

### Documentation (14,000+ words)

```
QUICKSTART.md                                    # 5-minute guide
SILVERCENTS_DEMO.md                             # User guide
SILVERCENTS_IMPLEMENTATION.md                   # Technical details
SILVERCENTS_IMPLEMENTATION_SUMMARY.md           # Overview
demo/README_SILVERCENTS.md                      # Script guide
```

### Data Generation

```
/tmp/silvercents_demo/state/                    # Account files
/tmp/silvercents_demo/logs/                     # Transaction logs
```

## 💡 Key Concepts

### Debt Notes
```
Alice creates: "I owe Bob 1000 SilverCents"
├─ Signed by Alice (Schnorr signature)
├─ Stored in tracker's AVL+ tree
├─ Backed by Alice's on-chain reserve
└─ Redeemable for physical silver
```

### Collateralization
```
Reserve / Total Notes Issued = Ratio

✓ 100%+ → Healthy (accept notes)
⚠ 80-100% → Warning (be careful)
✗ <80% → Risky (stop accepting)
```

### On-Chain Verification
```
Tracker publishes digest on-chain
↓
Ergo blockchain confirms state
↓
Light clients verify honesty
↓
Fallback if tracker goes offline
```

## 🏗️ System Architecture

```
┌──────────────────────────────────────┐
│   CLI Scripts (Orchestration)         │
│   (alice_issuer, bob_receiver, etc)   │
└──────────────┬───────────────────────┘
               │ HTTP API
               ▼
┌──────────────────────────────────────┐
│   Basis Server (Tracker)              │
│   ├─ AVL+ Tree (Note Ledger)         │
│   ├─ Signature Verification          │
│   └─ Collateral Monitoring           │
└──────────────┬───────────────────────┘
               │ On-Chain Commitment
               ▼
┌──────────────────────────────────────┐
│   Ergo Blockchain                    │
│   ├─ Reserve UTXO                    │
│   ├─ Tracker Public Key (R4)         │
│   └─ AVL+ Root Digest (R5)           │
└──────────────────────────────────────┘
```

## 📊 Demo Workflow

### Phase 1: Setup (1 min)
```
Generate accounts → Create reserve → Initialize tracking
```

### Phase 2: Issuance (2 min)
```
Alice creates notes → Signs with private key → Submits to tracker
                   → Collateral decreases
                   → Ratio monitored
                   → Halts when exhausted
```

### Phase 3: Reception (2 min)
```
Bob polls tracker → Fetches new notes → Verifies signatures
                 → Accumulates amount → Tracks collateral
                 → Stops if ratio < 100%
```

### Phase 4: Redemption (1 min)
```
Bob initiates → Tracker verifies → Records on-chain
             → Reserve updated → Silver delivered
             → Notes marked redeemed
```

## 🔐 Security Features

### Cryptography
- **Algorithm:** Schnorr signatures with secp256k1
- **Message Format:** recipient_pubkey || amount || timestamp
- **Verification:** All notes verified before acceptance
- **Protection:** Prevents forgery and replay attacks

### Collateralization
- **Real-Time Monitoring:** Continuous ratio calculation
- **Automatic Halt:** Stops when ratio drops below 100%
- **Risk Management:** Prevents over-leverage
- **Warning Alerts:** At 80% utilization

### Trust & Verification
- **Signature Verification:** Cryptographic proof of authenticity
- **AVL+ Tree:** Efficient proof of all notes
- **On-Chain Commitment:** Periodic state published to blockchain
- **Fallback:** Last committed state valid if tracker fails

## 📈 Real-World Use

**SilverCents in Action:**

1. **Merchant Setup** 
   - Alice stocks vault with silver coins
   - Creates on-chain reserve (cryptographic proof)
   - Begins accepting customers

2. **Customer Purchases**
   - Bob buys items from Alice
   - Alice issues digital notes instead of coins
   - Notes backed by reserve, no counterparty risk

3. **Note Circulation**
   - Bob can spend notes with others
   - Tracker maintains accurate records
   - Collateral always verifiable on-chain

4. **Redemption**
   - Bob exchanges notes for physical silver
   - Can happen anytime, anywhere
   - Alice's reserve automatically updated

5. **Network Effects**
   - Other merchants join
   - Multiple issuers competing
   - Digital notes for physical assets

## 🎓 Learning Resources

### Included Documentation
- Protocol specification: `specs/spec.md`
- Server API: `specs/server/basis_server_spec.md`
- Cryptography: `specs/offchain/spec.md`
- AVL Trees: `specs/trees/trees.md`

### Code Examples
```rust
// Signature verification
crates/basis_offchain/src/schnorr.rs

// AVL+ tree implementation
crates/basis_store/src/avl_tree.rs

// Note operations
crates/basis_server/src/reserve_api.rs

// CLI client
crates/basis_cli/src/commands/
```

### Test Cases
```bash
cargo test -p basis_offchain schnorr    # Crypto tests
cargo test -p basis_store avl_tree      # Tree tests
cargo test -p basis_server note         # API tests
```

## 🧪 Testing the Demo

### Automated Testing
```bash
./silvercents_complete_demo.sh  # Full workflow
```

### Manual Testing

**Terminal 1 - Server:**
```bash
cargo run -p basis_server
```

**Terminal 2 - Setup:**
```bash
cd demo
./silvercents_setup.sh
```

**Terminal 3 - Issuer:**
```bash
./silvercents_issuer.sh
```

**Terminal 4 - Receiver:**
```bash
./silvercents_receiver.sh
```

**Terminal 5 - Redeem:**
```bash
./silvercents_redeem.sh
```

### Monitoring
```bash
# Watch Alice's activity
tail -f /tmp/silvercents_demo/logs/alice_issuer.log

# Watch Bob's activity
tail -f /tmp/silvercents_demo/logs/bob_receiver.log

# Check ledger
cat /tmp/silvercents_demo/logs/alice_ledger.csv

# API status
curl http://localhost:3048/status | jq
```

## 🔧 Configuration

### Server Settings
```bash
SERVER_URL=http://localhost:3048          # Default
SERVER_URL=http://myserver:8080 ./demo.sh # Custom
```

### Timing
```bash
ISSUE_INTERVAL=30           # Alice: Issue every 30s
POLL_INTERVAL=10            # Bob: Check every 10s
RESERVE_TOTAL=1000000       # Alice: Starting reserve
```

### Amounts
```bash
AMOUNT_MIN=100              # Minimum note amount
AMOUNT_MAX=1000             # Maximum note amount
MIN_COLLATERALIZATION=1.0   # Bob: Minimum ratio (100%)
```

## 📦 Files Generated

After running demo, check:

```
/tmp/silvercents_demo/
├── state/
│   ├── alice_account.txt        # Alice's keys
│   └── bob_account.txt          # Bob's keys
└── logs/
    ├── alice_issuer.log         # Alice activity log
    ├── alice_ledger.csv         # Notes issued (CSV)
    ├── bob_receiver.log         # Bob activity log  
    ├── bob_notes.csv            # Notes received (CSV)
    ├── bob_redemption.log       # Redemption details
    └── redemptions.csv          # Completed redemptions
```

## 🎯 Next Steps

1. **Run the Demo** → See protocol in action
2. **Study the Code** → Understand implementation
3. **Review Docs** → Learn Basis protocol
4. **Modify Scripts** → Experiment with parameters
5. **Build on It** → Create applications

## 📖 Further Reading

- **Quick Reference:** [QUICKSTART.md](demo/QUICKSTART.md)
- **Complete Guide:** [SILVERCENTS_DEMO.md](demo/SILVERCENTS_DEMO.md)
- **Technical Deep Dive:** [SILVERCENTS_IMPLEMENTATION.md](demo/SILVERCENTS_IMPLEMENTATION.md)
- **Implementation Overview:** [SILVERCENTS_IMPLEMENTATION_SUMMARY.md](SILVERCENTS_IMPLEMENTATION_SUMMARY.md)
- **Protocol Spec:** [specs/spec.md](specs/spec.md)

## 💬 Questions?

### For Usage Help
See [demo/QUICKSTART.md](demo/QUICKSTART.md)

### For Technical Questions
See [demo/SILVERCENTS_IMPLEMENTATION.md](demo/SILVERCENTS_IMPLEMENTATION.md)

### For Protocol Details
See [specs/spec.md](specs/spec.md)

### For Troubleshooting
See [demo/SILVERCENTS_DEMO.md](demo/SILVERCENTS_DEMO.md) Troubleshooting section

## 📄 License

See LICENSE file in the repository.

## 🚀 Welcome to SilverCents!

Start with: `./demo/silvercents_complete_demo.sh`

Happy exploring! 🪙

---

**Last Updated:** December 2024  
**Status:** ✅ Complete and Tested  
**Version:** 1.0.0
