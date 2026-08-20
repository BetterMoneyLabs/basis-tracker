# Basis Tracker - Rust Implementation Guide

Complete reference for implementing the BasisTracker in Rust.

---

## Overview

Basis is a protocol for **off-chain debt tracking with on-chain redemption** on the Ergo blockchain. It enables:

- **Credit creation** without upfront collateralization
- **Trust-minimized redemption** via on-chain reserves
- **Emergency exits** if tracker goes offline
- **Micropayments** suitable for mesh networks

The **BasisTracker** is the off-chain component that:
1. Witnesses IOU notes by signing them
2. Maintains AVL tree of all debt relationships
3. Periodically commits state to blockchain
4. Publishes alerts via NOSTR protocol

---

## Directory Structure

```
basis-tracker-rust/
├── README.md                           ← You are here
├── contract/
│   └── basis.es                        ← On-chain reserve contract (ErgoScript, repo root)
├── specs/ergo/
│   └── basis_contract_rust_notes.md    ← Rust-specific implementation notes
├── specs/
│   ├── basis.md                        ← Protocol specification
│   └── tracker.md                      ← Tracker architecture
├── tests/
│   └── BasisSpec.scala                 ← Reference test suite (Scala)
├── demo/
│   ├── note.json                       ← Example IOU note
│   ├── sign_request.json               ← Example redemption transaction
│   ├── tracker_box_setup.json          ← Tracker box configuration
│   ├── SPECIFICATION.md                ← Detailed technical spec
│   ├── README.md                       ← Demo instructions
│   ├── debug_signing.sh                ← Debug script
│   ├── ANALYSIS.md                     ← Contract compatibility analysis
│   ├── FIXES.md                        ← Required fixes
│   └── src/                            ← Scala demo utilities
│       ├── BasisDeployer.scala
│       ├── BasisNoteCreator.scala
│       ├── BasisNoteRedeemer.scala
│       └── TrackerBoxSetup.scala
├── scala-utils/                        ← Reference Scala utilities
│   ├── Constants.scala                 ← Contract compilation constants
│   ├── AddressUtils.scala              ← Key/address derivation
│   ├── ParticipantSecretsReader.scala  ← Demo participant keys
│   └── SigUtils.scala                  ← Signature utilities
└── reference/
    └── message-formats.md              ← Complete message/data format guide
```

---

## Quick Start for Rust Implementation

### Phase 1: Cryptography Foundation

**Goal:** Implement signature verification and message construction

**Key Files:**
- `reference/message-formats.md` - Complete format specifications
- `../specs/ergo/basis_contract_rust_notes.md` - Rust-specific notes
- `demo/note.json` - Real test vectors

**Steps:**

1. **Set up dependencies:**
```toml
[dependencies]
k256 = { version = "0.13", features = ["schnorr"] }
blake2 = "0.10"
hex = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
```

2. **Implement Schnorr verification:**
```rust
use k256::{PublicKey, SecretKey, Scalar};
use k256::schnorr::{SigningKey, VerifyingKey, Signature};
use blake2::{Blake2b256, Digest};

fn verify_schnorr(
    pubkey_bytes: &[u8; 33],
    sig_a: &[u8; 33],
    sig_z: &[u8; 32],
    message: &[u8],
) -> bool {
    // See reference/message-formats.md for full implementation
    // Challenge: blake2b256(sig_a || message || pubkey)
    // Verify: G^z = a * pubkey^e
}
```

3. **Test with demo vectors:**
```rust
#[test]
fn test_demo_note_signatures() {
    let note: IouNote = serde_json::from_str(
        &std::fs::read_to_string("demo/note.json").unwrap()
    ).unwrap();
    
    let message = hex::decode(&note.message).unwrap();
    
    assert!(verify_schnorr(&note.payer_key, &note.payer_sig.a, &note.payer_sig.z, &message));
    assert!(verify_schnorr(&note.tracker_key, &note.tracker_sig.a, &note.tracker_sig.z, &message));
}
```

**Validation:** Signatures verify correctly ✅

---

### Phase 2: AVL+ Tree Implementation

**Goal:** Implement AVL tree operations for debt tracking

**Key Files:**
- `specs/tracker.md` - Tracker tree architecture
- `reference/message-formats.md` - Tree structure details
- `tests/BasisSpec.scala` - Reference tests (see AVL tests)

**Operations Needed:**

1. **Tree Creation:**
```rust
struct PlasmaMap {
    tree: AvlTree,
    flags: AvlTreeFlags,  // InsertOnly = 0x01
    key_length: u32,      // 32
    value_length: Option<u32>,  // None
}

impl PlasmaMap {
    fn new() -> Self {
        // Create empty tree with correct parameters
    }
}
```

2. **Insert:**
```rust
fn insert(&mut self, key: [u8; 32], value: Vec<u8>) -> InsertProof {
    // Insert key-value pair
    // Generate Merkle proof
    // Update tree digest
}
```

3. **Lookup:**
```rust
fn lookup(&self, key: [u8; 32]) -> Option<(Vec<u8>, LookupProof)> {
    // Lookup key in tree
    // Generate proof of existence
}
```

4. **Proof Serialization:**
```rust
fn serialize_proof(proof: &AvlProof) -> Vec<u8> {
    // Must match Ergo's AVL+ proof format
    // Used in context variables #5, #7, #8
}
```

**Challenge:** AVL+ trees are complex. Options:
- **Port** from Scala (`work.lithos.plasma.collections.PlasmaMap`)
- **Use** existing Rust implementation if available
- **Simplify** with Merkle tree for initial version

**Validation:** Proof generation matches Scala implementation ✅

---

### Phase 3: Core Tracker Logic

**Goal:** Implement debt tracking and note witnessing

**Key Files:**
- `specs/tracker.md` - Full tracker architecture
- `specs/basis.md` - Protocol specification
- `demo/src/BasisNoteCreator.scala` - Note creation reference

**Data Structures:**
```rust
/// Represents a debt relationship: A owes debt to B
struct DebtEntry {
    payer_key: PublicKey,
    payee_key: PublicKey,
    total_debt: u64,        // Cumulative nanoERG
    timestamp: i64,         // Milliseconds since epoch
}

/// Tracker's internal state
struct TrackerState {
    debts: HashMap<[u8; 32], DebtEntry>,  // key -> debt
    avl_tree: PlasmaMap,                   // On-chain commitment
    tracker_key: PublicKey,
    tracker_nft: [u8; 32],
}

/// IOU note with both signatures
struct WitnessedNote {
    payer_key: [u8; 33],
    payee_key: [u8; 33],
    total_debt: u64,
    timestamp: i64,
    payer_sig: SchnorrSignature,
    tracker_sig: SchnorrSignature,
}
```

**Core Operations:**

1. **Create IOU Note:**
```rust
fn create_iou_note(
    &mut self,
    payer_key: PublicKey,
    payee_key: PublicKey,
    amount: u64,
) -> Result<WitnessedNote, Error> {
    // 1. Calculate new cumulative debt
    let debt_key = debt_hash(payer_key, payee_key);
    let current_debt = self.debts.get(&debt_key).map(|d| d.total_debt).unwrap_or(0);
    let new_total = current_debt + amount;
    
    // 2. Update internal state
    let timestamp = current_timestamp_ms();
    self.debts.insert(debt_key, DebtEntry { ... });
    
    // 3. Update AVL tree
    self.avl_tree.insert(debt_key, new_total.to_le_bytes());
    
    // 4. Return note (payer must sign separately)
    Ok(WitnessedNote {
        payer_key: payer_key.compress().to_bytes(),
        payee_key: payee_key.compress().to_bytes(),
        total_debt: new_total,
        timestamp,
        payer_sig: ...,  // Signed by payer
        tracker_sig: self.sign_debt_note(debt_key, new_total, timestamp)?,
    })
}
```

2. **Sign Debt Note:**
```rust
fn sign_debt_note(
    &self,
    key: [u8; 32],
    total_debt: u64,
    timestamp: i64,
) -> Result<SchnorrSignature, Error> {
    // Construct message
    let message = build_iou_message(key, total_debt, timestamp);
    
    // Sign with tracker's secret key
    let sig = schnorr_sign(&message, &self.tracker_secret);
    
    Ok(sig)
}
```

3. **Transfer Debt (Novation):**
```rust
fn transfer_debt(
    &mut self,
    from_payee: PublicKey,
    to_payee: PublicKey,
    payer: PublicKey,
    amount: u64,
) -> Result<(WitnessedNote, WitnessedNote), Error> {
    // Split debt: A->B (10) becomes A->B (5) + A->C (5)
    // See specs/basis.md for debt transfer protocol
}
```

**Validation:** Can create and sign valid IOU notes ✅

---

### Phase 4: Blockchain Integration

**Goal:** Commit state to Ergo blockchain and support redemption

**Key Files:**
- `../contract/basis.es` - On-chain contract
- `demo/src/BasisNoteRedeemer.scala` - Redemption reference
- `demo/sign_request.json` - Transaction example

**On-Chain State Commitment:**
```rust
/// Create tracker box with current state
fn create_tracker_box(
    &self,
    tree_digest: [u8; 32],
) -> ErgoTransaction {
    // R4: tracker public key
    // R5: AVL tree digest
    // tokens: [tracker_nft]
}
```

**Redemption Support:**
```rust
/// Generate redemption transaction
fn build_redemption_tx(
    &self,
    note: &WitnessedNote,
    reserve_box_id: [u8; 32],
    fee_box_ids: Vec<[u8; 32]>,
) -> Result<UnsignedTransaction, Error> {
    // 1. Verify note signatures
    // 2. Generate AVL proofs
    // 3. Build transaction with context variables
    // 4. Return unsigned tx for signing
}
```

**Emergency Monitoring:**
```rust
/// Check if tracker has gone offline
fn monitor_tracker_health(&self) -> TrackerStatus {
    // Check last state commitment time
    // If > 3 days, emergency redemption available
}
```

**Validation:** Can build valid redemption transactions ✅

---

### Phase 5: NOSTR Alerts

**Goal:** Publish tracker state and alerts

**Key Files:**
- `specs/tracker.md` - NOSTR integration details

**Alert Types:**
1. Debt created/updated
2. Collateralization warning (80%/100%)
3. State committed to blockchain
4. Emergency redemption detected

```rust
use nostr_sdk::{Client, Keys, Event};

struct NostrPublisher {
    client: Client,
    keys: Keys,  // Tracker's NOSTR keys
}

impl NostrPublisher {
    async fn publish_alert(&self, alert: &TrackerAlert) -> Result<(), Error> {
        let content = serde_json::to_string(alert)?;
        let event = Event::new(text, &self.keys);
        self.client.publish_event(event).await
    }
}
```

**Validation:** Alerts published and receivable ✅

---

## Protocol Flow Summary

### Normal Flow

```
1. Alice wants to buy from Bob
   ↓
2. Alice creates IOU: "I owe Bob 50M nanoERG"
   ↓
3. Tracker witnesses IOU (signs with tracker key)
   ↓
4. Bob receives witnessed IOU note
   ↓
5. Bob can redeem anytime against Alice's reserve
   ↓
6. Tracker provides AVL proof for redemption
   ↓
7. On-chain contract verifies:
   - Alice's signature ✅
   - Tracker's signature ✅
   - AVL proof (debt in tracker tree) ✅
   - Amount <= (total_debt - already_redeemed) ✅
   - Timestamp > previous timestamp ✅
   ↓
8. Contract pays Bob from reserve
```

### Emergency Flow (Tracker Offline)

```
1. Tracker goes offline
   ↓
2. Wait 2160 blocks (~3 days)
   ↓
3. Bob redeems WITHOUT tracker signature
   ↓
4. Contract verifies:
   - Emergency period elapsed ✅
   - Alice's signature ✅
   - Last committed state proof ✅
   ↓
5. Contract pays Bob from reserve
```

---

## Critical Contract Rules

From `../contract/basis.es`, the on-chain contract enforces:

### Redemption Checks (ALL must pass)

1. **selfPreserved**: Contract proposition, tokens, R4, R6 unchanged
2. **trackerIdCorrect**: Tracker NFT ID matches R6 register
3. **trackerDebtCorrect**: Debt exists in tracker's AVL tree (proof #8)
4. **timestampCorrect**: New timestamp > stored timestamp
5. **properRedemptionTree**: AVL tree correctly updated (proof #5)
6. **properReserveSignature**: Valid Schnorr signature from owner
7. **properlyRedeemed**: 
   - `redeemed > 0`
   - `redeemed <= (total_debt - redeemed_debt)`
   - Tracker signature valid OR emergency period passed
8. **receiverCondition**: Receiver can prove knowledge of secret key

### Action Codes

```
Action 0: Redemption
Action 1: Top-up (add collateral)
```

### Emergency Period

```
2160 blocks ≈ 3 days (at ~2.5 min/block)
After this, tracker signature becomes OPTIONAL
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_iou_message_format() { ... }
    
    #[test]
    fn test_schnorr_signing() { ... }
    
    #[test]
    fn test_debt_hash() { ... }
    
    #[test]
    fn test_avl_tree_insert() { ... }
    
    #[test]
    fn test_avl_tree_lookup() { ... }
    
    #[test]
    fn test_emergency_period_detection() { ... }
}
```

### Integration Tests

```rust
#[test]
fn test_full_redemption_flow() {
    // 1. Create reserve (use BasisDeployer)
    // 2. Create IOU note
    // 3. Build redemption transaction
    // 4. Sign with Ergo node
    // 5. Submit to testnet
    // 6. Verify Bob received funds
}
```

### Use Demo Data

```bash
# Load example note
cat demo/note.json | jq .

# Verify signatures against message
# (implement in Rust using test vectors from reference/message-formats.md)
```

---

## Key Differences from Lightning/Cashu

| Feature | Basis | Lightning | Cashu |
|---------|-------|-----------|-------|
| Collateral first | No | Yes | Yes |
| On-chain settlement | Yes | Yes | No |
| Credit creation | Yes | No | No |
| Tracker trust | Minimal | Custodial | Custodial |
| Micropayments | Yes | Yes | Yes |
| Offline support | Emergency exit | Limited | Limited |
| Mesh networks | Yes | No | No |

---

## Resources

### In This Package
- `../specs/ergo/basis_contract_rust_notes.md` - Rust implementation notes
- `reference/message-formats.md` - Complete format specifications
- `specs/basis.md` - Full protocol specification
- `specs/tracker.md` - Tracker architecture

### External
- [Ergo Documentation](https://docs.ergoplatform.com/)
- [ErgoScript Reference](https://github.com/ergoplatform/ergoscript)
- [AVL+ Trees](https://github.com/kushti/ergo-avltree)
- [NOSTR Protocol](https://github.com/nostr-protocol/nostr)

### Scala Reference Implementation
- `demo/src/BasisNoteCreator.scala` - Note creation
- `demo/src/BasisNoteRedeemer.scala` - Redemption logic
- `demo/src/TrackerBoxSetup.scala` - Tracker setup
- `tests/BasisSpec.scala` - Test suite

---

## Development Checklist

- [ ] Cryptography: Schnorr signature verification
- [ ] Cryptography: IOU message construction (48 bytes)
- [ ] AVL Tree: Insert operation with proof generation
- [ ] AVL Tree: Lookup operation with proof generation
- [ ] Core: Debt tracking (cumulative per pair)
- [ ] Core: Note witnessing (tracker signature)
- [ ] Core: Debt transfer (novation)
- [ ] Blockchain: Tracker box creation
- [ ] Blockchain: Redemption transaction building
- [ ] Blockchain: Emergency period detection
- [ ] NOSTR: Alert publishing
- [ ] Testing: Unit tests for all components
- [ ] Testing: Integration with Ergo testnet
- [ ] Documentation: API reference
- [ ] Security: Audit signature verification

---

## Common Pitfalls

See `reference/message-formats.md#13-common-pitfalls` for detailed list.

**Top 3:**
1. ❌ Wrong z-component encoding (must be unsigned 32 bytes)
2. ❌ Missing timestamp in message (48 bytes, not 40)
3. ❌ Uncompressed public keys (must be 33 bytes compressed)

---

## Getting Help

1. Check `specs/basis.md` for protocol details
2. Check `../specs/ergo/basis_contract_rust_notes.md` for contract logic
3. Check `reference/message-formats.md` for data formats
4. Run `tests/BasisSpec.scala` to verify against reference
5. Use `demo/note.json` as test vector

---

**Last Updated:** April 9, 2026  
**Protocol Version:** 1.0  
**Contract:** `../contract/basis.es`
