# SilverCents Demo Suite

Complete demonstration of the Basis protocol with a silver-backed cryptocurrency use case.

## 📚 Documentation

Start here based on what you want to do:

| Document | Purpose |
|----------|---------|
| **[QUICKSTART.md](QUICKSTART.md)** | 🚀 Get running in 5 minutes |
| **[SILVERCENTS_DEMO.md](SILVERCENTS_DEMO.md)** | 📖 Complete user guide & scenarios |
| **[SILVERCENTS_IMPLEMENTATION.md](SILVERCENTS_IMPLEMENTATION.md)** | 🔧 Technical deep dive |

## 🎯 Quick Start

```bash
# Terminal 1: Start server
cd ..
cargo run -p basis_server

# Terminal 2: Run demo
cd demo
./silvercents_complete_demo.sh
```

That's it! The demo will run through all phases: setup, issuance, reception, and redemption.

## 📋 Demo Scripts

### Interactive Demos (Manual Control)

| Script | Role | Purpose |
|--------|------|---------|
| `silvercents_setup.sh` | System | Initialize accounts and reserve |
| `silvercents_issuer.sh` | Alice | Issue silver-backed notes |
| `silvercents_receiver.sh` | Bob | Receive and track notes |
| `silvercents_redeem.sh` | Bob | Redeem notes for silver |

### Automated Demo

| Script | Purpose |
|--------|---------|
| `silvercents_complete_demo.sh` | Run entire workflow automatically |

### Legacy Scripts (Simpler Version)

| Script | Purpose |
|--------|---------|
| `alice_issuer.sh` | Original Alice script |
| `bob_receiver.sh` | Original Bob script |

## 🎬 Demo Scenarios

### Scenario 1: Complete End-to-End (Recommended)
```bash
./silvercents_complete_demo.sh
```
- ✓ Automatic setup
- ✓ Alice issues notes
- ✓ Bob receives notes
- ✓ Bob redeems notes
- ✓ Full logging and reporting

### Scenario 2: Manual Step-by-Step
```bash
# Terminal 1
./silvercents_setup.sh

# Terminal 2
./silvercents_issuer.sh

# Terminal 3
./silvercents_receiver.sh

# Terminal 4 (when ready)
./silvercents_redeem.sh
```

### Scenario 3: Quick Test (Legacy)
```bash
# Terminal 1
./alice_issuer.sh

# Terminal 2
./bob_receiver.sh
```

## 🧩 System Components

```
┌─────────────────────────────────────────┐
│      Ergo Blockchain (On-Chain)         │
│                                         │
│  Alice's Reserve UTXO                   │
│  ├─ Collateral Value                   │
│  ├─ Tracker Public Key (R4)             │
│  └─ AVL+ Tree Root (R5)                 │
└─────────────────────────────────────────┘
           ▲
           │ Verify & Redeem
           ▼
┌─────────────────────────────────────────┐
│    Basis Server - Tracker (Off-Chain)    │
│                                         │
│  AVL+ Tree of Debt Notes                │
│  • Key: H(issuer || recipient)          │
│  • Value: (amount, timestamp)           │
│  • Root: Published on-chain              │
└─────────────────────────────────────────┘
           ▲
           │ HTTP API
           ▼
┌─────────────────────────────────────────┐
│         CLI Clients                      │
│                                         │
│  Alice: Merchant/Issuer                 │
│  • Create reserve                       │
│  • Issue notes                          │
│  • Monitor collateral                   │
│                                         │
│  Bob: Customer/Recipient                │
│  • Receive notes                        │
│  • Verify authenticity                  │
│  • Track collateralization              │
│  • Request redemption                   │
└─────────────────────────────────────────┘
```

## 💰 The SilverCents Protocol

### What is SilverCents?
- **On-chain tokens** on Ergo Platform using Basis protocol
- **Exchangeable 1:1** with constitutional silver dimes and quarters
- **Merchant-issued** digital notes backed by physical silver
- **Instantly redeemable** at merchant locations

### Key Concepts

**Debt Note:**
```
Alice issues to Bob: "I owe you 1000 SilverCents"
- Signed by Alice (cryptographic proof)
- Stored in tracker's AVL+ tree
- Backed by Alice's on-chain reserve
- Redeemable for physical silver
```

**Collateralization:**
```
Ratio = Reserve / Total Notes Issued

Example:
  Alice's Reserve: 1,000,000 units
  Bob's Notes:       500,000 units
  Collateralization:     200% ✓ Healthy

If Alice issues too much:
  Collateralization: 75% ✗ Too much risk
  Bob stops accepting notes
```

**Redemption:**
```
Bob presents notes to Alice
↓
Alice verifies via tracker
↓
Recorded on-chain
↓
Bob receives physical silver coins
```

## 📊 Data Files

Demo creates the following structure in `/tmp/silvercents_demo/`:

```
/tmp/silvercents_demo/
├── state/
│   ├── alice_account.txt       # Alice's keys & initial reserve
│   └── bob_account.txt         # Bob's keys
├── logs/
│   ├── alice_issuer.log        # Alice's activity log
│   ├── alice_ledger.csv        # Notes issued (CSV format)
│   ├── bob_receiver.log        # Bob's activity log
│   ├── bob_notes.csv           # Notes received (CSV format)
│   ├── bob_redemption.log      # Redemption details
│   └── redemptions.csv         # Completed redemptions
```

## 🔍 Monitoring & Verification

### View Alice's Activity
```bash
tail -f /tmp/silvercents_demo/logs/alice_issuer.log
```

### View Bob's Activity
```bash
tail -f /tmp/silvercents_demo/logs/bob_receiver.log
```

### Check Notes Issued
```bash
cat /tmp/silvercents_demo/logs/alice_ledger.csv
```

### Check Notes Received
```bash
cat /tmp/silvercents_demo/logs/bob_notes.csv
```

### API Status Check
```bash
curl http://localhost:3048/status | jq
```

## 🛠️ Configuration

### Server URL
```bash
SERVER_URL=http://myserver:3048 ./silvercents_issuer.sh
```

### Timing Parameters
```bash
# Alice: Issue notes more frequently
ISSUE_INTERVAL=15 ./silvercents_issuer.sh

# Bob: Check for notes more frequently
POLL_INTERVAL=5 ./silvercents_receiver.sh
```

### Reserve & Amounts
```bash
# Alice: Change initial reserve
RESERVE_TOTAL=500000 ./silvercents_issuer.sh

# Alice: Change note amounts
AMOUNT_MIN=50 AMOUNT_MAX=500 ./silvercents_issuer.sh
```

## 🚀 Advanced Usage

### Run Multiple Iterations
```bash
for i in {1..3}; do
  ./silvercents_complete_demo.sh
  echo "Iteration $i complete"
  sleep 10
done
```

### Extract Statistics
```bash
# Count notes issued
wc -l /tmp/silvercents_demo/logs/alice_ledger.csv

# Total amount issued
awk -F',' '{sum+=$4} END {print sum}' /tmp/silvercents_demo/logs/alice_ledger.csv

# Average note size
awk -F',' '{sum+=$4; count++} END {print sum/count}' /tmp/silvercents_demo/logs/alice_ledger.csv
```

### Monitor in Real-Time
```bash
watch -n 1 'tail -5 /tmp/silvercents_demo/logs/alice_issuer.log'
```

## 📚 Learning Resources

### Protocol
- **Basis Spec**: See `../specs/spec.md`
- **Server Details**: See `../specs/server/basis_server_spec.md`
- **Cryptography**: See `../specs/offchain/spec.md`

### Implementation
- **Client Code**: `../crates/basis_cli/src/`
- **Server Code**: `../crates/basis_server/src/`
- **Crypto**: `../crates/basis_offchain/src/schnorr.rs`

### Testing
```bash
# Run tests
cargo test -p basis_server

# Run specific test
cargo test -p basis_server note_creation

# With output
cargo test -- --nocapture
```

## ⚙️ Prerequisites

### Required
- Bash 4.0+
- curl
- bc (for calculations)

### Recommended
- jq (better JSON parsing)
- watch (monitor logs)

### Installation
```bash
# Ubuntu/Debian
sudo apt-get install curl bc jq

# macOS
brew install curl jq
```

## 🐛 Troubleshooting

### Server Not Running?
```bash
# Check if server is up
curl http://localhost:3048/status

# Start server
cd ..
cargo run -p basis_server
```

### Scripts Not Executable?
```bash
chmod +x silvercents_*.sh
```

### No Notes Appearing?
```bash
# Check Alice issued notes
grep ISSUED /tmp/silvercents_demo/logs/*.log

# Check tracker API
curl http://localhost:3048/notes/issuer/[alice_pubkey]
```

### Redemption Failing?
```bash
# Verify notes exist
cat /tmp/silvercents_demo/logs/bob_notes.csv | head

# Check reserve status
curl http://localhost:3048/reserve/status/[alice_pubkey]
```

See **[SILVERCENTS_DEMO.md](SILVERCENTS_DEMO.md)** for detailed troubleshooting.

## 📝 Examples

### Example 1: Issuing 50 Notes
```bash
# Modify script to issue quickly
ISSUE_INTERVAL=2 AMOUNT_MIN=100 AMOUNT_MAX=100 ./silvercents_issuer.sh
```

### Example 2: High Collateralization
```bash
# Start with large reserve, small notes
RESERVE_TOTAL=10000000 AMOUNT_MAX=1000 ./silvercents_issuer.sh
```

### Example 3: Stress Test
```bash
# Rapid issuance
ISSUE_INTERVAL=1 AMOUNT_MIN=10 AMOUNT_MAX=50 ./silvercents_issuer.sh &
POLL_INTERVAL=1 ./silvercents_receiver.sh
```

## 🎓 Educational Outputs

The demo teaches:

1. **Cryptography**: Schnorr signatures with secp256k1
2. **Economics**: Collateralization and reserve management
3. **Distributed Systems**: Off-chain data with on-chain verification
4. **Blockchain**: Reserve tracking and commitment proofs
5. **Trust**: Verification without trusted intermediaries

## 📞 Support

### Documentation
- QUICKSTART.md - 5-minute start
- SILVERCENTS_DEMO.md - User guide
- SILVERCENTS_IMPLEMENTATION.md - Technical details

### Help
```bash
# See what commands are available
grep "^Commands:" silvercents_*.sh

# Get specific help
grep -A 10 "Help:" silvercents_demo.sh
```

## 📄 License

See LICENSE in the main project directory.

## 🔗 Related

- [Basis Protocol](../specs/spec.md)
- [Server Spec](../specs/server/basis_server_spec.md)
- [Ergo Documentation](https://ergoplatform.org/)
