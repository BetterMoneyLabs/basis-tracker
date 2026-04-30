# Basis Tracker - Remaining Enhancements Specification

**Document Version:** 1.0  
**Date:** 2026-02-28  
**Status:** Pending Implementation  
**Priority:** LOW (Enhancements - core protocol is complete)

---

## Overview

This document specifies the remaining enhancements for the Basis Tracker system. All HIGH and MEDIUM priority issues from the original audit have been completed. The remaining items are LOW priority enhancements that do not block production deployment.

### Completion Status

| Category | Total | Completed | Remaining |
|----------|-------|-----------|-----------|
| HIGH Priority | 11 | 11 | 0 ✅ |
| MEDIUM Priority | 8 | 8 | 0 ✅ |
| LOW Priority | 5 | 3 | 2 |
| **TOTAL** | **24** | **22** | **2** |

---

## Enhancement 1: Debt Transfer (Novation)

**Priority:** LOW  
**Type:** Feature Implementation  
**Estimated Effort:** 2-3 days  
**Status:** Not Started

### 1.1 Description

Implement debt transfer/novation functionality to enable triangular trade in the Basis network. This allows debt obligations to be transferred between creditors with debtor consent, reducing the need for on-chain redemptions.

### 1.2 Use Case Example

**Scenario:** A owes 10 ERG to B. B wants to buy 5 ERG worth of services from C.

**Without Debt Transfer:**
1. B redeems 5 ERG from A's reserve (on-chain transaction)
2. B pays C with 5 ERG
3. A's reserve collateral decreases

**With Debt Transfer:**
1. B requests transfer: decrease debt(A→B) by 5 ERG, increase debt(A→C) by 5 ERG
2. A verifies and signs the transfer
3. Tracker updates both records atomically (off-chain)
4. Result: B effectively paid C using A's debt obligation

### 1.3 Specification

#### 1.3.1 Message Format

**Transfer Authorization Message** (signed by debtor A):
```
message = hash(A||B) || hash(A||C) || transferAmount
```

Where:
- `hash(A||B)` = blake2b256(issuer_pubkey_A || receiver_pubkey_B) - 32 bytes
- `hash(A||C)` = blake2b256(issuer_pubkey_A || receiver_pubkey_C) - 32 bytes  
- `transferAmount` = 8-byte big-endian unsigned long

**Total message size:** 72 bytes

#### 1.3.2 Tracker Operations

The tracker must perform the following atomically:

1. **Verify source debt exists:**
   - Lookup debt(A→B) in AVL tree
   - Verify debt(A→B) >= transferAmount

2. **Verify debtor signature:**
   - Verify signature from A on transfer message

3. **Update AVL tree atomically:**
   - debt(A→B) = debt(A→B) - transferAmount
   - debt(A→C) = debt(A→C) + transferAmount

4. **Commit new state:**
   - Update AVL tree root digest in R5 register

#### 1.3.3 API Endpoints

**POST /debt/transfer**

Request:
```json
{
  "debtor_pubkey": "hex_encoded_33_byte_pubkey",
  "current_creditor_pubkey": "hex_encoded_33_byte_pubkey",
  "new_creditor_pubkey": "hex_encoded_33_byte_pubkey",
  "transfer_amount": 5000000000,
  "debtor_signature": "hex_encoded_65_byte_signature"
}
```

Response (Success):
```json
{
  "success": true,
  "data": {
    "transfer_id": "transfer_abc123...",
    "debtor_pubkey": "hex...",
    "old_creditor_pubkey": "hex...",
    "new_creditor_pubkey": "hex...",
    "transfer_amount": 5000000000,
    "old_debt_remaining": 5000000000,
    "new_debt_total": 5000000000,
    "tracker_state_digest": "hex_encoded_33_byte_digest",
    "timestamp": 1234567890
  }
}
```

Response (Error):
```json
{
  "success": false,
  "error": {
    "code": "INSUFFICIENT_DEBT",
    "message": "Source debt (3 ERG) is less than transfer amount (5 ERG)"
  }
}
```

**Error Codes:**
- `INSUFFICIENT_DEBT` - Source debt less than transfer amount
- `INVALID_SIGNATURE` - Debtor signature verification failed
- `DEBT_RECORD_NOT_FOUND` - Source debt record doesn't exist
- `INVALID_AMOUNT` - Transfer amount is zero or negative
- `SELF_TRANSFER` - Current and new creditor are the same

### 1.4 Implementation Tasks

#### Phase 1: Core Logic (1 day)
- [ ] 1.1.1 Add `DebtTransferRequest` struct to `basis_store/src/lib.rs`
- [ ] 1.1.2 Add `DebtTransferResult` struct to `basis_store/src/lib.rs`
- [ ] 1.1.3 Implement `initiate_debt_transfer()` in `TrackerStateManager`
  - [ ] Verify source debt exists and is sufficient
  - [ ] Verify debtor signature on transfer message
  - [ ] Perform atomic AVL tree updates
  - [ ] Return transfer result with new state digest
- [ ] 1.1.4 Add unit tests for debt transfer logic

#### Phase 2: API Implementation (0.5 days)
- [ ] 1.2.1 Add `DebtTransferResponse` models to `basis_server/src/models.rs`
- [ ] 1.2.2 Implement `POST /debt/transfer` endpoint in `basis_server/src/api.rs`
  - [ ] Validate request parameters
  - [ ] Send `InitiateDebtTransfer` command to tracker thread
  - [ ] Handle response and return appropriate status codes
- [ ] 1.2.3 Add route to `basis_server/src/main.rs`
- [ ] 1.2.4 Add OpenAPI spec to `openapi.yaml`

#### Phase 3: Tracker Thread Integration (0.5 days)
- [ ] 1.3.1 Add `InitiateDebtTransfer` variant to `TrackerCommand` enum
- [ ] 1.3.2 Implement command handler in tracker thread (main.rs)
- [ ] 1.3.3 Add integration tests for end-to-end flow

#### Phase 4: CLI Support (0.5 days)
- [ ] 1.4.1 Add `debt transfer` command to `basis_cli/src/commands/debt.rs`
- [ ] 1.4.2 Implement debtor signing of transfer authorization
- [ ] 1.4.3 Submit transfer request to server
- [ ] 1.4.4 Display transfer result

#### Phase 5: Documentation (0.5 days)
- [ ] 1.5.1 Update `specs/spec.md` with debt transfer workflow
- [ ] 1.5.2 Add debt transfer section to `README.md`
- [ ] 1.5.3 Add API documentation to `OPENAPI.md`
- [ ] 1.5.4 Create debt transfer example in examples folder

### 1.5 Acceptance Criteria

- [ ] Debt transfer completes atomically (both updates or neither)
- [ ] Insufficient debt is properly rejected
- [ ] Invalid debtor signatures are properly rejected
- [ ] AVL tree state is correctly updated
- [ ] Tracker state digest changes after transfer
- [ ] API returns appropriate error codes
- [ ] CLI can initiate and complete debt transfer
- [ ] Unit tests cover all edge cases
- [ ] Integration tests verify end-to-end flow

### 1.6 Testing Requirements

**Unit Tests:**
- [ ] Transfer with sufficient debt succeeds
- [ ] Transfer with insufficient debt fails
- [ ] Transfer with invalid signature fails
- [ ] Transfer with zero amount fails
- [ ] Transfer to same creditor fails
- [ ] Transfer from non-existent debt fails

**Integration Tests:**
- [ ] Full debt transfer flow (API + tracker thread)
- [ ] Multiple sequential transfers
- [ ] Concurrent transfers from same debtor
- [ ] Transfer followed by redemption

---

## Enhancement 2: Emergency Redemption E2E Testing

**Priority:** LOW  
**Type:** Testing Gap  
**Estimated Effort:** 1 day  
**Status:** Not Started

### 2.1 Description

Comprehensive end-to-end testing for emergency redemption flow. Emergency redemption is available after 3 days (3×720 blocks) from tracker creation height if the tracker becomes unavailable.

### 2.2 Current Implementation Status

**Implemented:**
- ✅ Contract supports emergency redemption (3×720 blocks from tracker creation)
- ✅ Message format includes `|| 0L` suffix for emergency
- ✅ API endpoint `/tracker/signature` accepts `emergency` flag
- ✅ Transaction builder supports emergency flag

**Needs Testing:**
- Tracker creation height tracking
- Emergency eligibility verification
- Full emergency redemption flow
- Contract validation of emergency vs normal redemption

### 2.3 Emergency Redemption Flow

```
1. Tracker becomes unavailable (simulated in test)
2. Wait for 3 days worth of blocks (3 × 720 = 2160 blocks)
3. Redeemer initiates emergency redemption
4. System verifies: (current_height - tracker_creation_height) > 2160
5. Tracker signature verification is bypassed
6. Redemption proceeds with only reserve owner signature
7. AVL tree updated normally
```

### 2.4 Implementation Tasks

#### Phase 1: Test Infrastructure (0.5 days)
- [ ] 2.1.1 Create `crates/basis_store/src/emergency_redemption_tests.rs`
- [ ] 2.1.2 Add test helper functions:
  - [ ] `create_tracker_box_with_height(creation_height: u32)`
  - [ ] `simulate_block_height(current_height: u32)`
  - [ ] `create_emergency_redemption_transaction()`
- [ ] 2.1.3 Add test configuration for emergency mode

#### Phase 2: Unit Tests (0.25 days)
- [ ] 2.2.1 Test emergency redemption (same message format, tracker signature optional after 3 days)
- [ ] 2.2.2 Test emergency eligibility calculation
- [ ] 2.2.3 Test signature verification bypass logic

#### Phase 3: Integration Tests (0.25 days)
- [ ] 2.3.1 Test full emergency redemption flow
  - [ ] Create tracker box at height H
  - [ ] Simulate height H + 2161
  - [ ] Initiate emergency redemption
  - [ ] Verify redemption succeeds without tracker signature
- [ ] 2.3.2 Test premature emergency redemption fails
  - [ ] Create tracker box at height H
  - [ ] Simulate height H + 1000 (not enough time)
  - [ ] Attempt emergency redemption
  - [ ] Verify redemption fails
- [ ] 2.3.3 Test normal vs emergency redemption distinction

### 2.5 Test Cases

#### Test: Emergency Redemption After Timeout
```rust
#[test]
fn test_emergency_redemption_after_timeout() {
    // Setup
    let tracker_creation_height = 1000;
    let current_height = 3161; // 1000 + 2160 + 1 (more than 3 days)
    
    // Create debt note
    let note = create_test_note(1000000000); // 1 ERG debt
    
    // Initiate emergency redemption
    let redemption = initiate_emergency_redemption(
        &note,
        tracker_creation_height,
        current_height,
    );
    
    // Verify
    assert!(redemption.is_ok());
    assert!(redemption.unwrap().emergency_verified);
}
```

#### Test: Premature Emergency Redemption Fails
```rust
#[test]
fn test_premature_emergency_redemption_fails() {
    // Setup
    let tracker_creation_height = 1000;
    let current_height = 2000; // Only 1000 blocks, not 2160
    
    // Create debt note
    let note = create_test_note(1000000000);
    
    // Attempt emergency redemption
    let redemption = initiate_emergency_redemption(
        &note,
        tracker_creation_height,
        current_height,
    );
    
    // Verify
    assert!(redemption.is_err());
    assert_eq!(redemption.unwrap_err(), EmergencyRedemptionError::TimeoutNotElapsed);
}
```

### 2.6 Acceptance Criteria

- [ ] Emergency redemption succeeds after 2160 blocks
- [ ] Emergency redemption fails before 2160 blocks
- [ ] Emergency message format is correct (48 bytes with 0L suffix)
- [ ] Tracker signature verification is bypassed in emergency mode
- [ ] Reserve owner signature is still required
- [ ] AVL tree is updated correctly
- [ ] All test cases pass
- [ ] Code coverage > 90% for emergency redemption logic

---

## Implementation Priority

### Recommended Order

1. **Emergency Redemption E2E Testing** (1 day)
   - Lower risk
   - Validates existing emergency functionality
   - Improves test coverage

2. **Debt Transfer (Novation)** (2-3 days)
   - New feature implementation
   - Requires careful testing
   - Higher complexity

### Total Estimated Effort

- **Development:** 3-4 days
- **Testing:** 1-2 days
- **Documentation:** 0.5 days
- **Total:** 4.5-6.5 days

---

## Appendix A: Related Specifications

- `specs/spec.md` - Main Basis protocol specification
- `specs/server/redemption_transaction_format_spec.md` - Transaction format
- `specs/server/redemption_state_spec.md` - Redemption state machine
- `contract/basis.es` - ErgoScript contract

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| Novation | Legal term for transferring rights/obligations from one party to another |
| Debt Transfer | Process of transferring debt obligation from one creditor to another |
| Emergency Redemption | Redemption process available after tracker unavailability timeout |
| Triangular Trade | Trade involving three parties: A→B→C becomes A→C |
| AVL Tree | Authenticated data structure for efficient key-value lookups with proofs |

## Appendix C: Contact

For questions about this specification, please refer to the Basis Forum thread:
https://www.ergoforum.org/t/basis-a-foundational-on-chain-reserve-approach-to-support-a-variety-of-offchain-protocols/5153
