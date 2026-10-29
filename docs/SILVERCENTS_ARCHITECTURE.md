# SilverCents Architecture

**Technical documentation for silver-backed offchain cash implementation**

---

## Overview

SilverCents is a demonstration of the Basis protocol applied to local circular economies, using silver-backed reserves to issue offchain cash redeemable for physical silver coins.

### Key Concepts

- **Basis Protocol:** Offchain IOU tracking with on-chain reserves
- **DexySilver:** Tokenized silver on Ergo blockchain
- **Constitutional Silver:** Physical dimes and quarters (90% silver)
- **Local Economy:** Farmers markets, food trucks, small businesses

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     SilverCents Ecosystem                    │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
┌───────▼────────┐  ┌──────▼───────┐  ┌────────▼────────┐
│  Vendor CLI    │  │ Customer CLI │  │  Basis Tracker  │
│  (Issuer)      │  │  (Receiver)  │  │  (Server)       │
└───────┬────────┘  └──────┬───────┘  └────────┬────────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            │
                ┌───────────▼────────────┐
                │  On-Chain Reserve      │
                │  - ERG collateral      │
                │  - DexySilver tokens   │
                │  - Redemption contract │
                └────────────────────────┘
```

---

## Components

### 1. Vendor CLI (`silvercents_vendor.sh`)

**Purpose:** Manage vendor operations

**Responsibilities:**
- Initialize vendor accounts
- Create silver-backed reserves
- Issue SilverCents to customers
- Monitor collateralization
- Process redemption requests

**Data Storage:**
- Account: `/tmp/silvercents_<name>.account`
- Reserve: `/tmp/silvercents_reserve_<name>.dat`

### 2. Customer CLI (`silvercents_customer.sh`)

**Purpose:** Manage customer operations

**Responsibilities:**
- Initialize customer accounts
- Track received SilverCents
- Check balances
- Request redemptions
- Transfer to other customers

**Data Storage:**
- Account: `/tmp/silvercents_<name>.account`
- Notes: `/tmp/silvercents_notes_<name>.dat`

### 3. Utilities (`silvercents_utils.sh`)

**Purpose:** Shared functionality

**Functions:**
- API interaction
- Silver conversion calculations
- Collateralization tracking
- Signature generation (mock)
- Formatting and display

### 4. Interactive Demo (`silvercents_demo.sh`)

**Purpose:** Automated demonstration

**Scenario:** Portland Farmers Market with 3 vendors and 2 customers

---

## Data Structures

### Account

```bash
NAME=<vendor_or_customer_name>
PUBKEY=<secp256k1_public_key>
CREATED=<unix_timestamp>
METADATA=<additional_info>
```

### Reserve

```bash
VENDOR_NAME=<name>
PUBKEY=<vendor_public_key>
ERG_COLLATERAL=<nanoerg_amount>
DEXYSILVER_TOKENS=<token_count>
ISSUED_AMOUNT=<total_issued_nanoerg>
CREATED=<unix_timestamp>
```

### Note

```
<vendor_pubkey>|<amount_nanoerg>|<timestamp>|<vendor_name>|<memo>
```

---

## Collateralization Model

### Formula

```
collateral_ratio = (ERG + DexySilver_value) / issued_amount

where:
  ERG = ERG collateral in nanoERG
  DexySilver_value = tokens * value_per_token
  issued_amount = total SilverCents issued
```

### Thresholds

| Ratio | Status | Action |
|-------|--------|--------|
| ≥200% | EXCELLENT | Full issuance allowed |
| 150-200% | GOOD | Normal operations |
| 100-150% | ADEQUATE | Monitor closely |
| 80-100% | WARNING | Reduce issuance |
| 50-80% | CRITICAL | Add collateral |
| <50% | UNDER-COLLATERALIZED | Issuance blocked |

### Minimum Requirement

**50% DexySilver backing** (per SilverCents specification)

---

## Silver Conversion

### Equivalents

```
1 SilverCent = 1 silver dime equivalent
1 silver dime = 0.1 troy oz silver (constitutional)
2.5 dimes = 1 quarter
10 dimes = 1 dollar in silver
```

### Conversion Functions

```bash
# SilverCents to dimes
dimes = silvercents * 1

# Dimes to quarters
quarters = dimes / 2.5

# Mixed breakdown
50 SC = 50 dimes = 20 quarters
25 SC = 25 dimes = 10 quarters
```

---

## Workflows

### Vendor Workflow

```
1. Initialize Account
   └─> Generate keypair
   └─> Save account file

2. Create Reserve
   └─> Specify ERG + DexySilver
   └─> Calculate total collateral
   └─> Save reserve file

3. Issue SilverCents
   └─> Validate customer pubkey
   └─> Check collateralization
   └─> Generate signature
   └─> Create note via API
   └─> Update issued amount

4. Process Redemption
   └─> Verify customer request
   └─> Calculate silver to provide
   └─> Update issued amount
   └─> Provide physical silver
```

### Customer Workflow

```
1. Initialize Account
   └─> Generate keypair
   └─> Save account file

2. Receive SilverCents
   └─> Vendor issues note
   └─> Record in notes file
   └─> Update balance

3. Check Balance
   └─> Read all notes
   └─> Sum amounts
   └─> Display total

4. Redeem for Silver
   └─> Select vendor
   └─> Specify amount
   └─> Request redemption
   └─> Receive physical silver

5. Transfer to Peer
   └─> Specify recipient
   └─> Check balance
   └─> Create transfer note
```

---

## Security Model

### Mock Implementation (Demo)

**⚠️ This demo uses simplified security for demonstration purposes**

- **Keypairs:** Deterministic from names (not cryptographically secure)
- **Signatures:** SHA256 hashes (not real Schnorr signatures)
- **Storage:** Temporary files (not persistent database)

### Production Requirements

For real deployment:

1. **Proper Keypairs**
   - Use secp256k1 curve
   - Secure key storage
   - Hardware wallet support

2. **Real Signatures**
   - Schnorr signatures
   - Proper nonce generation
   - Signature verification

3. **Persistent Storage**
   - Database (PostgreSQL/SQLite)
   - Encrypted backups
   - Access control

4. **On-Chain Integration**
   - Deploy Basis reserve contract
   - Track blockchain events
   - Handle redemptions on-chain

---

## API Integration

### Basis Tracker API

**Endpoints Used:**

```
POST /notes
  Create new note
  Body: {issuer_pubkey, recipient_pubkey, amount, timestamp, signature}

GET /notes?issuer=<pk>&recipient=<pk>
  Get notes for issuer-recipient pair

GET /status
  Get server status
```

### Future Enhancements

- WebSocket for real-time updates
- Proof generation for redemptions
- Multi-tracker support
- Reputation tracking

---

## Deployment Considerations

### Local Testing

```bash
# Start Basis tracker server
cargo run -p basis_server

# Run SilverCents demo
cd demo/silvercents
./silvercents_demo.sh
```

### Production Deployment

1. **Infrastructure**
   - Ergo node (full or light)
   - Basis tracker server
   - Database (PostgreSQL)
   - Web server (nginx)

2. **Configuration**
   - Tracker NFT ID
   - Network (mainnet/testnet)
   - Collateral ratios
   - Fee structure

3. **Monitoring**
   - Collateralization alerts
   - Redemption tracking
   - Reserve balances
   - System health

---

## Limitations

### Current Demo

- ✅ Demonstrates core concepts
- ✅ Shows complete workflow
- ✅ Educational value
- ⚠️ Mock cryptography
- ⚠️ Temporary storage
- ⚠️ No blockchain integration

### Future Work

- [ ] Real cryptographic signatures
- [ ] Persistent database
- [ ] On-chain reserve deployment
- [ ] DexySilver token integration
- [ ] Web/mobile interface
- [ ] Reputation system
- [ ] Multi-vendor marketplace

---

## Performance

### Scalability

- **Vendors:** Unlimited (independent reserves)
- **Customers:** Unlimited (lightweight accounts)
- **Transactions:** Limited by tracker throughput
- **Storage:** Linear growth with notes

### Optimization Opportunities

- Batch note creation
- Proof caching
- Database indexing
- API rate limiting

---

## References

1. **Basis Protocol**
   - `chaincash/docs/basis.md`
   - `basis-tracker/README.md`

2. **SilverCents Specification**
   - `chaincash/docs/silvercents.md`

3. **Ergo Platform**
   - https://ergoplatform.org
   - https://docs.ergoplatform.com

4. **Constitutional Silver**
   - Pre-1965 US dimes and quarters
   - 90% silver content
   - Widely available and recognizable

---

**Status:** Demo Implementation  
**Version:** 1.0.0  
**Team:** Dev Engers (LNMIIT Hackathon 2025)  
**Issue:** #2 - SilverCents Demo Implementation
