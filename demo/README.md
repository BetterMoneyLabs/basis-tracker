# Basis Simple Demo

Minimal working example of the Basis protocol for off-chain debt with on-chain redemption.

## Overview

This demo demonstrates the core Basis protocol flow:
1. **IOU Note Creation** - Alice issues debt to Bob with dual signatures (Alice + Tracker)
2. **Note Output** - Signed note saved to JSON file for later redemption

The demo uses hardcoded test keys to simplify the flow and focus on the protocol mechanics.

## Architecture

```
Alice (Reserve Owner) ──issues IOU──> Bob (Payee)
      │                                   
      │   Signs with Alice's key         
      │   Signs with Tracker's key       
      └──> IOU Note (JSON)              
            - 48-byte message
            - Dual Schnorr signatures
            - Debt amount & timestamp
```

## Quick Start

### Run the Demo

```bash
./demo/run_demo.sh
```

This will:
1. Create an IOU note (Alice → Bob, 0.05 ERG)
2. Sign with both Alice's and Tracker's keys
3. Save to `demo/output/note.json`

### Manual Steps

#### Create IOU Note

```bash
cargo run -p basis_cli -- note create \
  --demo \
  --amount 50000000 \
  --output demo/output/note.json
```

**Output:** `demo/output/note.json`
```json
{
  "payerKey": "03d6bfe100d1600c0d8f769501676fc74c3809500bd131c8a549f88cf616c21f35",
  "payeeKey": "02ba03c59663731bf678d58caf3ca6bba0c2cac2ac39e5e9b5f8e7b5577f0f4739",
  "totalDebt": 50000000,
  "totalDebtERG": 0.05,
  "timestamp": 1775924356220,
  "payerSignature": {"a": "...", "z": "..."},
  "trackerSignature": {"a": "...", "z": "..."},
  "message": "500d5eac14a7ba32d8dd01af4aac7bdc682e31fddd984b85ade471693a6f12f80000000002faf0800000019d7d57247c",
  "messageFormat": "key (32 bytes) || totalDebt (8 bytes) || timestamp (8 bytes)",
  "noteKey": "..."
}
```

## Note Structure

### Message Format (48 bytes)

The signing message is constructed as:

```
message = key || totalDebt || timestamp
```

Where:
- **key** (32 bytes): `blake2b256(alice_pubkey || bob_pubkey)`
- **totalDebt** (8 bytes): Cumulative debt in nanoERG (big-endian)
- **timestamp** (8 bytes): Milliseconds since Unix epoch (big-endian)

### Signature Format (65 bytes each)

Both Alice and the Tracker sign the same 48-byte message using Schnorr signatures:

```
signature = a (33 bytes) || z (32 bytes)
```

Where:
- **a**: Random point R = k×G (compressed secp256k1 public key format)
- **z**: Response scalar z = k + e×s (mod n)
- **e**: Challenge = blake2b256(a || message || pubkey)

### Note Fields

| Field | Type | Description |
|-------|------|-------------|
| `payerKey` | String (66 hex) | Alice's compressed public key (33 bytes) |
| `payeeKey` | String (66 hex) | Bob's compressed public key (33 bytes) |
| `totalDebt` | u64 | Cumulative debt amount in nanoERG |
| `totalDebtERG` | f64 | Debt in ERG (for display) |
| `timestamp` | u64 | Creation time in milliseconds |
| `payerSignature` | Object | Alice's Schnorr signature {a, z} |
| `trackerSignature` | Object | Tracker's Schnorr signature {a, z} |
| `message` | String (96 hex) | The 48-byte signing message |
| `messageFormat` | String | Description of message structure |
| `noteKey` | String (64 hex) | Unique note identifier |

## Demo Keys

The demo uses hardcoded test keys for simplicity:

| Participant | Role | Description |
|-------------|------|-------------|
| **Alice** | Payer/Issuer | Creates IOU notes, owns reserve collateral |
| **Bob** | Payee/Recipient | Receives IOU notes, can redeem them |
| **Tracker** | Off-chain Witness | Signs notes to enable normal redemption |

**⚠️ Security Warning:** These are **TEST KEYS ONLY** - the private keys are publicly known and provide no security. Never use them in production!

### Key Generation

Demo keys are defined in `crates/basis_cli/src/demo_keys.rs`:

```rust
pub fn alice() -> DemoParticipant {
    let secret_hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    // ... creates KeyPair from secret
}

pub fn bob() -> DemoParticipant { ... }
pub fn tracker() -> DemoParticipant { ... }
```

## Protocol Flow

### Normal Flow (with Tracker online)

```
1. Alice wants to create debt to Bob
   ↓
2. Alice constructs 48-byte message:
   message = blake2b256(alice_pk || bob_pk) || totalDebt || timestamp
   ↓
3. Alice signs message with her secret key
   ↓
4. Tracker signs same message with tracker secret key
   ↓
5. Both signatures combined into IOU note JSON
   ↓
6. Bob receives IOU note (can redeem against Alice's reserve)
```

### Redemption Flow (future work)

```
1. Bob presents IOU note for redemption
   ↓
2. Verify both Alice's and Tracker's signatures
   ↓
3. Generate AVL proofs:
   - Tracker lookup proof (debt exists in tracker tree)
   - Reserve insert proof (update redeemed amount)
   ↓
4. Build redemption transaction with context variables:
   - #0: action byte (0x00 = redeem)
   - #1: receiver public key (Bob)
   - #2: reserve owner signature (Alice)
   - #3: total debt amount
   - #4: timestamp
   - #5: reserve insert proof
   - #6: tracker signature
   - #8: tracker lookup proof
   ↓
5. Sign transaction with Ergo node
   ↓
6. Broadcast to blockchain
   ↓
7. ErgoScript contract verifies all conditions
   ↓
8. Bob receives ERG from reserve
```

## Files

| File | Purpose |
|------|---------|
| `run_demo.sh` | Demo orchestration script |
| `config.toml` | Demo configuration |
| `output/note.json` | Generated IOU note |
| `README.md` | This file |

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

## Implementation Details

### Cryptography

- **Curve:** secp256k1 (same as Bitcoin/Ergo)
- **Signature Algorithm:** Schnorr signatures
- **Hash Function:** Blake2b-256
- **Key Format:** Compressed (33 bytes, starts with 0x02 or 0x03)

### Signature Generation

```rust
// 1. Generate random nonce k
let k = random_scalar();

// 2. Compute random point a = k × G
let a = secp256k1::G * k;

// 3. Compute challenge e = H(a || message || pubkey)
let e = blake2b256(&[a.serialize(), message, pubkey.serialize()].concat());

// 4. Compute response z = k + e × s (mod n)
let z = (k + e * secret_key) % secp256k1::ORDER;

// 5. Signature = (a, z)
```

### Signature Verification

```rust
// 1. Recompute challenge e = H(a || message || pubkey)
let e = blake2b256(&[a.serialize(), message, pubkey.serialize()].concat());

// 2. Verify: G^z == a × pubkey^e
let lhs = secp256k1::G * z;
let rhs = a + pubkey * e;

lhs == rhs  // true if signature is valid
```

## Testing

### Verify Note Signatures

```bash
# The note includes both signatures
# You can verify them manually:

# 1. Extract message from note.json
MESSAGE=$(jq -r '.message' demo/output/note.json)

# 2. Extract signatures
ALICE_SIG=$(jq -r '.payerSignature' demo/output/note.json)
TRACKER_SIG=$(jq -r '.trackerSignature' demo/output/note.json)

# 3. Verify using basis-cli (future command)
# basis-cli note verify --json demo/output/note.json
```

### Cross-Compatibility with Scala Demo

The Rust demo produces notes in the same format as the Scala demo:

```bash
# Create note with Rust
cargo run -p basis_cli -- note create --demo --amount 50000000 > note_rust.json

# Create note with Scala (if available)
cd scala && sbt "runMain chaincash.contracts.BasisNoteCreator" > note_scala.json

# Both should have the same structure:
jq 'keys' note_rust.json
jq 'keys' note_scala.json
# Should output the same field names
```

## Troubleshooting

### "No current account selected"

This error occurs when not using `--demo` flag. The demo mode bypasses the account manager requirement.

**Solution:** Use `--demo` flag or set up CLI accounts first.

### "Failed to create note"

Check that:
1. Amount is specified in nanoERG (1 ERG = 1,000,000,000 nanoERG)
2. Output directory exists (`mkdir -p demo/output`)
3. Rust toolchain is installed

### Signature Verification Fails

If signatures don't verify:
1. Check message is exactly 48 bytes (96 hex chars)
2. Verify public keys are 33 bytes compressed format (66 hex chars)
3. Ensure signatures are 65 bytes (130 hex chars)
4. Check Blake2b-256 hash function is used correctly

## Future Enhancements

- [ ] Full redemption flow with Ergo node integration
- [ ] AVL proof generation for tracker and reserve trees
- [ ] Transaction building and signing
- [ ] Blockchain broadcast
- [ ] Reserve deployment automation
- [ ] Fee box discovery and management
- [ ] Cross-compatibility tests with Scala demo

## References

- [Protocol Specification](../specs/spec.md)
- [Schnorr Signature Spec](../specs/SCHNORR_SIGNATURE_SPEC.md)
- [Scala Demo](../scala/demo/)
- [Ergo Documentation](https://docs.ergoplatform.com/)
- [secp256k1 Specification](https://www.secg.org/sec2-v2.pdf)

## License

CC0-1.0 (Same as rest of Basis Tracker project)
