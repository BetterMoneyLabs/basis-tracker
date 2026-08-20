# Basis Contract (basis.es) - Rust Implementation Notes

This document provides Rust-specific annotations for the basis.es ErgoScript contract.

## Contract Overview

The Basis contract manages on-chain reserves that back off-chain IOU notes. It enables:
1. **Redemption** (action=0): Creditors redeem IOU notes against reserve collateral
2. **Top-up** (action=1): Reserve owners add more collateral

## Key Architecture for Rust Implementation

### 1. AVL Tree Structure

The contract uses TWO separate AVL trees:

#### Reserve Tree (R5 register)
- **Purpose**: Tracks cumulative redeemed amounts per (owner, receiver) pair
- **Key**: `Blake2b256(owner_pubkey_bytes || receiver_pubkey_bytes)` (32 bytes)
- **Value**: `timestamp (8 bytes LE) || cumulative_redeemed (8 bytes LE)` (16 bytes total)
- **Flags**: InsertOnly (0x01) - can only insert, never update/remove
- **Plasma Parameters**: `(key_length=32, value_length=None)`

#### Tracker Tree (in tracker box R5)
- **Purpose**: Commits to off-chain debt state
- **Key**: `Blake2b256(payer_pubkey || payee_pubkey)` (32 bytes)
- **Value**: `total_debt` as Long (8 bytes LE)
- **Flags**: InsertOnly (0x01)
- **Plasma Parameters**: `(key_length=32, value_length=None)`

### 2. Message Format for Signatures

All signatures (both payer and tracker) use the SAME message format:

```
message = blake2b256(payer_key || payee_key) || total_debt_u64_le || timestamp_i64_le
```

**Total length**: 32 + 8 + 8 = **48 bytes**

**IMPORTANT**: 
- Timestamp is in **milliseconds** since Unix epoch (Java/JavaScript format)
- Use little-endian encoding for numbers (Rust: `to_le_bytes()`)
- The blake2b256 hash is of the raw compressed public keys (33 bytes each for compressed points)

### 3. Schnorr Signature Format

The contract uses Schnorr signatures with strong Fiat-Shamir transformation:

```rust
// Signature structure
struct SchnorrSignature {
    a: CompressedPoint,  // 33 bytes (random point on curve)
    z: Scalar,           // 32 bytes (response, unsigned)
}

// Total: 65 bytes
```

**Signature generation** (for reference, you'll verify not sign):
```rust
fn sign(message: &[u8], secret: &Scalar) -> SchnorrSignature {
    let k = random_scalar();
    let a = G * k;  // Random point
    let e = blake2b256(&[a.compress(), message, pubkey.compress()].concat());
    let e_scalar = bytes_to_scalar_le(e);
    let z = k + e_scalar * secret;
    SchnorrSignature { a, z }
}
```

**Signature verification**:
```rust
fn verify(sig: &SchnorrSignature, pubkey: &CompressedPoint, message: &[u8]) -> bool {
    let e = blake2b256(&[sig.a.as_bytes(), message, pubkey].concat());
    let e_scalar = bytes_to_scalar_le(e);
    let lhs = G * sig.z;
    let rhs = sig.a + pubkey * e_scalar;
    lhs == rhs
}
```

### 4. Context Variables for Redemption

When building redemption transactions, you must provide these context extension variables:

| Index | Type | Description | Required |
|-------|------|-------------|----------|
| #0 | Byte | Action code (0=redeem, 1=topup) + output index | ✅ Always |
| #1 | GroupElement | Receiver's public key | ✅ Always |
| #2 | Coll[Byte] | Reserve owner's signature (65 bytes) | ✅ Always |
| #3 | Long | Total debt amount (nanoERG) | ✅ Always |
| #4 | Long | Payment timestamp (ms) | ✅ Always |
| #5 | Coll[Byte] | AVL proof for reserve tree insert | ✅ Always |
| #6 | Coll[Byte] | Tracker's signature (65 bytes) | ✅ For normal redemption |
| #7 | Coll[Byte] | AVL proof for reserve tree lookup | ⚠️ Optional (2nd+ redemptions) |
| #8 | Coll[Byte] | AVL proof for tracker tree lookup | ✅ Always |

**Encoding for context variables**:
```rust
// Context var #0: action=0, index=0
let var0 = ErgoValue::serialize(0u8)?;  // "0200" in hex

// Context var #1: GroupElement
let var1 = ErgoValue::serialize(receiver_pubkey)?;

// Context var #3, #4: Long (64-bit)
let var3 = ErgoValue::serialize(total_debt_i64)?;
let var4 = ErgoValue::serialize(timestamp_i64)?;

// Context var #2, #5, #6, #7, #8: Coll[Byte]
let var2 = ErgoValue::serialize_coll_bytes(reserve_sig_bytes)?;
```

### 5. Contract Register Layout

```
Reserve Box:
├─ R4: GroupElement (owner public key)
├─ R5: AvlTree (redemption history)
└─ R6: Coll[Byte] (tracker NFT ID, 32 bytes with 0e20 prefix)

Tracker Box (data input):
├─ R4: GroupElement (tracker public key)
├─ R5: AvlTree (debt state commitment)
└─ tokens[0]: NFT (tracker identity)
```

### 6. Emergency Redemption

After **2160 blocks** (~3 days) from tracker creation:
- Tracker signature becomes **optional**
- Same message format used
- Enables redemption if tracker goes offline
- Contract checks: `(HEIGHT - tracker_creation_info.1) > 2160`

### 7. Contract Validation Logic

The contract verifies (in order):
1. ✅ `selfPreserved`: Contract proposition, tokens, R4, R6 unchanged
2. ✅ `trackerIdCorrect`: Tracker NFT ID matches R6
3. ✅ `trackerDebtCorrect`: Debt exists in tracker's AVL tree (via proof #8)
4. ✅ `timestampCorrect`: New timestamp > stored timestamp
5. ✅ `properRedemptionTree`: AVL tree correctly updated (via proof #5)
6. ✅ `properReserveSignature`: Owner's Schnorr signature valid
7. ✅ `properlyRedeemed`: Amount valid + tracker sig valid (or emergency)
8. ✅ `receiverCondition`: Receiver can spend (proveDlog)

### 8. Critical Implementation Notes for Rust

#### AVL Tree Operations
```rust
// You need a PlasmaMap implementation compatible with Ergo's AVL+ trees
// Key library: ergo-avltree or sigma-state Rust port

// Reserve tree operations:
// 1. Create empty tree with InsertOnly flags
// 2. Insert (key, timestamp || redeemed_amount)
// 3. Generate proof for insertion
// 4. Return updated tree digest for output box

// Tracker tree operations:
// 1. Create empty tree with InsertOnly flags
// 2. Insert (key, total_debt)
// 3. Generate proof for lookup
// 4. Use proof in context var #8
```

#### Serialization Gotchas
```rust
// Public keys: Use COMPRESSED format (33 bytes, starts with 02 or 03)
let compressed = point.compress();  // NOT uncompressed

// Numbers: Little-endian byte order
let debt_bytes = 50_000_000u64.to_le_bytes();  // 8 bytes
let timestamp_ms = SystemTime::now()...to_le_bytes();  // 8 bytes

// Signatures: z component must be EXACTLY 32 bytes (unsigned)
let z_bytes = scalar.to_bytes_le();  // NOT BigInt.toByteArray() which adds sign byte!
```

#### Transaction Building
```rust
// Input structure:
struct RedeemInput {
    reserve_box_id: BoxId,
    tracker_box_id: BoxId,
    context_vars: HashMap<u8, ErgoValue>,
}

// Output structure:
struct RedeemOutput {
    reserve_output: Box {
        ergo_tree: basis_contract,
        value: reserve_value - redeemed_amount,
        tokens: [reserve_nft],
        r4: owner_pubkey,
        r5: updated_avl_tree,
        r6: tracker_nft_id,
    },
    receiver_output: Box {
        ergo_tree: p2pk(receiver_pubkey),
        value: redeemed_amount,
    },
    fee_output: Box { ... }
}
```

### 9. Testing Your Implementation

1. **Unit tests**: Verify signature verification matches contract logic
2. **AVL tests**: Test tree insert/lookup proofs match Scala implementation
3. **Integration**: Use note.json from simple demo, build matching transaction
4. **Edge cases**: 
   - Empty bytes for tracker signature (emergency redemption)
   - Multiple sequential redemptions (timestamp ordering)
   - Partial redemptions (amount < total_debt)

### 10. Required Crates

```toml
[dependencies]
# Cryptography
k256 = "0.13"  # For secp256k1 (Ergo uses same curve as Bitcoin)
blake2 = "0.10"  # For Blake2b256

# AVL+ tree (you may need to port from Scala)
# Option 1: Use existing Rust AVL+ implementation
# Option 2: Port from work.lithos.plasma.collections.PlasmaMap

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
hex = "0.4"

# Time
chrono = "0.4"  # For timestamp handling
```

## See Also

- `specs/basis-protocol.md` - Full protocol specification
- `specs/tracker-architecture.md` - Tracker architecture
- `tests/BasisSpec.scala` - Reference test suite
- `demo/simple/` - Working demo with example data
- `reference/message-formats.md` - Detailed message encoding guide
