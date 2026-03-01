# SilverCents - Basis Protocol Implementation Guide

## Overview

SilverCents is a practical demonstration of the Basis protocol implemented on the Ergo Platform. It shows how off-chain credit notes can be backed by on-chain reserve collateral, using silver coins as the real-world store of value.

## System Architecture

### Components

```
┌─────────────────────────────────────────────────────────────┐
│                    On-Chain (Ergo)                          │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Reserve Contract Box                                        │
│  ├─ Owner: Alice's public key (33 bytes, secp256k1)          │
│  ├─ Value: Physical collateral (ERG or tokens)              │
│  ├─ R4: Tracker public key                                   │
│  └─ R5: AVL+ tree root (all notes digest)                   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ▲
                           │ Verification & Redemption
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                  Off-Chain (Basis Server)                    │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Tracker Service                                             │
│  ├─ AVL+ Tree: All debt notes (hash(issuer||recipient))     │
│  ├─ Storage: Note amounts and timestamps                    │
│  ├─ Verification: Issuer signature checks                    │
│  └─ Commitment: Periodic on-chain digest updates             │
│                                                               │
│  Note Format:                                                │
│  ├─ Issuer pubkey (33 bytes)                                │
│  ├─ Recipient pubkey (33 bytes)                             │
│  ├─ Amount (u64, in basic units)                            │
│  ├─ Timestamp (u64, seconds since epoch)                    │
│  └─ Signature (65 bytes, secp256k1 Schnorr)                │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                           ▲
                           │ HTTP API
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                  CLI Clients (Alice & Bob)                   │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Alice (Merchant/Issuer)                                     │
│  ├─ Create on-chain reserve                                │
│  ├─ Issue debt notes to customers                          │
│  ├─ Monitor collateralization                              │
│  └─ Redeem notes from reserve                              │
│                                                               │
│  Bob (Customer/Recipient)                                    │
│  ├─ Monitor notes issued to him                             │
│  ├─ Verify note authenticity                                │
│  ├─ Calculate collateralization                             │
│  └─ Request redemption                                      │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Cryptography

### Secp256k1 Keys

All participants use secp256k1 elliptic curve cryptography (Bitcoin standard):

**Private Key:** 32 bytes
```
Format: 32-byte secret scalar
Example: a1b2c3d4...
```

**Public Key:** 33 bytes (compressed)
```
Format: 02/03 prefix + x-coordinate (256-bit)
Example: 02a1b2c3d4e5f6...
Length: 66 hex characters
```

### Signature Scheme

SilverCents uses Schnorr signatures with the following properties:

**Signature:** 65 bytes
```
Structure:
  - Commitment point a: 33 bytes
  - Response scalar z:  32 bytes

Format: 130 hex characters
Example: 02abc...def1234...789
```

**Signing Process:**

1. **Message Construction:**
   ```
   message = recipient_pubkey || amount_be_bytes || timestamp_be_bytes
   ```

2. **Signature Creation (Schnorr):**
   ```
   k = random_scalar()
   a = g^k (commitment point)
   e = H(a || message || issuer_pubkey) (challenge)
   z = k + e*s (mod n) (response)
   signature = a || z (65 bytes)
   ```

3. **Verification:**
   ```
   e = H(a || message || issuer_pubkey)
   Verify: g^z = a * x^e (where x is issuer's public key)
   ```

## Data Model

### Debt Note

Each note represents a debt from issuer (Alice) to recipient (Bob).

```rust
pub struct Note {
    pub issuer_pubkey: [u8; 33],        // Alice's public key
    pub recipient_pubkey: [u8; 33],     // Bob's public key
    pub amount: u64,                    // Total debt amount
    pub timestamp: u64,                 // Latest payment time
    pub signature: [u8; 65],            // Alice's Schnorr signature
}
```

**Storage Key:** `H(issuer_pubkey || recipient_pubkey)`

**Invariants:**
- One note per (issuer, recipient) pair
- Timestamp always increases (newer updates only)
- Amount is cumulative (total debt, not delta)
- Signature must verify against issuer's public key

### Reserve

The on-chain collateral backing notes.

```rust
pub struct Reserve {
    pub owner: [u8; 33],                // Alice's public key
    pub nft_id: String,                 // Tracker NFT ID
    pub balance: u64,                   // Available collateral
    pub collateral_ratio: f64,          // balance / total_notes_issued
}
```

**On-Chain Representation:**
- UTXO box containing collateral value
- R4 register: Tracker public key
- R5 register: AVL+ tree root digest
- Redeemable by presenting valid note with timestamp proof

### AVL+ Tree

The tracker maintains an AVL+ tree of all notes for efficient commitment.

```
Tree Structure:
├─ Key: H(issuer || recipient)
├─ Value: (amount, timestamp)
└─ Root: Digest published on-chain in R5

Properties:
  • Balanced (AVL properties)
  • Self-certifying (root digest authenticates entire tree)
  • Efficient proofs (O(log n))
  • Persistent (all historical versions)
```

## Protocol Flows

### 1. Note Issuance

**Participants:** Alice (issuer), Tracker (server), Bob (recipient)

```
Alice              Tracker              Bob
  │                  │                   │
  ├──CREATE NOTE──►│                   │
  │  (signed)      │                   │
  │                ├─STORE in AVL+     │
  │                ├─UPDATE ROOT       │
  │                │                   │
  │                ◄──CONFIRM──┤       │
  │                │           │       │
  │                ├──BROADCAST────────►│
  │                │  (via API)        │
  │                │                   │
  └────────────────┴───────────────────┘
```

**Steps:**

1. Alice creates a note with:
   - Recipient: Bob's public key
   - Amount: Number of units
   - Timestamp: Current time
   - Signature: Created over message

2. Tracker verifies:
   - Issuer's signature is valid
   - Timestamp is newer than previous note
   - Amount is within reserve limits

3. Tracker updates:
   - AVL+ tree with new/updated note
   - Root digest
   - Available reserve balance

4. Bob queries tracker:
   - Fetches notes addressed to him
   - Verifies issuer's signature
   - Checks collateralization

### 2. Collateralization Monitoring

**Calculation:**
```
collateralization_ratio = reserve_balance / total_notes_issued

Target: ≥ 1.0 (100%)
Warning: 0.8 - 1.0 (80-100%)
Critical: < 0.8 (< 80%)
```

**Bob's Policy:**
- Accept notes while ratio ≥ 100%
- Stop accepting if ratio < 100%
- Use to decide when to redeem

**Alice's Policy:**
- Prevent issuance if ratio would drop below 100%
- Automatically halt to prevent over-leverage

### 3. Note Redemption

**Participants:** Bob, Tracker, Alice, Ergo blockchain

```
Bob                Tracker          Alice's Reserve    Ergo
  │                  │              (on-chain)         │
  ├──REDEEM REQUEST──►│               │                 │
  │  (amount, proof)  │               │                 │
  │                  ├──VERIFY────────►               │
  │                  │  note & proof   │                 │
  │                  │               ◄──VERIFIED───────│
  │                  ├──PUBLISH────────────────────────►│
  │                  │  redemption     │                 │
  │                  │  proof          │                 │
  │                  │               ◄──UPDATE────────│
  │                  │  (reduce        │                 │
  │                  │   balance)      │                 │
  │                  │                 │                 │
  │                ◄──CONFIRM──┤       │                 │
  │                  │         │       │                 │
  └────────────────────────────┴──────────────────────┘
      (collect physical silver)
```

**Steps:**

1. Bob submits redemption request:
   - Issuer (Alice)
   - Amount to redeem
   - Note timestamp proof

2. Tracker verifies:
   - Bob's note exists
   - Timestamp meets one-week requirement
   - Amount doesn't exceed available note

3. Tracker publishes redemption:
   - Creates on-chain proof
   - Reduces reserve

4. Alice delivers:
   - Physical silver coins at shop
   - Confirmed via receipt/signature

## API Endpoints

### Note Creation

```bash
POST /notes
Content-Type: application/json

{
  "issuer_pubkey": "02a1b2c3...",      # 33 bytes, 66 hex chars
  "recipient_pubkey": "02d4e5f6...",   # 33 bytes, 66 hex chars
  "amount": 1000,                       # Units of silver
  "timestamp": 1703001234,              # Unix timestamp
  "signature": "02abc...z789"           # 65 bytes, 130 hex chars
}

Response: 201 Created
{
  "note_id": "hash(issuer||recipient)",
  "amount": 1000,
  "timestamp": 1703001234,
  "collateralization": 1.05
}
```

### Query Notes

```bash
# Notes by issuer
GET /notes/issuer/:pubkey

# Notes by recipient
GET /notes/recipient/:pubkey

Response: 200 OK
{
  "data": [
    {
      "issuer_pubkey": "02a1b2c3...",
      "recipient_pubkey": "02d4e5f6...",
      "amount": 1000,
      "timestamp": 1703001234,
      "signature": "02abc...z789"
    }
  ]
}
```

### Reserve Status

```bash
GET /reserve/status/:issuer_pubkey

Response: 200 OK
{
  "issuer": "02a1b2c3...",
  "total_notes": 5000,
  "reserve_balance": 1000000,
  "collateralization": 200.0,
  "status": "HEALTHY"
}
```

### Redemption

```bash
POST /redeem
Content-Type: application/json

{
  "issuer_pubkey": "02a1b2c3...",
  "recipient_pubkey": "02d4e5f6...",
  "amount": 1000,
  "timestamp": 1703001234
}

Response: 200 OK
{
  "redemption_id": "...",
  "status": "PROCESSING",
  "on_chain_proof": "..."
}
```

## Security Considerations

### 1. Signature Verification

**Threat:** Note forgery

**Mitigation:**
- Each note signed by issuer
- Signature verified before acceptance
- Schnorr signature prevents signature reuse
- Message includes amount and timestamp

### 2. Collateralization

**Threat:** Over-issuance of unsupported notes

**Mitigation:**
- On-chain reserve proves backing
- Ratio monitoring prevents over-leverage
- Automatic halt at 100% collateralization
- Bob stops accepting when ratio drops

### 3. Tracker Honesty

**Threat:** Tracker removes notes (censorship)

**Mitigation:**
- Periodic on-chain commitments
- AVL+ tree root in R5 register
- Light clients can verify honesty
- Fallback to on-chain state if tracker goes offline

### 4. Timestamp Ordering

**Threat:** Replay or timestamp manipulation

**Mitigation:**
- Timestamps always increase
- One-week lock after issuance
- Prevents immediate reversal
- Allows dispute resolution period

## Configuration

### Server Configuration

File: `config/basis.toml`

```toml
[server]
host = "127.0.0.1"
port = 3048

[tracker]
nft_id = "0000...0001"
enable_avl_tree = true
periodic_commit = 3600  # seconds

[security]
min_collateralization = 1.0
signature_scheme = "schnorr-secp256k1"
```

### Ergo Node Configuration

File: `config/ergo_nodes.toml`

```toml
[mainnet]
url = "http://159.89.116.15:11088"
timeout = 30

[testnet]
url = "http://213.239.193.208:9052"
timeout = 30

[local]
url = "http://localhost:9053"
timeout = 30
```

## Demo Execution

### Quick Start

```bash
# Terminal 1: Start server
cargo run -p basis_server

# Terminal 2: Run complete demo
cd demo
./silvercents_complete_demo.sh
```

### Manual Sequence

```bash
# Terminal 1: Server
cargo run -p basis_server

# Terminal 2: Setup
./silvercents_setup.sh

# Terminal 3: Alice issues notes
./silvercents_issuer.sh

# Terminal 4: Bob receives notes
./silvercents_receiver.sh

# Terminal 5: Bob redeems
./silvercents_redeem.sh
```

## Testing

### Unit Tests

```bash
# All tests
cargo test

# Specific crate
cargo test -p basis_store

# With output
cargo test -- --nocapture
```

### Integration Tests

```bash
# Tracker tests
cargo test -p basis_store tracker_

# AVL tree tests
cargo test -p basis_store avl_tree

# Signature tests
cargo test -p basis_offchain schnorr
```

### Stress Testing

```bash
# Issue many notes rapidly
./demo/silvercents_stress_test.sh

# Monitor collateralization
watch -n 1 'curl -s http://localhost:3048/reserve/status/$(cat /tmp/silvercents_demo/state/alice_account.txt | grep PUBLIC_KEY | cut -d= -f2) | jq'
```

## Troubleshooting

### Server Won't Start

```bash
# Check if port is in use
lsof -i :3048

# Check logs
tail -f ~/.basis/tracker.log

# Reset database
rm -rf ~/.basis/db
```

### Notes Not Appearing

```bash
# Check tracker is running
curl http://localhost:3048/status

# Verify note format
jq . /tmp/silvercents_demo/logs/alice_ledger.csv

# Check tracker logs
grep -i "note" ~/.basis/tracker.log
```

### Redemption Failing

```bash
# Verify reserve exists
curl http://localhost:3048/reserve/status/[issuer_pubkey]

# Check note timestamp
jq '.data[] | select(.recipient=="[bob_pubkey]")' [notes_file]

# Verify on-chain
curl http://localhost:9053/blockchain/box/[box_id]
```

## Production Deployment

### Requirements

1. **Physical Infrastructure:**
   - Secure vault for silver coins
   - Insurance coverage
   - Backup location

2. **Regulatory:**
   - Money transmitter license
   - AML/KYC procedures
   - Audit trail

3. **Security:**
   - Multi-sig authority
   - Regular audits
   - Insurance

4. **Monitoring:**
   - 24/7 tracking
   - Alerting system
   - Incident response

### Deployment Checklist

- [ ] Ergo node configured and synced
- [ ] Basis server deployed
- [ ] Reserve contract deployed
- [ ] SSL/TLS certificates installed
- [ ] Rate limiting configured
- [ ] Backup systems tested
- [ ] Disaster recovery plan
- [ ] Legal review completed
- [ ] Audit completed
- [ ] Load testing passed

## Future Enhancements

1. **Multi-Signature Reserves:** Require multiple merchant keys
2. **Liquid Collateral:** Allow ERG and other tokens as backing
3. **Staking Rewards:** Interest on tracked debt
4. **Decentralized Trackers:** Multiple tracker consensus
5. **State Channels:** Direct peer-to-peer transfers
6. **Atomic Swaps:** Direct SilverCents ↔ other assets
7. **Mobile Wallets:** Full-featured wallet apps
8. **Merchant Dashboard:** Real-time analytics

## References

- [Basis Protocol Specification](../specs/spec.md)
- [Basis Server Specification](../specs/server/basis_server_spec.md)
- [Ergo Platform Documentation](https://ergoplatform.org/docs/)
- [secp256k1 Cryptography](https://en.bitcoin.it/wiki/Secp256k1)
- [Schnorr Signatures](https://www.bip340.xyz/)
