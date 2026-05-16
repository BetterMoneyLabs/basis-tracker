# SilverCents Demo - Quick Start Guide

## TL;DR - Run the Demo in 3 Steps

### Step 1: Start the Server
```bash
cd basis-tracker
cargo run -p basis_server
```

### Step 2: Run the Complete Demo
```bash
cd demo
chmod +x silvercents_*.sh
./silvercents_complete_demo.sh
```

### Step 3: Review the Results
```bash
# Check Alice's issuance
cat /tmp/silvercents_demo/logs/alice_issuer.log

# Check Bob's notes
cat /tmp/silvercents_demo/logs/bob_receiver.log

# View transaction ledger
cat /tmp/silvercents_demo/logs/alice_ledger.csv
```

---

## What is SilverCents?

**SilverCents** is a practical implementation of the Basis protocol - an off-chain cash system backed by on-chain collateral.

```
Traditional Money Flow:
  Alice → Silver Coins → Bob

SilverCents Flow:
  Alice → Digital Notes (Backed by Reserve) → Bob → Redeem for Silver
```

**Key Innovation:** Debt notes issued off-chain but backed by on-chain reserves. This enables:
- Fast, free transactions (no blockchain fees)
- Instant settlement between trust parties
- On-chain redemption capability
- Cryptographic verification

---

## Architecture Overview

### 1. The Merchant (Alice)
- **Role:** Issues SilverCents notes to customers
- **Reserve:** Holds physical silver in vault
- **On-Chain:** Reserve NFT proves collateral backing
- **Action:** Issues notes when customers make purchases

### 2. The Customer (Bob)
- **Role:** Receives and accumulates notes
- **Trust:** Verifies Alice's reserve is sufficient
- **Monitoring:** Tracks collateralization ratio
- **Redemption:** Exchanges notes for physical silver

### 3. The Tracker (Basis Server)
- **Role:** Maintains ledger of all notes
- **Storage:** AVL+ tree with all debt relationships
- **Verification:** Ensures notes are signed correctly
- **Commitment:** Periodically publishes state on-chain

### 4. The Collateral (Ergo Blockchain)
- **Proof:** Reserve UTXO proves Alice's backing
- **Security:** Cryptographic verification
- **Permanence:** Immutable transaction history
- **Redemption:** Final settlement happens on-chain

---

## Demo Workflow

### Phase 1: Setup
```
✓ Create merchant account (Alice)
✓ Create customer account (Bob)
✓ Initialize reserve tracking
✓ Set up demo databases
```

### Phase 2: Issuance
```
Alice's Actions:
  ├─ Creates on-chain reserve (1M units of silver)
  ├─ Issues notes to Bob every 30 seconds
  ├─ Monitors collateralization ratio
  ├─ Stops when ratio drops below 100%
  └─ Each note: amount, timestamp, signature

Tracker's Actions:
  ├─ Stores each note in AVL+ tree
  ├─ Verifies Alice's signature
  ├─ Updates root digest
  └─ Records in ledger
```

### Phase 3: Reception
```
Bob's Actions:
  ├─ Polls tracker every 10 seconds
  ├─ Fetches new notes addressed to him
  ├─ Verifies signatures
  ├─ Calculates collateralization
  ├─ Accumulates notes
  └─ Stops accepting if ratio < 100%

Result:
  Bob now holds digital notes worth X units
  Backed by Alice's reserve of Y units
  Collateralization = Y / X
```

### Phase 4: Redemption
```
Bob's Actions:
  ├─ Presents accumulated notes
  ├─ Verifies signatures are valid
  ├─ Records redemption request

Alice's Actions:
  ├─ Verifies notes are legitimate
  ├─ Records redemption on-chain
  ├─ Reduces reserve balance
  └─ Delivers physical silver coins

Result:
  Bob receives constitutional silver coins
  Alice's reserve decreases
  Notes removed from circulation
```

---

## Key Concepts

### Debt Notes
```
Format: (issuer, recipient, amount, timestamp, signature)

Example:
  Issuer:    Alice (02a1b2c3...)
  Recipient: Bob (02d4e5f6...)
  Amount:    1,000 units
  Time:      1703001234
  Sig:       02abc...def (65 bytes)

Meaning: "Alice owes Bob 1,000 units of value"
```

### Collateralization Ratio
```
Ratio = Reserve / Total Notes Issued

Example:
  Alice's Reserve:     1,000,000 units
  Bob's Accumulated:   500,000 units
  Collateralization:   200% (healthy)

  If Alice issues too much:
  Reserve:            1,000,000 units
  Bob's Accumulated: 1,500,000 units
  Collateralization:   67% (risky!) → Stop accepting
```

### Cryptographic Verification
```
Alice Creates Note:
  1. Message = Bob's pubkey || amount || timestamp
  2. Signs with her private key
  3. Creates 65-byte Schnorr signature
  4. Sends to tracker

Bob Receives Note:
  1. Gets note from tracker
  2. Verifies signature using Alice's public key
  3. Confirms it was created by Alice (not forged)
  4. Accepts the note
```

### On-Chain Commitment
```
Every hour (configurable):
  ├─ Tracker computes AVL+ tree root
  ├─ Root = Hash of all notes and amounts
  ├─ Publishes root to Ergo blockchain
  ├─ Stored in R5 register of reserve UTXO
  └─ Light clients can verify tracker honesty

If tracker disappears:
  └─ Last on-chain commitment remains valid
  └─ Notes can be redeemed using on-chain proof
```

---

## File Structure

```
demo/
├── SILVERCENTS_DEMO.md                 # User guide
├── SILVERCENTS_IMPLEMENTATION.md       # Technical deep dive
├── silvercents_setup.sh                # Initialize accounts
├── silvercents_issuer.sh              # Alice issues notes
├── silvercents_receiver.sh            # Bob receives notes
├── silvercents_redeem.sh              # Bob redeems notes
└── silvercents_complete_demo.sh       # Run everything

/tmp/silvercents_demo/                 # Demo data
├── state/
│   ├── alice_account.txt              # Alice's keys & reserve
│   └── bob_account.txt                # Bob's keys
└── logs/
    ├── alice_issuer.log               # Alice's activity
    ├── alice_ledger.csv               # Notes issued
    ├── bob_receiver.log               # Bob's activity
    ├── bob_notes.csv                  # Notes received
    ├── bob_redemption.log             # Redemption details
    └── redemptions.csv                # Completed redemptions
```

---

## Common Commands

### 1. Start Everything

```bash
# Terminal 1: Server
cargo run -p basis_server

# Terminal 2: Demo
cd demo
./silvercents_complete_demo.sh
```

### 2. Custom Configuration

```bash
# Change server URL
SERVER_URL=http://myserver:3048 ./silvercents_issuer.sh

# Change timing
ISSUE_INTERVAL=10 ./silvercents_issuer.sh
POLL_INTERVAL=5 ./silvercents_receiver.sh
```

### 3. View Results

```bash
# Real-time monitoring
tail -f /tmp/silvercents_demo/logs/*.log

# Summary statistics
wc -l /tmp/silvercents_demo/logs/*.csv

# Export for analysis
cat /tmp/silvercents_demo/logs/alice_ledger.csv | sort -t',' -k5 -n
```

### 4. API Calls

```bash
# Check server status
curl http://localhost:3048/status

# Get Alice's reserve status
curl http://localhost:3048/reserve/status/[alice_pubkey]

# Query notes for an issuer
curl http://localhost:3048/notes/issuer/[alice_pubkey]

# Query notes for a recipient
curl http://localhost:3048/notes/recipient/[bob_pubkey]
```

---

## Interpreting Results

### Alice's Log
```
2024-01-20 10:00:00] ✓ Reserve created: 1000000 units
2024-01-20 10:00:30] ✓ Note #1 issued: 150 units
2024-01-20 10:01:00] ✓ Note #2 issued: 200 units
2024-01-20 10:01:30] ✓ Note #3 issued: 175 units
...
Collateralization: 96.5% (still accepting)
Collateralization: 92.3% (warning)
Collateralization: 87.1% (critical) → Stop issuing
```

### Bob's Log
```
2024-01-20 10:00:31] ✓ Received note: 150 units
2024-01-20 10:01:01] ✓ Received note: 200 units
2024-01-20 10:01:31] ✓ Received note: 175 units
...
Total accumulated: 525 units
Collateralization: 190.5% ✓ Safe
Collateralization: 95.2% (borderline)
Collateralization: 87.1% ✗ Stop accepting
```

### Ledger CSV
```
TIMESTAMP,ISSUER,RECIPIENT,AMOUNT,SIGNATURE,STATUS
1703001230,02a1b2c3...,02d4e5f6...,150,02abc...def,ISSUED
1703001260,02a1b2c3...,02d4e5f6...,200,02xxx...yyy,ISSUED
1703001290,02a1b2c3...,02d4e5f6...,175,02zzz...www,ISSUED
```

---

## Troubleshooting

### Problem: "Cannot connect to Basis server"
**Solution:**
```bash
# Check if server is running
curl http://localhost:3048/status

# Start server if needed
cd basis-tracker
cargo run -p basis_server
```

### Problem: "Collateralization drops too fast"
**Solution:**
```bash
# Adjust note amounts in script
AMOUNT_MIN=50      # Smaller notes
AMOUNT_MAX=200
./silvercents_issuer.sh
```

### Problem: "No notes received"
**Solution:**
```bash
# Check Alice issued notes
cat /tmp/silvercents_demo/logs/alice_ledger.csv

# Check Bob's polling
tail -f /tmp/silvercents_demo/logs/bob_receiver.log

# Verify tracker has notes
curl http://localhost:3048/notes/issuer/[alice_pubkey] | jq
```

### Problem: "Redemption failed"
**Solution:**
```bash
# Verify notes exist
cat /tmp/silvercents_demo/logs/bob_notes.csv

# Check Alice's reserve status
curl http://localhost:3048/reserve/status/[alice_pubkey]

# Ensure timestamps are valid (older than 1 week for real blockchain)
```

---

## Learning Resources

### Included Documentation
1. **SILVERCENTS_DEMO.md** - Complete user guide
2. **SILVERCENTS_IMPLEMENTATION.md** - Technical details
3. **../specs/spec.md** - Basis protocol specification
4. **../specs/server/basis_server_spec.md** - Server API details

### Key Concepts to Study
- Off-chain cash and IOUs
- Elliptic curve cryptography (secp256k1)
- Schnorr signatures
- AVL+ trees for commitments
- Collateralization ratios
- Reserve management

### Code Examples
- Cryptography: `crates/basis_offchain/src/schnorr.rs`
- Storage: `crates/basis_store/src/avl_tree.rs`
- API: `crates/basis_server/src/reserve_api.rs`
- CLI: `crates/basis_cli/src/commands/note.rs`

---

## Next Steps

1. **Run the demo** to see it in action
2. **Study the code** to understand implementation
3. **Modify parameters** to explore different scenarios
4. **Read the specs** for protocol details
5. **Deploy locally** with real Ergo node
6. **Test on testnet** with actual blockchain
7. **Deploy to production** with security measures

---

## Support

### Documentation
- See `SILVERCENTS_DEMO.md` for detailed guide
- See `SILVERCENTS_IMPLEMENTATION.md` for technical depth
- See `../specs/` for protocol specifications

### Testing
```bash
# Run all tests
cargo test

# Specific tests
cargo test -p basis_server note_creation

# With detailed output
cargo test -- --nocapture --test-threads=1
```

### Contributing
To extend the demo:
1. Review existing code
2. Create feature branch
3. Implement changes
4. Add tests
5. Submit pull request

---

## License

This demo is part of the Basis Tracker project. See LICENSE file for details.
