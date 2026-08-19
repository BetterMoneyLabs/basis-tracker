# Basis Simple Demo - Required Fixes

**Priority Order:** Critical → Medium → Low  
**Date:** April 9, 2026

---

## 🔴 CRITICAL FIXES

### Fix #1: Update SPECIFICATION.md Message Format

**File:** `demo/basis/simple/SPECIFICATION.md`  
**Section:** 5.1 - Message Construction

**Current (WRONG):**
```markdown
val message = key ++ Longs.toByteArray(totalDebt)
// Total: 40 bytes (32 + 8)
```

**Should be:**
```markdown
val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
// Total: 48 bytes (32 + 8 + 8)

Example:
6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4 0000000002faf080 00000194f8c88000
││                                                               ││                 ││
└─ Key (Blake2b256 of owner||receiver)                           └─ Debt (50M)      └─ Timestamp (ms)
```

---

### Fix #2: Update SPECIFICATION.md Value Encoding

**File:** `demo/basis/simple/SPECIFICATION.md`  
**Section:** 4.3 - Value Encoding

**Current (WRONG):**
```markdown
val value = Longs.toByteArray(amount)
// Example: 0000000002faf080 (50000000 in big-endian)
```

**Should be:**
```markdown
**Reserve Tree Value Format:**
```scala
val value = Longs.toByteArray(timestamp) ++ Longs.toByteArray(redeemedAmount)
// 8 bytes timestamp + 8 bytes redeemedAmount = 16 bytes total
// Example: 00000194f8c880000000000002faf080
//          ││              ││
//          └─ Timestamp    └─ Redeemed Amount
```

**Tracker Tree Value Format:**
```scala
val value = Longs.toByteArray(totalDebt)
// 8 bytes (total debt only, no timestamp)
// Example: 0000000002faf080
```
```

---

### Fix #3: Update SPECIFICATION.md Contract Conditions

**File:** `demo/basis/simple/SPECIFICATION.md`  
**Section:** 6 - Contract Conditions

**Fixed:**
```markdown
properReserveSignature:
- Schnorr signature verification with message = `key || totalDebt || timestamp`
- Message is 48 bytes (32 + 8 + 8)
- Both owner and tracker sign the SAME message including timestamp
```

**Note:** This was previously documented as `key || totalDebt` (40 bytes) but the actual implementation and contract always used 48 bytes with timestamp.

**Also add new condition:**
```markdown
timestampCorrect:
- New timestamp must be > stored timestamp from AVL tree
- Prevents replay attacks with old notes
- For first redemption: stored timestamp = 0, so any valid timestamp works
```

---

### Fix #4: Document Context Variable #7

**File:** `demo/basis/simple/SPECIFICATION.md`  
**Section:** 3.1 - Input Structure

**Add to context extension:**
```json
{
  "extension": {
    "0": "0200",
    "1": "<receiver_pubkey>",
    "2": "<reserve_signature>",
    "3": "<total_debt>",
    "4": "<timestamp>",
    "5": "<reserve_insert_proof>",
    "6": "<tracker_signature>",
    "7": "<reserve_lookup_proof>",  // OPTIONAL - only needed for 2nd+ redemption
    "8": "<tracker_lookup_proof>"
  }
}
```

**Add documentation:**
```markdown
**Context Variable #7 (Optional):**
- Purpose: Proof for looking up previous redemption state in reserve's AVL tree
- Format: AVL proof for `hash(ownerKey||receiverKey)` in reserve tree
- Needed: When `redeemedDebt > 0` (second or later redemption for same pair)
- Not needed: For first redemption (defaults to `storedTimestamp=0`, `redeemedDebt=0`)
- Contract handles this with: `if (lookupProofOpt.isDefined) { ... } else { 0L }`
```

---

## 🟡 MEDIUM PRIORITY FIXES

### Fix #5: Verify sign_request.json R5 Register

**File:** `demo/basis/simple/sign_request.json`

**Current R5:**
```json
"R5": "642c1d1fb21a9df51972a5439ca7ce8d5601f99c871f15cbf2c4ff6ae53d57a96f01012000"
```

**Issue:** This appears to be an empty tree format. After redemption, R5 should contain the redemption record.

**Action:** Re-run the redeemer to generate correct transaction:
```bash
sbt "runMain chaincash.contracts.BasisNoteRedeemer \
  --note-json note.json \
  --reserve-box auto \
  --tracker-box auto \
  --fee-box $FEE_BOXES \
  --output sign_request.json"
```

**Verify the new R5:**
- Should NOT match empty tree digest
- Should contain 1 entry with 16-byte value
- Value format: `timestamp (8 bytes) ++ 50000000 (8 bytes)`

---

### Fix #6: Add Sequential Redemption Test

**File:** `src/test/scala/chaincash/demo/BasisDemoSpec.scala` (create if doesn't exist)

**Add test:**
```scala
"Sequential redemptions" should "enforce timestamp ordering" in {
  // 1. Create first note with timestamp T1
  // 2. Redeem it (stores T1 in AVL tree)
  // 3. Create second note with timestamp T2 < T1 (older)
  // 4. Try to redeem - should FAIL (replay attack prevention)
  
  // 5. Create third note with timestamp T3 > T1 (newer)
  // 6. Redeem it - should SUCCEED
  // 7. Verify AVL tree now has T3, not T1
}
```

---

### Fix #7: Add Emergency Redemption Test

**File:** `src/test/scala/chaincash/demo/BasisDemoSpec.scala`

**Add test:**
```scala
"Emergency redemption" should "work without tracker signature after timeout" in {
  // 1. Create note WITHOUT tracker signature (or use empty bytes)
  // 2. Mock blockchain height to be > tracker creation + 2160 blocks
  // 3. Attempt redemption - should SUCCEED without tracker sig
  
  // 4. Try same redemption at normal height (before timeout)
  // 5. Should FAIL (tracker sig required)
}
```

---

### Fix #8: Update SPECIFICATION.md Section 6.1

**File:** `demo/basis/simple/SPECIFICATION.md`  
**Section:** 6.1 - Condition Details

**Add:**
```markdown
**timestampCorrect:**
- `timestamp > storedTimestamp`
- Where `storedTimestamp` comes from AVL tree lookup (context var #7)
- If no lookup (first redemption): `storedTimestamp = 0`
- Prevents replay attacks: can't redeem same note twice or with older timestamp

**properlyRedeemed:**
- `redeemed > 0` (must redeem something)
- `redeemed <= (totalDebt - redeemedDebt)` (can't redeem more than owed)
- `trackerSigValid` (valid tracker signature OR emergency period passed)
- Emergency period: `(HEIGHT - trackerCreationHeight) > 2160` (3 days)
```

---

## 🟢 LOW PRIORITY FIXES

### Fix #9: Update README.md Examples

**File:** `demo/basis/simple/README.md`

**Add to Key Parameters table:**
```markdown
| Parameter | Value |
|-----------|-------|
| Message Format | `key (32) || totalDebt (8) || timestamp (8)` = 48 bytes |
| Reserve Tree Value | `timestamp (8) || redeemedAmount (8)` = 16 bytes |
| Tracker Tree Value | `totalDebt (8)` = 8 bytes |
| Emergency Period | 2160 blocks (~3 days) |
| Context Var #7 | Optional (reserve lookup proof) |
```

---

### Fix #10: Add Timestamp to note.json Human Output

**File:** `demo/basis/simple/src/BasisNoteCreator.scala`

**Current output:**
```scala
val timestampStr = new java.text.SimpleDateFormat("yyyy-MM-dd HH:mm:ss").format(new java.util.Date(note.timestamp))
```

**Add validation:**
```scala
val now = System.currentTimeMillis()
if (note.timestamp > now + 86400000) {
  Console.err.println("WARNING: Timestamp is more than 1 day in the future!")
}
if (note.timestamp < now - 31536000000L) {
  Console.err.println("WARNING: Timestamp is more than 1 year in the past!")
}
```

---

## Testing Checklist

After applying fixes, verify:

- [ ] SPECIFICATION.md message format shows 48 bytes
- [ ] SPECIFICATION.md value encoding shows 16 bytes for reserve tree
- [ ] SPECIFICATION.md documents context variable #7
- [ ] SPECIFICATION.md includes timestampCorrect condition
- [ ] sign_request.json regenerated with correct R5 (not empty tree)
- [ ] Sequential redemption test exists and passes
- [ ] Emergency redemption test exists and passes
- [ ] README.md updated with new parameters
- [ ] Note creator warns about suspicious timestamps

---

## Files to Modify

1. `demo/basis/simple/SPECIFICATION.md` - Sections 3.1, 4.3, 5.1, 6, 6.1
2. `demo/basis/simple/sign_request.json` - Regenerate
3. `demo/basis/simple/README.md` - Key parameters table
4. `demo/basis/simple/src/BasisNoteCreator.scala` - Add timestamp validation
5. `src/test/scala/chaincash/demo/BasisDemoSpec.scala` - Add new tests

---

## Verification Commands

```bash
# 1. Regenerate transaction
sbt "runMain chaincash.contracts.BasisNoteRedeemer \
  --note-json note.json \
  --reserve-box auto \
  --tracker-box auto \
  --fee-box $FEE_BOXES \
  --output sign_request_new.json"

# 2. Decode R5 register from new file
python3 -c "
import json
from base64 import b16decode
with open('sign_request_new.json') as f:
    data = json.load(f)
    r5 = data['tx']['outputs'][0]['additionalRegisters']['R5']
    print(f'R5 hex: {r5}')
    print(f'R5 length: {len(r5)//2} bytes')
    # Decode AVL tree structure
    # Type(1) + Digest(33) + Flags(1) + KeyLen(1) + KeyLen(4) + ValueLen(4 if present)
"

# 3. Run tests
sbt "testOnly chaincash.demo.BasisDemoSpec"

# 4. Verify message format in note
python3 -c "
import json
with open('note.json') as f:
    note = json.load(f)
    msg = bytes.fromhex(note['message'])
    print(f'Message length: {len(msg)} bytes')
    print(f'Key (32): {msg[:32].hex()}')
    print(f'TotalDebt (8): {msg[32:40].hex()} = {int.from_bytes(msg[32:40], \"big\")}')
    print(f'Timestamp (8): {msg[40:48].hex()} = {int.from_bytes(msg[40:48], \"big\")}')
"
```

---

**Status:** Ready to implement fixes  
**Estimated effort:** 2-3 hours  
**Risk:** Low (documentation + test updates only, production code is correct)
