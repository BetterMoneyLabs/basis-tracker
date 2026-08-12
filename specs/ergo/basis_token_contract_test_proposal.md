# Proposed Additional Tests for `contract/basis-token.es`

## Background

`scala/src/test/scala/basis/contracts/BasisTokenSpec.scala` already contains 31 property-based tests covering the token-backed Basis reserve contract. All of them currently pass. This document proposes additional test scenarios that are either not yet exercised or that exercise important edge cases specific to the token-backed variant.

The proposals are written as ScalaCheck `property` tests intended for `BasisTokenSpec`. Each entry gives the test name, what it verifies, and the rough test setup.

## Proposed Tests

### 1. Emergency Redemption — Success After 3 Days Without Tracker Signature

**What it verifies:** After the emergency period (`HEIGHT - tracker.creationInfo._1 > 3 * 720`), a note can be redeemed with a valid reserve owner signature and an empty tracker signature.

**Setup:**
- Create a tracker data input with `creationHeight = ctx.getHeight - 3 * 720 - 1`.
- Set context var #6 to an empty byte array.
- Provide valid reserve owner signature and all other required proofs.
- Expect transaction to succeed.

### 2. Emergency Redemption — Failure Before 3 Days Without Tracker Signature

**What it verifies:** Before the emergency period, omitting the tracker signature causes script failure.

**Setup:**
- Create a tracker data input with `creationHeight = ctx.getHeight`.
- Set context var #6 to an empty byte array.
- Expect transaction to fail with script validation error.

### 3. Emergency Redemption — Valid Tracker Signature Still Accepted After Emergency Period

**What it verifies:** Even after the emergency period, a valid tracker signature is accepted.

**Setup:**
- Create an old tracker data input as in test #1.
- Provide a valid tracker signature in context var #6.
- Expect transaction to succeed.

### 4. Emergency Redemption — Invalid Tracker Signature Before Emergency Period Fails

**What it verifies:** Before the emergency period, an invalid tracker signature is rejected even if all other fields are valid.

**Setup:**
- Create a fresh tracker data input.
- Corrupt the tracker signature bytes.
- Expect transaction to fail.

### 5. Top-up — Changing Token ID at Position #0 Fails

**What it verifies:** The contract enforces token ID preservation for both token positions during top-up.

**Setup:**
- Build a top-up transaction where the output reserve box has a different token ID at position #0.
- Expect script validation failure.

### 6. Top-up — Changing Token ID at Position #1 Fails

**What it verifies:** The reserve token ID cannot be swapped during top-up.

**Setup:**
- Build a top-up transaction where the output reserve box has a different token ID at position #1.
- Expect script validation failure.

### 7. Top-up — Exactly 1 Token Unit Boundary Succeeds

**What it verifies:** The minimum top-up of exactly 1 token unit is accepted.

**Setup:**
- Top-up the reserve by exactly 1 token unit.
- Expect transaction to succeed.

### 8. Redemption — Negative Redeemed Amount Fails

**What it verifies:** The contract rejects an output where the reserve token #1 amount is **greater** than the input amount (which would imply negative redemption).

**Setup:**
- Build a redemption output where `selfOut.tokens(1)._2 > SELF.tokens(1)._2`.
- Expect script validation failure.

### 9. Redemption — Extra Token in Reserve Output Fails

**What it verifies:** The reserve output must contain exactly two tokens.

**Setup:**
- Build a redemption transaction where the updated reserve output contains a third, unrelated token.
- Expect script validation failure.

### 10. Redemption — Missing Token #0 in Reserve Output Fails

**What it verifies:** The reserve NFT must remain in the reserve output.

**Setup:**
- Build a redemption output reserve box that contains only token #1.
- Expect script validation failure.

### 11. Redemption — Missing Token #1 in Reserve Output Fails

**What it verifies:** The reserve token must remain in the reserve output.

**Setup:**
- Build a redemption output reserve box that contains only token #0.
- Expect script validation failure.

### 12. Redemption — Token #0 Amount Not Equal to 1 in Output Fails

**What it verifies:** The reserve NFT amount must remain 1 in the output.

**Setup:**
- Build a redemption output reserve box where token #0 amount is 2 (or any value other than 1).
- Expect script validation failure.

### 13. Initiate Refund — Taking ERG Out Fails

**What it verifies:** During refund initiation, neither ERG nor reserve tokens may leave the box.

**Setup:**
- Build an initiate-refund transaction where the output reserve box has a lower ERG value than the input.
- Expect script validation failure.

### 14. Complete Refund — Failing to Take Reserve Tokens Fails

**What it verifies:** Complete refund must withdraw all tokens from the reserve; leaving the reserve NFT or reserve token behind should fail.

**Setup:**
- Build a complete-refund transaction whose output still holds the reserve NFT or reserve token.
- Expect script validation failure.

### 15. Redemption — Timestamp Equal to Stored Timestamp Fails

**What it verifies:** The contract requires `timestamp > storedTimestamp`, not `>=`.

**Setup:**
- Pre-populate the reserve AVL tree with an entry whose timestamp equals the timestamp of the note being redeemed.
- Expect script validation failure.

### 16. Redemption — Multiple Notes at Non-Zero Output Index

**What it verifies:** The `index = action % 10` mechanism allows the reserve output to be at a position other than 0.

**Setup:**
- Build a transaction with multiple redemption outputs and place the updated reserve box at output index 1.
- Set context var #0 to `0 * 10 + 1 = 1`.
- Expect transaction to succeed.

### 17. Redemption — Action Byte Out of Range Fails

**What it verifies:** Any action byte other than 0, 1, 2, or 3 results in `sigmaProp(false)`.

**Setup:**
- Attempt a transaction with action byte 4 (or 40, 50, etc.).
- Expect script validation failure.

### 18. Redemption — Total Debt of Zero Fails

**What it verifies:** A note with `totalDebt = 0` cannot be redeemed (redemption amount must be > 0).

**Setup:**
- Build a redemption attempt where the committed total debt is 0.
- Expect script validation failure.

### 19. Top-up — Zero Token Increase Fails

**What it verifies:** A top-up transaction that does not actually increase token #1 amount is rejected.

**Setup:**
- Build a top-up transaction where the output reserve token amount equals the input amount.
- Expect script validation failure.

### 20. Redemption — Corrupted Reserve Tree Value Format Fails

**What it verifies:** The reserve AVL tree value must be exactly 16 bytes: `timestamp (8 bytes) ++ redeemedAmount (8 bytes)`.

**Setup:**
- Construct an insert/update proof that writes a malformed value (e.g., 24 bytes or 8 bytes) into the reserve tree.
- Expect script validation failure.

## Implementation Notes

- All proposed tests should reuse the existing helpers in `BasisTokenSpec` (`mkBasisTokenInput`, `mkTrackerDataInput`, `mkTreeAndProof`, `mkTrackerTreeAndProof`, `mkKey`, `mkMessage`, `mkSigBytes`, `createOut`, `assertTxFails`, `assertTxSucceeds`).
- For emergency tests, set `creationHeightOffset = Some(3 * 720 + 1)` on `mkTrackerDataInput` and pass `trackerSigBytes = Array.emptyByteArray`.
- For boundary tests, pay close attention to the difference between `ctx.getHeight` and the tracker's `creationInfo._1`.
- Tests #5, #6, #9-#12 are specific to the token-backed contract and have no direct analogue in `BasisSpec`.

## Suggested Priority

**High priority:** #1, #2, #3, #4 (emergency redemption is a core protocol feature not yet tested for `basis-token.es`).
**Medium priority:** #5, #6, #8, #9, #10, #11, #12, #13, #14 (token-specific preservation rules).
**Low priority:** #7, #15, #16, #17, #18, #19, #20 (boundary and edge cases).
