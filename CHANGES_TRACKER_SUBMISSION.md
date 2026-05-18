# Changes Summary: Tracker Box Updater Always Submits Transactions

## Overview

The tracker box updater has been modified to **always submit transactions** to the Ergo blockchain. The `submit_transaction` configuration flag has been removed - there is no longer a "logging-only" mode.

## Changes Made

### 1. `crates/basis_server/src/tracker_box_updater.rs`

**Removed `submit_transaction` field from config:**
```rust
pub struct TrackerBoxUpdateConfig {
    pub update_interval_seconds: u64,
    pub enabled: bool,
    // REMOVED: pub submit_transaction: bool,
    pub ergo_node_url: String,
    pub ergo_api_key: Option<String>,
}
```

**Updated `Default` implementation:**
- Removed `submit_transaction: false` from default config

**Updated `start()` method:**
- Removed the `if config.submit_transaction` conditional
- Transaction submission now happens unconditionally on every updater cycle
- Error logging remains for failed submissions

**Updated test:**
- Removed assertion checking `!config.submit_transaction`

### 2. `crates/basis_server/src/main.rs`

**Updated config initialization:**
```rust
let tracker_box_config = TrackerBoxUpdateConfig {
    update_interval_seconds: 600, // 10 minutes
    enabled: true,
    // REMOVED: submit_transaction: config.tracker_public_key_bytes().ok().is_some(),
    ergo_node_url: config.ergo.node.node_url.clone(),
    ergo_api_key: config.ergo.node.api_key.clone(),
};
```

### 3. `specs/server/tracker_box_update_spec.md`

**Updated documentation:**
- Removed `submit_transaction` field from configuration struct documentation
- Updated description to reflect that transactions are always submitted

## Behavior

### Before
- Tracker box updater had two modes:
  - `submit_transaction: false` - Only logged R4/R5 values, no blockchain interaction
  - `submit_transaction: true` - Submitted actual transactions to Ergo node
- Default was `false` (logging-only mode)

### After
- Tracker box updater **always** attempts to submit transactions
- No configuration flag exists
- On each 10-minute cycle:
  1. Reads current AVL root digest from shared state
  2. Builds R4 (tracker pubkey) and R5 (AVL commitment) registers
  3. Calls Ergo node `/wallet/payment/send` API
  4. Logs success or failure

## Test Script

Created `test_note_avl_commitment.sh` that verifies:

1. **Note Creation** - Creates IOU note with demo keys using `basis_cli`
2. **Note Submission** - Posts note to tracker server via API
3. **Storage Verification** - Confirms note is stored and indexed
4. **Updater Verification** - Checks tracker box updater is running and submitting
5. **Transaction Monitoring** - Looks for submission attempts in logs
6. **Wallet Status** - Checks if Ergo node wallet is unlocked (required for success)
7. **Mempool Check** - Verifies mempool status
8. **On-Chain Box** - Validates tracker box format if available

## Wallet Requirement

**Important:** The Ergo node wallet must be **unlocked** for transaction submission to succeed.

If the wallet is locked, the tracker will log:
```
Failed to submit tracker box update transaction: ... wallet is locked
```

To unlock the wallet:
```bash
curl -X POST http://localhost:9053/wallet/unlock \
  -H "api_key: hello" \
  -d '{"pass": "YOUR_WALLET_PASSWORD"}'
```

## Verification

Run the test script:
```bash
./test_note_avl_commitment.sh
```

Monitor tracker submissions:
```bash
tail -f server.log | grep -E 'Transaction Submitted|Failed to submit'
```

## Files Modified

- `crates/basis_server/src/tracker_box_updater.rs`
- `crates/basis_server/src/main.rs`
- `specs/server/tracker_box_update_spec.md`

## Files Created

- `test_note_avl_commitment.sh` - Integration test script
