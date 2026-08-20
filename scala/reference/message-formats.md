# Basis Protocol - Message Formats & Data Structures Reference

**For:** Rust BasisTracker Implementation  
**Version:** 1.0 (April 9, 2026)

---

## 1. Cryptographic Primitives

### Curve: secp256k1
- Same as Bitcoin/Ergo
- Generator point: `G` (standard secp256k1 generator)
- All public keys in **compressed format** (33 bytes)

### Hash: Blake2b-256
- Digest size: 32 bytes
- Used for: message hashing, AVL tree keys, signature challenges

---

## 2. Key Formats

### Public Key (Compressed)
```rust
// 33 bytes total
// Byte 0: 0x02 (even y) or 0x03 (odd y)
// Bytes 1-32: x-coordinate (big-endian)
struct CompressedPoint {
    prefix: u8,    // 0x02 or 0x03
    x: [u8; 32],   // big-endian
}
```

**Example:**
```
0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
│                                                          │
└─ Compressed (odd y)                                      └─ x-coordinate
```

### Secret Key
```rust
// 32 bytes (255-bit scalar)
// Always positive, no sign byte
type SecretKey = Scalar;  // From k256 crate
```

**Example (hex):**
```
c693d626538e9dd926519c13f3855412d60aaaa9c8818e7725415a45e92f3108
```

---

## 3. Signature Format (Schnorr)

### Structure
```rust
struct SchnorrSignature {
    a: CompressedPoint,  // 33 bytes (random point R = k*G)
    z: [u8; 32],         // 32 bytes (response scalar, unsigned big-endian)
}
// Total: 65 bytes
```

### Signing Algorithm
```rust
fn sign(message: &[u8; 48], secret: &SecretKey) -> SchnorrSignature {
    // 1. Generate random nonce
    let k = random_scalar();
    
    // 2. Compute commitment point
    let a = G * k;
    
    // 3. Compute challenge (strong Fiat-Shamir)
    let challenge_input = [
        a.compress().to_bytes(),    // 33 bytes
        message,                     // 48 bytes
        public_key.compress(),       // 33 bytes
    ].concat();
    let e_bytes = blake2b_256(&challenge_input);
    let e = scalar_from_bytes_le(&e_bytes);
    
    // 4. Compute response
    let z = k + e * secret;
    
    SchnorrSignature {
        a: a.compress(),
        z: z.to_bytes_be(),  // EXACTLY 32 bytes, unsigned!
    }
}
```

### Verification Algorithm
```rust
fn verify(sig: &SchnorrSignature, pubkey: &CompressedPoint, message: &[u8]) -> bool {
    // 1. Recompute challenge
    let challenge_input = [
        sig.a.as_bytes(),          // 33 bytes
        message,                    // 48 bytes
        pubkey.as_bytes(),          // 33 bytes
    ].concat();
    let e_bytes = blake2b_256(&challenge_input);
    let e = scalar_from_bytes_le(&e_bytes);
    
    // 2. Verify: G^z = a * pubkey^e
    let lhs = G * sig.z;
    let rhs = sig.a + pubkey * e;
    
    lhs == rhs
}
```

**⚠️ CRITICAL GOTCHAS:**
1. **z must be exactly 32 bytes** - use BouncyCastle-style unsigned encoding
2. **NO sign byte** - don't use BigInt.toByteArray() which adds sign byte for negative numbers
3. **Challenge is little-endian** when converting bytes to scalar
4. **Strong Fiat-Shamir** - includes public key in challenge (not just message)

---

## 4. Message Formats

### 4.1 IOU Note Message (48 bytes)

```rust
// message = blake2b256(payer_key || payee_key) || total_debt || timestamp
struct IouMessage {
    key: [u8; 32],        // Blake2b256(payer_key_bytes || payee_key_bytes)
    total_debt: [u8; 8],  // u64 little-endian (nanoERG)
    timestamp: [u8; 8],   // i64 little-endian (ms since Unix epoch)
}
// Total: 48 bytes
```

**Construction:**
```rust
fn create_iou_message(
    payer_key: &CompressedPoint,
    payee_key: &CompressedPoint,
    total_debt: u64,
    timestamp: i64,
) -> [u8; 48] {
    // 1. Hash the concatenated public keys
    let mut key_input = [0u8; 66];  // 33 + 33
    key_input[..33].copy_from_slice(&payer_key.to_bytes());
    key_input[33..].copy_from_slice(&payee_key.to_bytes());
    let key = blake2b_256(&key_input);  // 32 bytes
    
    // 2. Assemble message
    let mut message = [0u8; 48];
    message[..32].copy_from_slice(&key);
    message[32..40].copy_from_slice(&total_debt.to_le_bytes());
    message[40..48].copy_from_slice(&timestamp.to_le_bytes());
    
    message
}
```

**Example:**
```
6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4 0000000002faf080 00000194f8c88000
││                                                               ││                 ││
└─ Blake2b256(Alice||Bob)                                        └─ 50,000,000      └─ 1743379200000 ms
                                                                 (0.05 ERG)         (Mar 29, 2025)
```

### 4.2 Signature Message (Same for Both Payer and Tracker)

Both payer (Alice) and tracker sign the **exact same message**.

```rust
// Payer signs:
payer_signature = sign(iou_message, payer_secret)

// Tracker signs:
tracker_signature = sign(iou_message, tracker_secret)
```

---

## 5. AVL Tree Structures

### 5.1 Plasma Parameters

```rust
struct PlasmaParameters {
    key_length: u32,         // 32 bytes
    value_length: Option<u32>, // None (dynamic)
}

// ChainCash uses:
const CHAINCASH_PLASMA_PARAMS: PlasmaParameters = PlasmaParameters {
    key_length: 32,
    value_length: None,  // Variable length values
};
```

### 5.2 Tree Flags

```rust
enum AvlTreeFlags {
    InsertOnly = 0x01,      // Can only insert (not update/remove)
    InsertUpdate = 0x03,    // Can insert and update
}

// Reserve tree: InsertOnly
// Tracker tree: InsertOnly
```

### 5.3 Reserve Tree

**Purpose:** Track cumulative redeemed amounts per (owner, receiver) pair

```rust
struct ReserveTreeEntry {
    key: [u8; 32],         // Blake2b256(owner_key || receiver_key)
    value: [u8; 16],       // timestamp (8 bytes LE) || redeemed_amount (8 bytes LE)
}
```

**Key Construction:**
```rust
fn reserve_tree_key(owner_key: &CompressedPoint, receiver_key: &CompressedPoint) -> [u8; 32] {
    let mut input = [0u8; 66];
    input[..33].copy_from_slice(&owner_key.to_bytes());
    input[33..].copy_from_slice(&receiver_key.to_bytes());
    blake2b_256(&input)
}
```

**Value Construction:**
```rust
fn reserve_tree_value(timestamp: i64, redeemed_amount: u64) -> [u8; 16] {
    let mut value = [0u8; 16];
    value[..8].copy_from_slice(&timestamp.to_le_bytes());
    value[8..16].copy_from_slice(&redeemed_amount.to_le_bytes());
    value
}
```

**Operations:**
```rust
// On first redemption:
// - Tree is empty
// - Insert (key, timestamp || redeemed_amount)
// - Generate insert proof

// On subsequent redemptions:
// - Lookup (key) -> (old_timestamp, old_redeemed)
// - Verify new_timestamp > old_timestamp
// - Update: (key, new_timestamp || old_redeemed + new_amount)
// - Generate update proof
```

### 5.4 Tracker Tree

**Purpose:** Commit to off-chain debt state

```rust
struct TrackerTreeEntry {
    key: [u8; 32],         // Blake2b256(payer_key || payee_key)
    value: [u8; 8],        // total_debt (u64 LE, nanoERG)
}
```

**Value Construction:**
```rust
fn tracker_tree_value(total_debt: u64) -> [u8; 8] {
    total_debt.to_le_bytes()
}
```

**Operations:**
```rust
// When creating IOU note:
// - Insert or update (key, total_debt)
// - Generate proof for redemption

// Tracker maintains cumulative debt:
// debt(A->B) = 50M  (first payment)
// debt(A->B) = 80M  (second payment, cumulative)
// debt(A->B) = 120M (third payment, cumulative)
```

---

## 6. Box Register Layout

### 6.1 Reserve Box

```rust
struct ReserveBox {
    box_id: [u8; 32],
    ergo_tree: Vec<u8>,        // Compiled basis.es contract
    value: u64,                 // ERG in nanoERG
    tokens: [(TokenId, u64)],   // [(reserve_nft_id, 1)]
    registers: ReserveRegisters,
}

struct ReserveRegisters {
    r4: CompressedPoint,        // Owner public key
    r5: AvlTreeDigest,          // Reserve tree digest
    r6: [u8; 32],               // Tracker NFT ID (without prefix)
}
```

**R6 encoding in transaction JSON:**
```json
"R6": "0e20<tracker_nft_id_hex>"
// 0e = Coll[Byte] type tag
// 20 = length (32 bytes)
// <32 bytes of NFT ID>
```

### 6.2 Tracker Box (Data Input)

```rust
struct TrackerBox {
    box_id: [u8; 32],
    ergo_tree: Vec<u8>,
    value: u64,
    tokens: [(TokenId, u64)],   // [(tracker_nft_id, 1)]
    registers: TrackerRegisters,
    creation_info: (u32, u32),  // (height, tx_index)
}

struct TrackerRegisters {
    r4: CompressedPoint,        // Tracker public key
    r5: AvlTreeDigest,          // Tracker tree digest
}
```

---

## 7. Context Variable Encoding

### For Ergo Transaction Extension

```rust
struct ContextVariables {
    var0: Vec<u8>,  // Action + output index
    var1: Vec<u8>,  // Receiver pubkey (GroupElement)
    var2: Vec<u8>,  // Reserve owner signature (Coll[Byte])
    var3: Vec<u8>,  // Total debt (Long)
    var4: Vec<u8>,  // Timestamp (Long)
    var5: Vec<u8>,  // Reserve insert proof (Coll[Byte])
    var6: Vec<u8>,  // Tracker signature (Coll[Byte])
    var7: Option<Vec<u8>>,  // Reserve lookup proof (Coll[Byte], optional)
    var8: Vec<u8>,  // Tracker lookup proof (Coll[Byte])
}
```

**Encoding functions:**
```rust
fn encode_action_index(action: u8, index: u8) -> Vec<u8> {
    // ErgoValue serialization for Byte
    let value = (action * 10 + index) as u8;
    // Type 02 = Byte, followed by value
    vec![0x02, value]
}

fn encode_group_element(pk: &CompressedPoint) -> Vec<u8> {
    // Type 07 = GroupElement
    let mut result = vec![0x07];
    result.extend(pk.to_bytes());
    result
}

fn encode_coll_bytes(bytes: &[u8]) -> Vec<u8> {
    // Type 0e = Coll[Byte]
    let mut result = vec![0x0e];
    // Length as variable-length integer
    result.extend(encode_vlq_length(bytes.len()));
    result.extend(bytes);
    result
}

fn encode_long(value: i64) -> Vec<u8> {
    // Type 05 = Long
    let mut result = vec![0x05];
    // ZigZag encoding for signed integers
    result.extend(encode_zigzag(value));
    result
}
```

**Example context vars JSON:**
```json
{
  "0": "0200",
  "1": "0703af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
  "2": "0e41<65 bytes>",
  "3": "0580c2d72f",
  "4": "058080e4b3d6f017",
  "5": "0e46<70 bytes>",
  "6": "0e41<65 bytes>",
  "8": "0e71<113 bytes>"
}
```

---

## 8. Emergency Redemption

### Conditions
```rust
const EMERGENCY_PERIOD_BLOCKS: u32 = 2160;  // ~3 days

fn is_emergency_period(tracker_creation_height: u32, current_height: u32) -> bool {
    current_height - tracker_creation_height > EMERGENCY_PERIOD_BLOCKS
}
```

### Tracker Signature Handling
```rust
fn validate_tracker_signature(
    sig_bytes: &[u8],
    message: &[u8; 48],
    tracker_pubkey: &CompressedPoint,
    is_emergency: bool,
) -> Result<(), Error> {
    if sig_bytes.is_empty() {
        // No signature provided
        if !is_emergency {
            return Err(Error::TrackerSignatureRequired);
        }
        // Emergency period - signature not required
        Ok(())
    } else {
        // Signature provided - must be valid
        let sig = SchnorrSignature::from_bytes(sig_bytes)?;
        if verify(&sig, tracker_pubkey, message) {
            Ok(())
        } else {
            Err(Error::InvalidTrackerSignature)
        }
    }
}
```

---

## 9. Complete IOU Note JSON Format

```json
{
  "payerKey": "<33 bytes hex - compressed>",
  "payeeKey": "<33 bytes hex - compressed>",
  "totalDebt": 50000000,
  "totalDebtERG": 0.05,
  "timestamp": 1743379200000,
  "payerSignature": {
    "a": "<33 bytes hex - compressed point>",
    "z": "<32 bytes hex - unsigned scalar>"
  },
  "trackerSignature": {
    "a": "<33 bytes hex - compressed point>",
    "z": "<32 bytes hex - unsigned scalar>"
  },
  "message": "<48 bytes hex - iou message>",
  "messageFormat": "key (32 bytes) || totalDebt (8 bytes) || timestamp (8 bytes)",
  "noteKey": "<32 bytes hex - blake2b256 of payerKey||payeeKey>"
}
```

---

## 10. Test Vectors

### 10.1 Demo Keys

```
Alice (Payer/Reserve Owner):
  Address:  9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ
  Public:   0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
  Secret:   c693d626538e9dd926519c13f3855412d60aaaa9c8818e7725415a45e92f3108

Bob (Payee/Receiver):
  Address:  9fJj8vHmB8P7yN5xQ3kR2tM4wL6sG9cV1bX3hD5fA7eK2jN4mP
  Public:   03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea
  Secret:   (in wallet, used for node signing)

Tracker:
  Address:  9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe
  Public:   03<see ParticipantKeys>
  Secret:   (in ParticipantSecretsReader)
```

### 10.2 Example IOU Note

```json
{
  "payerKey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
  "payeeKey": "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
  "totalDebt": 50000000,
  "timestamp": 1743379200000,
  "message": "6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a40000000002faf08000000194f8c88000"
}
```

**Message breakdown:**
- Key: `6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4`
- Debt: `0000000002faf080` = 50,000,000 nanoERG (0.05 ERG)
- Timestamp: `00000194f8c88000` = 1,743,379,200,000 ms (Sat Mar 29 2025)

### 10.3 Signature Verification Test

```rust
#[test]
fn test_verify_demo_signatures() {
    let message = hex::decode("6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a40000000002faf08000000194f8c88000").unwrap();
    
    // Alice's signature
    let alice_pub = hex::decode("0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83").unwrap();
    let alice_sig_a = hex::decode("035354d462a94ba9193ebedf9ca9802bf9781492244c02a8126b21d3111449e497").unwrap();
    let alice_sig_z = hex::decode("5bf46efbd651f171d490ad85c70db8aad9dd30cf267d4fdb6313113a891ebbef").unwrap();
    
    // Verify Alice's signature
    assert!(verify_schnorr(&alice_sig_a, &alice_sig_z, &alice_pub, &message));
    
    // Tracker signature
    let tracker_pub = /* from ParticipantKeys */;
    let tracker_sig_a = hex::decode("036acf767d2efc64d5eafb21c04125794d154a075f4826e33bed532bb180090b79").unwrap();
    let tracker_sig_z = hex::decode("4258e6ff09fc6c5497afc1962148125b8c643a38172d01240fca05d346c56a11").unwrap();
    
    // Verify tracker signature
    assert!(verify_schnorr(&tracker_sig_a, &tracker_sig_z, &tracker_pub, &message));
}
```

---

## 11. NOSTR Integration (For Tracker Alerts)

The tracker publishes alerts via NOSTR protocol:

### Alert Types

```rust
enum TrackerAlert {
    DebtCreated {
        payer: String,
        payee: String,
        amount: u64,
        timestamp: i64,
        total_debt: u64,
    },
    CollateralizationWarning {
        issuer: String,
        collateralization_pct: f64,  // e.g., 80.0 for 80%
        total_debt: u64,
        reserve_value: u64,
    },
    StateCommitted {
        tree_digest: String,
        block_height: u32,
        entry_count: u64,
    },
    EmergencyRedemption {
        payer: String,
        payee: String,
        amount: u64,
    },
}
```

### NOSTR Event Format

```rust
// Event kind: 1 (short text note) or custom
struct NostrAlert {
    pubkey: String,           // Tracker's public key
    created_at: u64,          // Unix timestamp (seconds)
    kind: u64,                // Event type
    tags: Vec<Vec<String>>,   // Metadata tags
    content: String,          // JSON-encoded alert
    id: String,               // Event hash
    sig: String,              // Schnorr signature (NOSTR format)
}
```

---

## 12. Required Rust Types Summary

```rust
// Cryptography
type CompressedPoint = [u8; 33];
type SecretKey = k256::SecretKey;
type PublicKey = k256::PublicKey;
type Scalar = k256::Scalar;

// Hash
type Blake2b256Hash = [u8; 32];

// Signatures
struct SchnorrSignature {
    a: CompressedPoint,
    z: [u8; 32],
}

// Messages
struct IouMessage {
    key: [u8; 32],
    total_debt: u64,
    timestamp: i64,
}

// AVL Tree
struct AvlTreeDigest {
    digest: [u8; 32],
    height: u8,
}

// Boxes
struct ReserveBox {
    box_id: [u8; 32],
    value: u64,
    owner_key: CompressedPoint,
    tracker_nft: [u8; 32],
    tree_digest: AvlTreeDigest,
}

struct TrackerBox {
    box_id: [u8; 32],
    value: u64,
    tracker_key: CompressedPoint,
    tracker_nft: [u8; 32],
    tree_digest: AvlTreeDigest,
    creation_height: u32,
}
```

---

## 13. Common Pitfalls

1. ❌ **Wrong z encoding**: Using `BigInt.toByteArray()` adds sign byte
   ✅ Use unsigned 32-byte encoding

2. ❌ **Wrong message format**: Missing timestamp or wrong order
   ✅ Always: `key || total_debt || timestamp` (48 bytes)

3. ❌ **Uncompressed public keys**: Contract expects compressed
   ✅ Always compress to 33 bytes

4. ❌ **Wrong endianness**: Numbers must be little-endian
   ✅ Use `to_le_bytes()` not `to_be_bytes()`

5. ❌ **AVL tree parameters mismatch**: Must match contract exactly
   ✅ Use `(key_length=32, value_length=None)` with `InsertOnly` flags

6. ❌ **Forgetting context var #7**: Needed for 2nd+ redemptions
   ✅ Include when `redeemedDebt > 0`

7. ❌ **Timestamp in seconds instead of milliseconds**: Contract expects ms
   ✅ Use `SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()`

---

**Last Updated:** April 9, 2026  
**See Also:** `../specs/ergo/basis_contract_rust_notes.md`, `../specs/basis.md`
