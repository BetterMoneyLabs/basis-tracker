# Basis Simple Demo vs basis.es Contract - Compatibility Analysis

**Date:** April 9, 2026  
**Status:** ⚠️ CRITICAL ISSUES FOUND

---

## Executive Summary

The simple demo has **several critical mismatches** with the current `basis.es` contract that will cause redemption to fail. The contract has evolved to include timestamp tracking and replay protection, but the demo artifacts haven't been updated accordingly.

---

## Critical Issues

### 🔴 CRITICAL #1: Missing Context Variable #7 (AVL Lookup Proof)

**Contract requires:**
- Context variable #7: OPTIONAL proof for AVL tree lookup in reserve's tree
- Used to retrieve `(storedTimestamp, redeemedDebt)` from previous redemptions
- Format: `hash(ownerKey||receiverKey) -> (timestamp, redeemedAmount)`

**Demo provides:**
- ❌ NO context variable #7 in `sign_request.json`
- The contract code checks `getVar[Coll[Byte]](7)` which will be undefined
- For first redemption, this is OK (contract handles `isDefined` check)
- **BUT**: The SPECIFICATION.md doesn't mention this variable at all

**Impact:** 
- ✅ First redemption: Works (contract defaults to `storedTimestamp = 0`, `redeemedDebt = 0`)
- ⚠️ Subsequent redemptions: Will FAIL without proper #7 proof

**Fix:** Update SPECIFICATION.md to document context var #7, even if not used in first redemption.

---

### 🔴 CRITICAL #2: Message Format Mismatch in Demo Note

**Contract expects:**
```scala
val message = key ++ longToByteArray(totalDebt) ++ longToByteArray(timestamp)
// 32 bytes + 8 bytes + 8 bytes = 48 bytes
```

**Demo note.json has:**
```json
{
  "message": "6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a40000000002faf08000000194f8c88000",
  "messageFormat": "key (32 bytes) || totalDebt (8 bytes) || timestamp (8 bytes)"
}
```

Let's decode:
- Key (32 bytes): `6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4` ✅
- TotalDebt (8 bytes): `0000000002faf080` = 50,000,000 nanoERG ✅
- Timestamp (8 bytes): `00000194f8c88000` = **1,743,379,200,000** ms ✅

**Actual timestamp in note.json:** `1743379200000` (March 29, 2025)

✅ **Message format is CORRECT** - includes timestamp as required by contract.

---

### 🟡 MEDIUM #3: Contract Requires Timestamp Verification

**Contract checks:**
```scala
val timestampCorrect = timestamp > storedTimestamp
```

**Demo doesn't test:**
- Replay attack prevention
- Timestamp ordering
- Multiple redemptions with different timestamps

**Impact:** Demo only tests first redemption path, doesn't validate timestamp enforcement.

---

### 🟡 MEDIUM #4: Tracker Signature Emergency Period Not Tested

**Contract supports:**
```scala
val enoughTimeSpent = (HEIGHT - trackerUpdateTime) > 3 * 720 // 3 days
val trackerSigValid = if (trackerSigProvided) {
  properTrackerSignature
} else {
  enoughTimeSpent
}
```

**Demo only tests:**
- ✅ Normal redemption WITH tracker signature
- ❌ Emergency redemption WITHOUT tracker signature (after 3 days)

**Impact:** Emergency exit path is untested.

---

### 🟡 MEDIUM #5: AVL Tree Value Format Changed

**Contract now stores:**
```scala
// Reserve tree value format:
val treeValue = longToByteArray(timestamp) ++ longToByteArray(newRedeemed)
// 8 bytes timestamp + 8 bytes redeemedAmount = 16 bytes total
```

**SPECIFICATION.md states:**
```markdown
### 4.2 Key Construction
val key = Blake2b256(ownerKeyBytes ++ receiverKeyBytes)

### 4.3 Value Encoding
val value = Longs.toByteArray(amount)
// Example: 0000000002faf080 (50000000 in big-endian)
```

❌ **Specification is OUTDATED** - still shows old 8-byte format, not new 16-byte format with timestamp.

**BasisNoteRedeemer.scala is CORRECT:**
```scala
// Tree value format: timestamp (8 bytes) ++ redeemedAmount (8 bytes) = 16 bytes
val treeValue = Longs.toByteArray(timestamp) ++ Longs.toByteArray(redeemedAmount)
```

**Impact:** Documentation mismatch, but code appears correct.

---

### 🟢 MINOR #6: Context Variable Indexing

**Contract context vars:**
```scala
// #0 - action code
// #1 - receiver pubkey
// #2 - reserve owner signature
// #3 - total debt amount
// #4 - timestamp
// #5 - reserve insert proof
// #6 - tracker signature
// #7 - [OPTIONAL] reserve tree lookup proof
// #8 - tracker tree lookup proof
```

**Demo sign_request.json context vars:**
```json
{
  "0": "0200",           // action=0, index=0 ✅
  "1": "<payee_key>",    // receiver pubkey ✅
  "2": "<reserve_sig>",  // reserve signature ✅
  "3": "<total_debt>",   // total debt ✅
  "4": "<timestamp>",    // timestamp ✅ (present in code)
  "5": "<reserve_proof>",// reserve insert proof ✅
  "6": "<tracker_sig>",  // tracker signature ✅
  "8": "<tracker_proof>" // tracker lookup proof ✅
}
```

✅ Context variables are correctly mapped (note: #7 is intentionally absent for first redemption).

---

## Code-Specific Issues

### BasisNoteRedeemer.scala

**✅ CORRECT:**
1. Message construction includes timestamp
2. AVL proof generation for both reserve and tracker trees
3. Context variable mapping includes all required vars
4. Signature encoding uses BouncyCastle for 32-byte z values
5. Reserve tree insert proof generation with 16-byte values

**⚠️ NEEDS IMPROVEMENT:**
1. No test for emergency redemption (no tracker signature)
2. No test for multiple sequential redemptions
3. No validation that `timestamp > storedTimestamp`

---

### BasisNoteCreator.scala

**✅ CORRECT:**
1. Message includes timestamp: `key || totalDebt || timestamp`
2. Both payer and tracker signatures generated
3. JSON output includes all required fields

**⚠️ MINOR:**
1. Could document timestamp format more clearly (Java milliseconds)
2. No validation that timestamp is reasonable (not in future, not too old)

---

### BasisDeployer.scala

**✅ CORRECT:**
1. Reserve tree initialized as empty with correct parameters
2. Registers R4 (owner key), R5 (empty AVL tree), R6 (tracker NFT) set correctly
3. Uses `chainCashPlasmaParameters` consistently

---

### TrackerBoxSetup.scala

**✅ CORRECT:**
1. Debt key construction: `Blake2b256(payerKey || payeeKey)`
2. Value stored as `Longs.toByteArray(totalDebt)` (8 bytes)
3. AVL tree uses correct `InsertOnly` flags
4. Uses `chainCashPlasmaParameters`

---

### sign_request.json

Let me decode the actual transaction to verify:

**Reserve Output R5 register:**
```
"R5": "642c1d1fb21a9df51972a5439ca7ce8d5601f99c871f15cbf2c4ff6ae53d57a96f01012000"
```

Breaking this down:
- `64` = AvlTree type tag
- `2c1d1fb21a9df51972a5439ca7ce8d5601f99c871f15cbf2c4ff6ae53d57a96f` = 32-byte digest + height byte
- `01` = Flags (InsertOnly)
- `01` = Key length present
- `20` = Key length (32 bytes)
- `00` = Value length (None for empty tree? But this should have data!)

❌ **PROBLEM:** This looks like an **empty tree** format, but after redemption it should contain the redeemed amount data.

**Expected after redemption:**
- Tree should have 1 entry: `hash(ownerKey||payeeKey) -> (timestamp, redeemedAmount)`
- 16-byte value: `00000194f8c88000` (timestamp) + `0000000002faf080` (50M nanoERG)

---

## SPECIFICATION.md Issues

### Outdated Sections:

**Section 3.1 - Input Structure:**
- ❌ Missing context variable #7 documentation
- ✅ Has context variable #8 (tracker proof)

**Section 4.3 - Value Encoding:**
```markdown
val value = Longs.toByteArray(amount)
// Example: 0000000002faf080 (50000000 in big-endian)
```
❌ **WRONG** - Should be 16 bytes: `timestamp ++ redeemedAmount`

**Section 5.1 - Message Construction:**
```markdown
val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
// Total: 48 bytes (32 + 8 + 8)
```
✅ **CORRECT** - 48 bytes: `key ++ totalDebt ++ timestamp`

**Section 6 - Contract Conditions:**
```markdown
properReserveSignature:
- Schnorr signature verification with message = `key || totalDebt || timestamp`
```
✅ **CORRECT** - Message is `key || totalDebt || timestamp` (48 bytes)

---

## Test Coverage Gaps

| Test Scenario | Covered? | Priority |
|---------------|----------|----------|
| First redemption with tracker sig | ✅ Yes | - |
| First redemption AVL proofs | ✅ Yes | - |
| Signature format (65 bytes) | ✅ Yes | - |
| **Second redemption with lookup proof** | ❌ No | 🔴 HIGH |
| **Timestamp replay protection** | ❌ No | 🔴 HIGH |
| **Emergency redemption (no tracker sig)** | ❌ No | 🟡 MEDIUM |
| **Multiple redemptions same pair** | ❌ No | 🟡 MEDIUM |
| **Partial redemption** | ❌ No | 🟡 MEDIUM |
| **Tracker signature invalid** | ❓ Unclear | 🟡 MEDIUM |
| **Reserve signature invalid** | ❓ Unclear | 🟡 MEDIUM |

---

## Recommendations

### Immediate (Must Fix):

1. **Update SPECIFICATION.md Section 4.3**
   - Change value encoding from 8 bytes to 16 bytes
   - Document timestamp + redeemedAmount format

2. **Update SPECIFICATION.md Section 5.1**
   - Change message format to include timestamp
   - Update from 40 bytes to 48 bytes

3. **Update SPECIFICATION.md Section 3.1**
   - Document context variable #7 (even as optional)
   - Clarify when it's needed vs not needed

4. **Fix sign_request.json**
   - Verify R5 register in output is correct (shouldn't be empty tree)
   - Verify reserve proof generates correct 16-byte values

### Short Term (Should Fix):

5. **Add integration tests for:**
   - Sequential redemptions (test timestamp enforcement)
   - Emergency redemption path
   - Invalid signature rejection

6. **Update README.md examples**
   - Show timestamp in human-readable format
   - Document emergency redemption workflow

### Long Term (Nice to Have):

7. **Create test harness that:**
   - Deploys real reserve on testnet
   - Creates multiple notes
   - Redeems sequentially to test state transitions
   - Tests emergency exit after timeout

---

## Conclusion

The demo code (`BasisNoteRedeemer.scala`, `BasisNoteCreator.scala`, etc.) appears to be **mostly correct** and aligned with the current `basis.es` contract.

However, the **documentation (SPECIFICATION.md) is significantly outdated** and describes the old message/value formats without timestamp support.

The **sign_request.json may have issues** with the R5 register format - needs verification that it's not using an empty tree when it should contain redemption data.

**Priority:** Fix documentation immediately, verify transaction JSON, add sequential redemption tests.
