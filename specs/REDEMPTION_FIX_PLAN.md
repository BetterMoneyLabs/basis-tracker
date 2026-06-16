# Plan: Fix Rust Redemption Code to Match Scala Reference Demo

## Overview

This document outlines the plan to fix discrepancies between the Rust tracker server redemption code and the documented Scala reference demo / ErgoScript contract (`basis.es`).

## Phase 1: Fix Critical Cryptographic Discrepancies

### 1.1 Fix CLI `crypto.rs` to enforce `z.bitLength <= 255`
**Problem**: `crates/basis_cli/src/crypto.rs` (lines 77-80) only checks `z == 0 || z >= n`, missing the Scala `z.bitLength > 255` constraint. CLI-generated signatures may be rejected by the ErgoScript contract.

**Fix**: Add the bitLength check with retry loop, matching `basis_core/src/impls.rs`:
```rust
let z_bit_length = z_big.bits();
if z_bit_length > 255 || z_big == 0 || z_big >= n {
    continue; // Retry with new nonce
}
```
Wrap the signing logic in a `loop { ... }` to retry automatically.

**Files**: `crates/basis_cli/src/crypto.rs`

### 1.2 Deduplicate Signing Code in `basis_core/src/impls.rs`
**Problem**: Two duplicate signing implementations (`SchnorrVerifier.sign_message` and standalone `schnorr_sign`).

**Fix**: Have `SchnorrVerifier.sign_message` delegate to `schnorr_sign` to eliminate code duplication and prevent future divergence.

**Files**: `crates/basis_core/src/impls.rs`

## Phase 2: Fix Redemption Manager Logic

### 2.1 Fix `initiate_redemption` time lock handling
**Problem**: `crates/basis_store/src/redemption.rs` (lines 134-146) enforces a 60-second time lock (`note.timestamp + 60_000`), but the contract handles time locks via tracker creation height, not the transaction builder.

**Fix**: Remove the time lock check from `initiate_redemption`. The contract already validates:
- Normal redemption: no time restriction (just valid signatures)
- Emergency redemption: `(HEIGHT - tracker_creation_height) > 2160`

**Files**: `crates/basis_store/src/redemption.rs`

### 2.2 Fix `build_unsigned_redemption_transaction` to use `amount` not `outstanding_debt`
**Problem**: `transaction_builder.rs` line 278 uses `note.outstanding_debt()` as redemption amount, ignoring `request.amount`. This prevents partial redemptions.

**Fix**: Use `request.amount` as the redemption amount, validating it against `note.outstanding_debt()`.

**Files**: `crates/basis_store/src/transaction_builder.rs`

## Phase 3: Fix Transaction Builder Hardcoded Values

### 3.1 Fix hardcoded reserve remaining value
**Problem**: `transaction_builder.rs` line 438 hardcodes `reserve_remaining = 49000000u64`.

**Fix**: Calculate from actual reserve value:
```rust
let reserve_remaining = reserve_value - tx_data.redemption_amount - tx_data.fee;
```
The reserve value must be fetched from the actual reserve box.

**Files**: `crates/basis_store/src/transaction_builder.rs`

### 3.2 Fix hardcoded reserve NFT ID
**Problem**: `transaction_builder.rs` line 435 hardcodes `reserve_nft_id`.

**Fix**: Use `tx_data.tracker_nft_id` (which comes from reserve box R6).

**Files**: `crates/basis_store/src/transaction_builder.rs`

## Phase 4: Fix Mock Contract Validator

### 4.1 Fix emergency logic in mock validator
**Problem**: `redemption_blockchain_tests.rs` conflates `emergency` flag with `enoughTimeSpent`.

**Fix**: Match `basis.es` exactly:
```rust
let tracker_sig_provided = tracker_signature.iter().any(|b| *b != 0);
let proper_tracker_signature = if tracker_sig_provided { verify } else { true };
let tracker_sig_valid = if tracker_sig_provided { 
    proper_tracker_signature 
} else { 
    blockchain.is_emergency_available() 
};
```

**Files**: `crates/basis_store/src/redemption_blockchain_tests.rs`

## Phase 5: Fix Server API Issues

### 5.1 Fix `/redemption/prepare` to use correct amount field
**Problem**: Need to verify the server uses `payload.amount` consistently in the 48-byte message.

**Fix**: Audit `crates/basis_server/src/api.rs` to ensure `payload.amount` is used, not `total_debt` from storage.

**Files**: `crates/basis_server/src/api.rs`

### 5.2 Fix tracker signature fallback to Ergo node
**Problem**: The server falls back to Ergo node's `/utils/schnorrSign` which may generate incompatible signatures.

**Fix**: Ensure `tracker_secret_key` is always configured for local signing, or improve compatibility check to reject incompatible signatures rather than just warn.

**Files**: `crates/basis_server/src/api.rs`

## Phase 6: Add Cross-Validation Tests

### 6.1 Add test verifying CLI signatures match `basis_core` signatures
Generate the same message with the same key in both CLI and `basis_core` and verify signatures are compatible.

**Files**: `crates/basis_cli/src/crypto.rs` (tests), `crates/basis_store/src/schnorr_tests.rs`

### 6.2 Add test using Scala demo keys
Use Alice/Bob/Tracker keys from `demo_keys.rs` to generate a note and redemption, verifying against the known Scala test vectors.

**Files**: `crates/basis_store/src/basis_spec_tests.rs`

## Implementation Order

1. **Phase 1.2** - Deduplicate signing code (simplest, prevents future divergence) ✅ DONE
2. **Phase 1.1** - Fix CLI crypto.rs (critical for signature compatibility) ✅ DONE
3. **Phase 2.1** - Remove incorrect time lock (simplifies redemption flow) ✅ DONE
4. **Phase 2.2** - Fix redemption amount handling (enables partial redemptions) ✅ DONE
5. **Phase 3** - Fix hardcoded values in transaction builder ✅ DONE
6. **Phase 4** - Fix mock validator emergency logic ✅ DONE
7. **Phase 5** - Fix server API issues ✅ DONE
8. **Phase 6** - Add cross-validation tests ✅ DONE

## References

- Scala test vectors: `crates/basis_store/src/schnorr_test_vectors.rs`
- Contract spec: `contract/basis.es`
- Schnorr spec: `specs/SCHNORR_SIGNATURE_SPEC.md`
- Scala demo keys: `crates/basis_cli/src/demo_keys.rs`
- Basis spec tests: `crates/basis_store/src/basis_spec_tests.rs`
