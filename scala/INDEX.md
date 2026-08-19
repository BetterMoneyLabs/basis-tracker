# Basis Tracker Rust - Quick Reference Index

**Location:** `/out/basis-tracker-rust/`  
**Purpose:** Complete reference for implementing BasisTracker in Rust

---

## 📁 Directory Structure

```
basis-tracker-rust/
├── README.md                          ⭐ START HERE - Implementation guide
├── contract/
│   ├── basis.es                       ← On-chain reserve contract
│   └── basis-es-rust-notes.md         ← Rust-specific annotations
├── specs/
│   ├── basis.md                       ← Protocol specification
│   └── tracker.md                     ← Tracker architecture
├── tests/
│   └── BasisSpec.scala                ← Reference test suite
├── demo/
│   ├── note.json                      ← Example IOU note with signatures
│   ├── sign_request.json              ← Example redemption transaction
│   ├── tracker_box_setup.json         ← Tracker box configuration
│   ├── SPECIFICATION.md               ← Detailed technical spec
│   ├── README.md                      ← Demo instructions
│   ├── debug_signing.sh               ← Debug signing issues
│   ├── ANALYSIS.md                    ← Contract compatibility analysis
│   ├── FIXES.md                       ← Required fixes for demo
│   └── src/                           ← Scala reference utilities
│       ├── BasisDeployer.scala
│       ├── BasisNoteCreator.scala
│       ├── BasisNoteRedeemer.scala
│       └── TrackerBoxSetup.scala
├── scala-utils/                       ← Scala reference code
│   ├── Constants.scala                ← Contract compilation
│   ├── AddressUtils.scala             ← Key/address derivation
│   ├── ParticipantSecretsReader.scala ← Demo participant keys
│   └── SigUtils.scala                 ← Signature utilities
└── reference/
    └── message-formats.md             ⭐ CRITICAL - All data formats
```

---

## 🎯 Key Files by Task

### Implementing Signature Verification
1. `reference/message-formats.md` - Section 3 (Signatures)
2. `demo/note.json` - Test vectors
3. `scala-utils/SigUtils.scala` - Reference implementation
4. `contract/basis-es-rust-notes.md` - Section 3

### Building AVL Tree Operations
1. `reference/message-formats.md` - Section 5 (AVL Trees)
2. `specs/tracker.md` - Tree architecture
3. `tests/BasisSpec.scala` - AVL tests
4. `demo/src/BasisNoteRedeemer.scala` - Proof generation

### Creating IOU Notes
1. `reference/message-formats.md` - Section 4 (Messages)
2. `specs/basis.md` - Protocol flow
3. `demo/src/BasisNoteCreator.scala` - Reference code
4. `demo/note.json` - Output format

### Building Redemption Transactions
1. `reference/message-formats.md` - Section 6-7 (Boxes, Context vars)
2. `contract/basis.es` - Contract logic
3. `demo/sign_request.json` - Transaction example
4. `demo/src/BasisNoteRedeemer.scala` - Reference code

### Understanding Contract Logic
1. `contract/basis.es` - Full contract
2. `contract/basis-es-rust-notes.md` - Rust annotations
3. `specs/basis.md` - Protocol spec
4. `demo/ANALYSIS.md` - Compatibility analysis

---

## 🔑 Critical Values

### Message Format
```
48 bytes total: key (32) || totalDebt (8) || timestamp (8)
```

### Signature Format
```
65 bytes total: a (33 bytes compressed point) || z (32 bytes unsigned scalar)
```

### AVL Tree Parameters
```
keyLength: 32
valueLength: None (dynamic)
flags: InsertOnly (0x01)
```

### Emergency Period
```
2160 blocks ≈ 3 days
```

### Demo Test Vectors
```json
// From demo/note.json
{
  "payerKey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
  "payeeKey": "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
  "totalDebt": 50000000,
  "timestamp": 1743379200000,
  "message": "6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a40000000002faf08000000194f8c88000"
}
```

---

## ⚠️ Common Pitfalls

1. **Wrong z encoding** - Must be unsigned 32 bytes (use BouncyCastle, not BigInt.toByteArray)
2. **Wrong message length** - 48 bytes (with timestamp), NOT 40 bytes
3. **Uncompressed keys** - Must use 33-byte compressed format
4. **Wrong endianness** - Numbers are little-endian, not big-endian
5. **Timestamp units** - Milliseconds, not seconds
6. **Missing context var #7** - Required for 2nd+ redemptions
7. **AVL params mismatch** - Must use InsertOnly flags with (32, None)

**See:** `reference/message-formats.md` Section 13 for full list

---

## 📚 Reading Order

### For Rust Implementation
1. `README.md` - Overview and phases
2. `reference/message-formats.md` - Data structures
3. `contract/basis-es-rust-notes.md` - Contract logic
4. `specs/basis.md` - Protocol details
5. `specs/tracker.md` - Tracker architecture

### For Understanding Protocol
1. `specs/basis.md` - High-level overview
2. `demo/README.md` - Quick start
3. `demo/note.json` - Concrete example
4. `contract/basis.es` - Contract details

### For Testing
1. `tests/BasisSpec.scala` - Reference tests
2. `demo/note.json` - Test vectors
3. `demo/sign_request.json` - Transaction format
4. `demo/ANALYSIS.md` - Known issues

---

## 🔗 External Resources

- [Ergo Docs](https://docs.ergoplatform.com/)
- [ErgoScript](https://github.com/ergoplatform/ergoscript)
- [secp256k1 (k256 crate)](https://docs.rs/k256/latest/k256/)
- [Blake2](https://docs.rs/blake2/latest/blake2/)
- [NOSTR](https://github.com/nostr-protocol/nostr)

---

## 📞 Getting Help

1. Check `demo/ANALYSIS.md` for known compatibility issues
2. Check `demo/FIXES.md` for required corrections
3. Run `tests/BasisSpec.scala` to verify against reference
4. Use `demo/debug_signing.sh` to debug signing issues

---

**Created:** April 9, 2026  
**Protocol Version:** 1.0  
**Total Files:** 23
