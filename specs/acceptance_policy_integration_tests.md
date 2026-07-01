# Acceptance Policy Integration Tests

This document describes the integration tests for the acceptance policy feature.

## Test Coverage

### 1. `check_acceptance` Endpoint Tests (4 tests)

| Test | Description |
|------|-------------|
| `test_check_acceptance_without_policy` | Tests default reject behavior when no policy is configured |
| `test_check_acceptance_whitelist` | Tests whitelist policy - accepts whitelisted pubkeys, rejects others |
| `test_check_acceptance_invalid_pubkey` | Tests validation: rejects invalid hex and wrong-length pubkeys with 400 |
| `test_check_acceptance_with_max_debt` | Tests whitelist with max_debt limit - accepts under limit, rejects over |

### 2. `upload_policy` Endpoint Tests (7 tests)

| Test | Description |
|------|-------------|
| `test_upload_policy_invalid_hex_pubkey` | Tests 400 Bad Request for invalid hex in recipient_pubkey |
| `test_upload_policy_wrong_length_pubkey` | Tests 400 Bad Request for wrong-length recipient_pubkey |
| `test_upload_policy_invalid_signature_hex` | Tests 400 Bad Request for invalid hex in signature |
| `test_upload_policy_wrong_signature_length` | Tests 400 Bad Request for signature not equal to 65 bytes |
| `test_upload_policy_invalid_json` | Tests 400 Bad Request for invalid policy JSON structure |
| `test_upload_policy_invalid_signature` | Tests 401 Unauthorized for all-zero signature (fails verification) |
| `test_upload_and_retrieve_policy_roundtrip` | Tests full roundtrip: generate keypair, sign policy, upload, retrieve |

### 3. `get_policy_by_recipient` Endpoint Tests (2 tests)

| Test | Description |
|------|-------------|
| `test_get_policy_not_found` | Tests 404 Not Found when no policy exists for recipient |
| `test_get_policy_invalid_pubkey` | Tests 400 Bad Request for invalid hex and wrong-length pubkeys |

### 4. Per-Recipient Policy Integration Tests (2 tests)

| Test | Description |
|------|-------------|
| `test_check_acceptance_uses_per_recipient_policy` | Tests that check_acceptance uses per-recipient policy from DB when available |
| `test_check_acceptance_fallback_to_global_policy` | Tests fallback to global policy when no per-recipient policy exists |

## Running the Tests

```bash
# Run all acceptance API integration tests
cargo test -p basis_server --test acceptance_api_integration_tests

# Run specific test
cargo test -p basis_server --test acceptance_api_integration_tests test_upload_and_retrieve_policy_roundtrip

# Run all tests across all crates
cargo test
```

## Test Architecture

### Helper Functions

Three helper functions create test apps with different route configurations:

1. `create_test_app()` - Only mounts `/acceptance/check` endpoint
2. `create_test_app_with_policy_routes()` - Mounts `/acceptance/policy` (POST) and `/acceptance/policy/{pubkey}` (GET)
3. `create_test_app_with_all_routes()` - Mounts all three endpoints for integration testing

### Signature Generation

The `sign_policy_with_key()` helper generates valid Schnorr signatures using the `basis_core::impls::SchnorrVerifier` implementation. It:
1. Generates a secp256k1 keypair
2. Signs the policy JSON with the secret key
3. Verifies the signature locally before returning

### Policy JSON Format

Policies must use the correct JSON format with `type` fields:
```json
{
  "default": "reject",
  "root": "require_full_collateral",
  "predicates": [
    {
      "name": "require_full_collateral",
      "type": "collateralization",
      "min_ratio": 1.0
    }
  ]
}
```

## Test Results

All 15 tests pass successfully:
- 4 `check_acceptance` tests
- 7 `upload_policy` tests
- 2 `get_policy_by_recipient` tests
- 2 per-recipient integration tests
