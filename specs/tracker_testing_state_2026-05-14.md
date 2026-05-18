# Basis Tracker Testing State - May 14, 2026

## Executive Summary

The Basis Tracker project is in active development with recent focus on reserve box scanning fixes and redemption transaction testing. The codebase has uncommitted changes affecting 16 files across multiple crates. Currently, the project has compilation errors in tests that need to be resolved before testing can proceed.

## Current System Status

### Infrastructure Status
- **Ergo Node**: Not responding (localhost:9053 down)
- **Basis Tracker Server**: Not running (binary doesn't exist at `target/release/basis_server`)
- **Last Known Scanner Activity**: May 14, 2026 10:13:34 UTC
  - Successfully parsed 2 of 3 reserve boxes from mainnet scan
  - One box failed parsing due to empty R6 register (`0e00`)
  - Race condition observed: box `b9688b07` parsed then immediately removed as "not found in current scan"

### Build Status
- **Library compilation**: Success (`cargo check` passes)
- **Test compilation**: **FAILING** - 6 errors in `basis_store` lib tests
- **Error type**: Missing field `issuer_pubkey` in `RedemptionTransactionData` struct initializers

## Recent Development Activity

### Last 20 Commits (Git History)
```
682a7ab tx builder fixes, demo
72013f6 as_millis fix in avl_tree.rs , docs update
66dd154 fix towards updated contract
4551ced tests fix
b73a5cb updated basis.es and offchain
04c6760 p2s update
3ad609b contract & specs update
9416477 basis.es update
ab62507 totalDebt
b1b78a1 basis.es cleanup
35a3292 trackerIdCorrect check fix, more comments in contract
ecd4db3 clearing unused and duplicate code around redemption
529995a tracker NFT in r6
0667b69 r6 spec updated
6651029 generate-keypair restored
44dc019 updating specs to account for R6 in reserve boxes
a325f0c starting redemption testing spec & test
0685c99 redemption_cli_spec.md, first iteration
f73d5d4 latest-box-id API method
f744d63 Merge branch 'master' of github.com:BetterMoneyLabs/basis-tracker
```

### Uncommitted Changes (16 files modified)

#### Configuration Files
- `config/basis.toml` - Tracker configuration
- `config/ergo_nodes.toml` - Node URLs and contract P2S address updated

#### Core Crates
- `crates/basis_offchain/src/transaction_builder.rs` - Offchain transaction building
- `crates/basis_store/src/contract_compiler.rs` - Contract compilation
- `crates/basis_store/src/ergo_scanner.rs` - **Scanner fixes for R6 register parsing**
- `crates/basis_store/src/redemption.rs` - Redemption logic
- `crates/basis_store/src/transaction_builder.rs` - **Added `issuer_pubkey` field to `RedemptionTransactionData`**

#### Server Crate
- `crates/basis_server/src/api.rs` - API endpoints (172 lines added)
- `crates/basis_server/src/config.rs` - Configuration handling
- `crates/basis_server/src/create_reserve_tests.rs` - Reserve creation tests
- `crates/basis_server/src/main.rs` - Server startup
- `crates/basis_server/src/models.rs` - Data models

#### Integration Tests
- `crates/basis_server/tests/cors_tests.rs` - CORS tests
- `crates/basis_server/tests/http_api_integration_tests.rs` - HTTP API tests
- `crates/basis_store/src/real_scanner_integration_tests.rs` - Real scanner tests

#### Demo
- `demo/config.toml` - Demo configuration

### New Untracked Files
- `crates/basis_store/src/bin/` - New binary directory

## Known Issues

### 1. Test Compilation Errors (Blocking)
**Status**: Critical - prevents all testing
**Location**: `crates/basis_store/src/transaction_builder.rs`
**Issue**: The `RedemptionTransactionData` struct was updated to include `issuer_pubkey` field, but 6 test files still construct the struct without this field.
**Error**:
```
error[E0063]: missing field `issuer_pubkey` in initializer of `transaction_builder::RedemptionTransactionData`
```
**Fix needed**: Update all test instantiations of `RedemptionTransactionData` to include the new `issuer_pubkey` field.

### 2. Ergo Node Unavailable
**Status**: Blocking integration testing
**Issue**: Local Ergo node at localhost:9053 is not running
**Impact**: Cannot perform integration tests requiring blockchain interaction
**Last transaction monitored**: `12675b653342f1c9b8007d73cf53afe6ce6e11a34938a31a6e7f7b117cb1d6b6`
  - Was in mempool with 0 confirmations
  - Now neither in mempool nor confirmed (possibly dropped)

### 3. Scanner Race Condition
**Status**: Investigating
**Evidence**: From logs:
```
Updated and persisted reserve: b9688b07bc9f894c1a717f25bd5be59db8ddb7c266a4cb250ddc670026addc02
...
Removing spent reserve: b9688b07... (not found in current scan)
```
**Issue**: Box was successfully parsed and persisted in one scan cycle, then immediately flagged as spent in the same cycle.

### 4. Empty R6 Register Handling
**Status**: By design (needs confirmation)
**Issue**: Box `146d8b8c5144770d9c2aca2b471fa81e7e574a5509195024456ff96232da5f62` has R6=`0e00` (empty)
**Current behavior**: Scanner skips this box with warning: "Invalid tracker NFT ID length: expected 32 bytes, got 0"
**Question**: Should empty R6 be treated as valid (no tracker NFT) or always invalid?

## Testing Infrastructure

### Test Files Inventory
| Test File | Type | Status |
|-----------|------|--------|
| `crates/basis_server/tests/cors_tests.rs` | Integration | Needs compilation fix |
| `crates/basis_server/tests/http_api_integration_tests.rs` | Integration | Needs compilation fix |
| `crates/basis_server/tests/api_integration_tests.rs` | Integration | Unknown |
| `crates/basis_server/tests/tracker_box_updater_integration.rs` | Integration | Unknown |
| `crates/basis_server/tests/api_avl_integration_tests.rs` | Integration | Unknown |
| `crates/basis_server/tests/avl_tree_integration_tests.rs` | Integration | Unknown |
| `crates/basis_cli/tests/cli_integration_tests.rs` | Integration | Unknown |
| `crates/basis_cli/tests/test_404_issue.rs` | Integration | Unknown |
| `tests/end_to_end_flow.rs` | End-to-end | Unknown |
| `tests/tracker_box_updater_integration.rs` | Integration | Unknown |

### Specification Documents
- `specs/testing_redemption_transaction_spec.md` - Complete redemption testing workflow
- `Alice_Bob_Redemption_Test.md` - Alice/Bob test scenario
- `BUILD_AND_CREATE_RESERVE.md` - Reserve creation guide
- `BUILD_INSTALL.md` - Build and installation guide

## Configuration State

### Ergo Nodes Configured
```toml
[nodes]
niscoverednodes = [
    { url = "http://159.89.116.15:11088", description = "Public Ergo node" },
    { url = "http://213.239.193.208:9053", description = "Another public Ergo node" },
    { url = "http://localhost:9053", description = "Local Ergo node" },
]

[reserve_contract_p2s]
basis_reserve = "RtQxdWJ9axeb5Ltahqosnhj45BE26xuDK4YWddVj5p59t9RjKPEkkHCYEiyxwRFMJcEHwVd9syFod8ReQo1Zaz9eNTZ5JwDEN5hkLd67sVr2sNQ6R46TSfausAc9D3q7et1apYaXnqV9PkpHPMCA1zMCEsmmADj62XRGq4Cw2VwpuKKCAdreTgmLzdFWHGVGQMsPDFFBkRibsPFMzXkytdy2mPs2zCtm15uyDpd3jDLBy95BtUFXU2DdaYa1xMZE9UXju4R4MhWH8vqWda5BgpRTa1RpQxpS5b96FG46r1v3ZWCLYcVo51J1ekY8cqqVFNNykpQScRRYqFjCLMjG26dYEwZyn21wGeLJ7RzcTwCpvGDBa2w1P3ycAEJAv9XDPEtJrSQpkvBaD1HaZ6X2JuXmFjPF5MChmVLk4CTXtRQVRis7vP95ByTTmbHbtVdao32kbN3xhCWgJZZdaKkNyKH4vFQn5jyoEmiV7FjQDegWnnaFXu5FW6stx9cbhsxWz5FfGpW1BCMRNNJTCRF6FtYoehrMT74LDRNxHQ38EmMn6mBEpSrhkzDj2jysdFJvDUf8UQjLZQLmUQtgNotfxeAPxiavsT5mLUja3hdWvZPv71FcHxvP53WJHAcn9JPek3vepbH9gxRdmBMW"
```

## Next Steps Required

### Immediate (Blocking)
1. **Fix test compilation errors** - Add `issuer_pubkey` field to all `RedemptionTransactionData` struct initializations in test files
2. **Verify build passes** - Run `cargo test --lib` to confirm all tests compile
3. **Run unit tests** - Execute `cargo test` to verify baseline functionality

### Short-term
4. **Start local Ergo node** - Required for integration testing
5. **Rebuild server binary** - `cargo build --release -p basis_server`
6. **Start tracker server** - `./run_server.sh`
7. **Verify scanner functionality** - Confirm reserve boxes are correctly parsed and persisted

### Medium-term
8. **Investigate race condition** - Why is box `b9688b07` parsed then immediately removed?
9. **Resolve empty R6 handling** - Determine if empty R6 should be valid or always rejected
10. **Complete redemption transaction testing** - Follow `specs/testing_redemption_transaction_spec.md`
11. **Run full integration test suite** - All tests in `crates/basis_server/tests/`

## Test Data

### Alice (Issuer) Test Keys
- Secret: `9864a747e1b97f13e1a3ad0d3fbdc0ff350f51e49191cf47783c5e1fe77dae39`
- Public: `027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087`

### Bob (Recipient) Test Keys
- Secret: `a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2`
- Public: `03c5424252a1a1e4c2f5d6e7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8`

### Reserve Boxes from Last Scan
| Box ID | Value (nanoERG) | Height | Status |
|--------|----------------|--------|--------|
| `7d97c01c9a807d02d0e628128132b24fb72004aaf78eed8333a6e1bd00c95039` | 50,000,000 | 1750616 | Active |
| `b9688b07bc9f894c1a717f25bd5be59db8ddb7c266a4cb250ddc670026addc02` | 100,000,000 | 1778741 | Removed (race condition) |
| `146d8b8c5144770d9c2aca2b471fa81e7e574a5509195024456ff96232da5f62` | 100,000,000 | 1779383 | Skipped (empty R6) |

## Appendix: Commands for Recovery

```bash
# Fix compilation and run tests
cargo fix --lib -p basis_store
cargo test --lib

# Build server
cargo build --release -p basis_server

# Start server
./run_server.sh

# Check server status
./server_status.sh

# Run all tests
cargo test

# Run specific crate tests
cargo test -p basis_store
cargo test -p basis_server

# Check scanner logs
tail -f server.log | grep -E "(Found|Updated|Removing|WARN|ERROR)"

# Verify node connectivity
curl -s http://localhost:9053/info | python3 -m json.tool
```

---
*Document generated: May 14, 2026*
*Based on commit: 682a7ab (with uncommitted changes)*
