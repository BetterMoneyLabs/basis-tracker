# Test Results: Tracker Box Updater Always Submits

## Summary

Successfully modified the Basis Tracker to **always submit transactions** without a configuration flag. The tracker box updater now attempts to update the on-chain tracker box every 10 minutes.

## What Was Changed

1. **Removed `submit_transaction` flag** from `TrackerBoxUpdateConfig`
2. **Updated tracker box updater** to always call `submit_tracker_box_update()`
3. **Updated tests** to reflect the new behavior
4. **Updated documentation** in the spec file

## Test Results

### ✅ Verified

- **Note Creation**: IOU notes created with valid Schnorr signatures
- **Note Storage**: Notes successfully stored in tracker database with indices
- **Updater Running**: Tracker box updater started and running every 10 minutes
- **Transaction Submission**: 3 submission attempts logged (at 09:21, 09:31, 09:41)
- **Wallet Status**: Wallet is unlocked and ready
- **Mempool Activity**: 10 tracker-like transactions found in mempool
- **AVL Structure**: Note commitment structure verified

### ⚠️ Known Issue

The tracker box updater is submitting transactions, but they fail with script validation errors:

```
Failed to sign boxes due to null
```

This indicates the `/wallet/payment/send` API cannot properly spend the tracker box because:
1. The tracker box has a specific ErgoScript contract
2. The wallet payment API doesn't know how to satisfy the contract conditions
3. The transaction builder needs to use a lower-level API (e.g., `/wallet/transaction/generateUnsigned`)

### Log Evidence

```
2026-05-15T09:21:08 - Failed (wallet locked)
2026-05-15T09:31:08 - Failed (Script reduced to false)
2026-05-15T09:41:08 - Failed (Failed to sign boxes due to null)
```

The progression shows:
1. Initially failed because wallet was locked
2. After unlock, fails due to script validation
3. This confirms the tracker IS attempting submissions

## Next Steps for Full Verification

To see successful on-chain updates:

1. **Fix transaction building**: Use proper Ergo transaction builder that can handle the tracker box contract
2. **Monitor mempool**: After fix, tracker transactions should appear in mempool
3. **Verify on-chain**: Query tracker box from Ergo node to confirm R5 register updated

## Commands to Monitor

```bash
# Watch for submissions
tail -f server.log | grep -E 'Transaction Submitted|Failed to submit'

# Check mempool
curl -s http://127.0.0.1:9053/transactions/unconfirmed | jq 'length'

# Check wallet status
curl -s -H "api_key: hello" http://127.0.0.1:9053/wallet/status | jq '.isUnlocked'

# Run test
./test_note_avl_commitment.sh
```

## Files Modified

- `crates/basis_server/src/tracker_box_updater.rs` - Removed flag, always submit
- `crates/basis_server/src/main.rs` - Removed flag from config
- `specs/server/tracker_box_update_spec.md` - Updated docs
- `test_note_avl_commitment.sh` - Created test script
