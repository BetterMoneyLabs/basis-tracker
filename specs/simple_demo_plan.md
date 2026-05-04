# Simple Demo Plan: Scala → Rust Basis Tracker

## Overview

The Scala demo (`scala/demo/`) demonstrates three core Basis protocol operations:
1. **Reserve Deployment** - Creating an ERG-backed reserve box on-chain
2. **Note Creation** - Issuing IOU notes with dual signatures (payer + tracker)
3. **Note Redemption** - Redeeming notes against reserve collateral

The Rust Basis tracker already implements most of the underlying functionality. This plan identifies what exists, what needs adaptation, and how to wire it together into a cohesive demo flow matching the Scala simplicity.

---

## Architecture Comparison

| Component | Scala Demo | Rust Basis Tracker | Gap |
|-----------|-----------|-------------------|-----|
| **Cryptography** | `SigUtils.scala` (Schnorr) | `basis_core::impls::SchnorrVerifier` | ✅ Equivalent |
| **Key Management** | `ParticipantKeys` (CSV, hardcoded Alice/Bob/Tracker) | CLI accounts, no predefined demo keys | ⚠️ Need demo key setup |
| **IOU Note Creation** | `BasisNoteCreator.scala` | Server `POST /notes` + CLI `basis-cli note create` | ✅ Exists, needs demo script |
| **AVL Tree Ops** | `PlasmaMap` (Lithos) | `ergo_avltree_rust` via `basis_trees` | ✅ Equivalent |
| **Redemption Builder** | `BasisNoteRedeemer.scala` | `RedemptionManager` + `TransactionBuilder` | ✅ Exists, needs demo wiring |
| **Reserve Deployer** | `BasisDeployer.scala` | Partial (`create_reserve` API exists) | ⚠️ Needs completion |
| **Tracker Box Setup** | `TrackerBoxSetup.scala` | `TrackerStateManager` + `tracker_box_updater` | ⚠️ Needs initial setup flow |
| **Demo Orchestration** | SBT `runMain` scripts | CLI + HTTP API | ⚠️ Need demo runner |

---

## Implementation Plan

### Phase 1: Demo Key Infrastructure

**Goal**: Establish predefined demo participants (Alice, Bob, Tracker) matching Scala demo.

#### 1.1 Create Demo Keys Module
- **File**: `crates/basis_cli/src/demo_keys.rs`
- **Purpose**: Hardcoded demo keypairs (Alice, Bob, Tracker) with known secrets
- **Contents**:
  ```rust
  pub struct DemoParticipant {
      pub name: &'static str,
      pub secret_key: [u8; 32],
      pub public_key: [u8; 33],
      pub address: String,
  }
  
  pub fn alice() -> DemoParticipant { ... }
  pub fn bob() -> DemoParticipant { ... }
  pub fn tracker() -> DemoParticipant { ... }
  ```
- **Key generation**: Use same secp256k1 secrets as Scala demo for cross-compatibility

#### 1.2 Demo Configuration File
- **File**: `demo/config.toml`
- **Contents**:
  ```toml
  [participants]
  alice_secret = "<hex>"
  bob_secret = "<hex>"
  tracker_secret = "<hex>"
  
  [ergo_node]
  url = "http://localhost:9053"
  api_key = "hello"
  network = "mainnet"
  
  [demo]
  default_debt_amount = 50000000  # 0.05 ERG in nanoERG
  fee_box_value = 250000
  fee_box_count = 4
  ```

---

### Phase 2: Reserve Deployment

**Goal**: Enable creating a reserve box on-chain, matching `BasisDeployer.scala`.

#### 2.1 Complete Reserve Creation API
- **Current State**: `POST /reserves/create` exists but may be incomplete
- **Required**: Build transaction JSON for reserve box creation
- **File**: `crates/basis_server/src/reserve_api.rs` - enhance `create_reserve`
- **Output format** (matching Scala `BasisDeployer`):
  ```json
  {
    "tx": {
      "inputs": [{"boxId": "<funding_box>", "extension": {}}],
      "dataInputs": [],
      "outputs": [{
        "ergoTree": "<basis_contract_hex>",
        "value": 100000000,
        "assets": [{"tokenId": "<reserve_nft>", "amount": 1}],
        "additionalRegisters": {
          "R4": "<owner_pubkey>",
          "R5": "<empty_avl_tree>",
          "R6": "0e20<tracker_nft_id>"
        }
      }]
    }
  }
  ```

#### 2.2 Reserve Deployer CLI Command
- **File**: `crates/basis_cli/src/commands/reserve.rs` - add `deploy` subcommand
- **Usage**: `basis-cli reserve deploy --owner alice --amount 100000000`
- **Flow**:
  1. Load owner key from demo config
  2. Generate empty AVL tree (InsertOnly, keyLength=32)
  3. Build unsigned transaction
  4. Save to `demo/output/reserve_tx.json`

#### 2.3 Empty AVL Tree Generation
- **File**: `crates/basis_trees/src/lib.rs` - add `empty_tree()` constructor
- **Parameters**: InsertOnly flags, keyLength=32, valueLength=None
- **Must match**: Scala `Constants.chainCashPlasmaParameters`

---

### Phase 3: Note Creation Demo

**Goal**: Create IOU notes with both payer and tracker signatures, matching `BasisNoteCreator.scala`.

#### 3.1 Note Creator CLI Enhancement
- **Current State**: `basis-cli note create` exists
- **Enhancement**: Add `--demo` flag that:
  1. Uses Alice as payer, Bob as payee
  2. Includes tracker signature in output
  3. Outputs both JSON (stdout) and human-readable (stderr)

#### 3.2 Tracker Signature Endpoint
- **Current State**: `POST /tracker/signature` exists
- **Verify**: Returns 65-byte Schnorr signature on message
- **Message format**: `blake2b256(alice_pk || bob_pk) || totalDebt || timestamp`

#### 3.3 Demo Note Creation Script
- **File**: `demo/create_note.sh`
- **Flow**:
  ```bash
  # 1. Create note with Alice's signature
  NOTE=$(basis-cli note create --issuer alice --recipient bob --amount 50000000)
  
  # 2. Get tracker signature
  TRACKER_SIG=$(curl -X POST localhost:8080/tracker/signature -d "$NOTE")
  
  # 3. Combine and save
  echo "$NOTE" | jq '. + {trackerSignature: $TRACKER_SIG}' > demo/output/note.json
  ```

---

### Phase 4: Note Redemption Demo

**Goal**: Build and sign redemption transactions, matching `BasisNoteRedeemer.scala`.

#### 4.1 Redemption CLI Enhancement
- **Current State**: `RedemptionManager` + `/redeem` endpoint exist
- **Enhancement**: Add `basis-cli redeem` command that:
  1. Loads note from JSON file
  2. Verifies both signatures
  3. Generates AVL proofs (tracker lookup + reserve insert)
  4. Builds unsigned transaction
  5. Outputs `TransactionSigningRequest` JSON

#### 4.2 CLI Redeem Command
- **File**: `crates/basis_cli/src/commands/transaction.rs` - add `redeem` subcommand
- **Usage**:
  ```bash
  basis-cli redeem \
    --note-json demo/output/note.json \
    --reserve-box <id> \
    --tracker-box <id> \
    --fee-boxes <box1,box2,box3,box4> \
    --output demo/output/redeem_tx.json
  ```
- **Flow** (matching `BasisNoteRedeemer.redeem()`):
  1. Parse note JSON, extract keys/amounts/signatures
  2. Verify payer and tracker signatures
  3. Compute redemption message
  4. Generate reserve owner signature (re-sign with Alice's key)
  5. Generate tracker signature (re-sign with tracker key)
  6. Generate tracker AVL lookup proof (context var #8)
  7. Generate reserve AVL insert proof (context var #5)
  8. Build transaction JSON with all context variables
  9. Save to file

#### 4.3 Fee Box Discovery
- **Add**: `basis-cli reserve fee-boxes` command
- **Purpose**: Find unspent boxes with value=250000 nanoERG
- **Implementation**: Query `/wallet/boxes/unspent`, filter by value

---

### Phase 5: Demo Orchestration

**Goal**: Single script/command to run the entire demo flow end-to-end.

#### 5.1 Demo Runner Script
- **File**: `demo/run_demo.sh`
- **Flow**:
  ```bash
  #!/bin/bash
  set -e
  
  echo "=== Basis Simple Demo ==="
  echo ""
  
  # Step 1: Deploy Reserve
  echo "--- Step 1: Deploying Reserve ---"
  basis-cli reserve deploy --owner alice --amount 100000000
  RESERVE_BOX=$(jq -r '.boxId' demo/output/reserve_tx.json)
  echo "Reserve box: $RESERVE_BOX"
  echo ""
  
  # Step 2: Create IOU Note
  echo "--- Step 2: Creating IOU Note ---"
  basis-cli note create --demo --amount 50000000 > demo/output/note.json
  echo "Note created: demo/output/note.json"
  echo ""
  
  # Step 3: Redeem Note
  echo "--- Step 3: Redeeming Note ---"
  basis-cli redeem \
    --note-json demo/output/note.json \
    --reserve-box $RESERVE_BOX \
    --tracker-box auto \
    --fee-boxes auto \
    --output demo/output/redeem_tx.json
  echo "Redemption transaction: demo/output/redeem_tx.json"
  echo ""
  
  echo "=== Demo Complete ==="
  echo "Next: Sign and broadcast transaction with Ergo node"
  ```

#### 5.2 Demo README
- **File**: `demo/README.md`
- **Contents**: Step-by-step instructions, prerequisites, expected output

---

### Phase 6: Testing & Verification

**Goal**: Ensure demo works correctly and produces valid transactions.

#### 6.1 Unit Tests for Demo Components
- Test demo key generation matches Scala demo keys
- Test empty AVL tree serialization matches Scala
- Test note creation produces same message hash as Scala
- Test signature verification roundtrip

#### 6.2 Integration Test
- **File**: `tests/demo_integration.rs`
- **Flow**:
  1. Create reserve (mock Ergo node)
  2. Create note with Alice → Bob
  3. Redeem note
  4. Verify transaction structure and context variables

#### 6.3 Cross-Compatibility Test
- Create note with Scala `BasisNoteCreator`
- Verify with Rust `basis-cli note verify`
- Create note with Rust, verify with Scala `BasisNoteRedeemer`

---

## File Creation Summary

| File | Purpose | Priority |
|------|---------|----------|
| `crates/basis_cli/src/demo_keys.rs` | Demo participant keys | P0 |
| `demo/config.toml` | Demo configuration | P0 |
| `demo/run_demo.sh` | Demo orchestration script | P0 |
| `demo/README.md` | Demo documentation | P0 |
| `crates/basis_cli/src/commands/reserve.rs` (enhanced) | Reserve deploy command | P0 |
| `crates/basis_cli/src/commands/transaction.rs` (enhanced) | Redeem command | P0 |
| `crates/basis_trees/src/lib.rs` (enhanced) | Empty tree constructor | P1 |
| `crates/basis_server/src/reserve_api.rs` (enhanced) | Complete reserve creation | P1 |
| `tests/demo_integration.rs` | Demo integration tests | P2 |

---

## Execution Order

1. **Phase 1** → Demo keys + config (foundation)
2. **Phase 2** → Reserve deployment (on-chain setup)
3. **Phase 3** → Note creation (off-chain payment)
4. **Phase 4** → Note redemption (on-chain payout)
5. **Phase 5** → Demo orchestration (user experience)
6. **Phase 6** → Testing & verification (correctness)

---

## Key Differences from Scala Demo

| Aspect | Scala Demo | Rust Basis Tracker |
|--------|-----------|-------------------|
| **Key Management** | Hardcoded in `ParticipantKeys` | CLI accounts + demo_keys module |
| **Transaction Building** | Direct Ergo library calls | Server API + CLI commands |
| **AVL Trees** | `PlasmaMap` (Lithos) | `ergo_avltree_rust` |
| **Demo Execution** | `sbt runMain` | `basis-cli` + shell scripts |
| **Output** | JSON to stdout | JSON files + CLI output |
| **Node Integration** | Direct HTTP in Scala | Server proxy + CLI HTTP client |

---

## Success Criteria

The demo is complete when:

1. ✅ Can deploy a reserve box with single CLI command
2. ✅ Can create an IOU note with payer + tracker signatures
3. ✅ Can build a redemption transaction with all context variables
4. ✅ Transaction can be signed and broadcast via Ergo node
5. ✅ All outputs match Scala demo format (cross-compatible)
6. ✅ Single `./demo/run_demo.sh` runs end-to-end
7. ✅ Documentation explains every step
