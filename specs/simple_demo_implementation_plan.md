# Simple Demo Implementation Plan: Scala → Rust Basis Tracker

**Date:** April 11, 2026  
**Status:** Ready for Implementation  
**Source:** Scala demo in `scala/demo/`  
**Target:** Rust Basis Tracker

---

## Executive Summary

The Scala demo provides a minimal working example of the Basis protocol with three operations:
1. **Note Creation** - Alice issues IOU to Bob with dual signatures (Alice + Tracker)
2. **Note Redemption** - Bob redeems IOU against Alice's reserve collateral
3. **Reserve Deployment** - Creating ERG-backed reserve box (supporting infrastructure)

The Rust Basis tracker already has **~90% of the required functionality** implemented. This plan outlines how to wire existing components together with minimal new code to replicate the Scala demo flow.

---

## Scala Demo Flow Analysis

### 1. Note Creation (`BasisNoteCreator.scala`)

**What it does:**
- Takes Alice (payer) and Bob (payee) keys
- Creates 48-byte message: `blake2b256(alice_pk || bob_pk) || totalDebt || timestamp`
- Alice signs message with her secret key → `(a, z)` signature
- Tracker signs same message with tracker secret key → `(a, z)` signature
- Outputs JSON note with both signatures

**Key code:**
```scala
val message = Blake2b256(payerKey ++ payeeKey) ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
val (a, z) = SigUtils.sign(message, payerSecret)        // Alice signs
val (a2, z2) = SigUtils.sign(message, trackerSecret)    // Tracker signs
```

**Output format (`note.json`):**
```json
{
  "payerKey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
  "payeeKey": "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
  "totalDebt": 50000000,
  "timestamp": 1743379200000,
  "payerSignature": {"a": "...", "z": "..."},
  "trackerSignature": {"a": "...", "z": "..."},
  "message": "6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a40000000002faf08000000194f8c88000"
}
```

### 2. Note Redemption (`BasisNoteRedeemer.scala`)

**What it does:**
- Loads note JSON and verifies both signatures
- Generates AVL proofs:
  - **Tracker lookup proof** (context var #8): proves debt exists in tracker tree
  - **Reserve insert proof** (context var #5): proves correct update to reserve tree
- Re-signs redemption message with Alice's and Tracker's keys
- Builds unsigned transaction JSON with all context variables

**Transaction structure:**
```
Inputs (5):
  - Reserve box (100M nanoERG) [with context extension]
  - 4x Fee boxes (250k nanoERG each) [empty extension]

Data Inputs (1):
  - Tracker box (holds AVL tree digest)

Outputs (3):
  - Updated reserve (50M nanoERG, new AVL tree)
  - Bob receiver (50M nanoERG)
  - Fee recipient (1M nanoERG)
```

**Context variables:**
- `#0`: action byte (0x00 = redeem)
- `#1`: receiver public key
- `#2`: reserve owner signature (65 bytes)
- `#3`: total debt amount
- `#4`: timestamp
- `#5`: reserve insert proof
- `#6`: tracker signature (65 bytes)
- `#8`: tracker lookup proof

### 3. Participant Keys

**Source:** `scala-utils/AddressUtils.scala` (ParticipantKeys object)  
**Loading:** CSV file `secrets/participants.local.csv` with format:
```csv
name,address,secret_hex
alice,<address>,<hex_secret>
bob,<address>,<hex_secret>
tracker,<address>,<hex_secret>
```

**Keys are derived from secrets:**
- Public keys derived from Ergo addresses via `AddressUtils.derivePublicKeyFromAddress()`
- Secrets loaded from CSV, verified to match addresses

---

## Rust Basis Tracker Component Map

### ✅ Already Implemented (No Changes Needed)

| Component | Location | Status | Notes |
|-----------|----------|--------|-------|
| **Schnorr Signing** | `basis_core::schnorr` | ✅ Complete | Matches Scala `SigUtils.sign/verify` |
| **Message Construction** | `basis_core::note` | ✅ Complete | 48-byte message format correct |
| **Signature Verification** | `basis_core::schnorr` | ✅ Complete | Verified against chaincash-rs |
| **AVL Tree Operations** | `basis_trees` | ✅ Complete | Using `ergo_avltree_rust` |
| **Reserve Insert Proof** | `basis_offchain::avl_proofs` | ✅ Complete | Matches Scala `generateReserveInsertProof` |
| **Tracker Lookup Proof** | `basis_offchain::avl_proofs` | ✅ Complete | Matches Scala `generateTrackerAvlProof` |
| **Transaction Builder** | `basis_offchain::tx_builder` | ✅ Complete | Builds Ergo transaction JSON |
| **Server API** | `basis_server` | ✅ Complete | `POST /notes`, `POST /redeem`, `POST /tracker/signature` |
| **Store/State** | `basis_store` | ✅ Complete | Tracks debt, reserves, AVL trees |
| **Scanner** | `basis_store::ergo_scanner` | ✅ Complete | Monitors blockchain state |

### ⚠️ Need Minor Enhancements

| Component | Location | Gap | Work Required |
|-----------|----------|-----|---------------|
| **CLI Note Creation** | `basis_cli` | No `--demo` mode | Add demo flag to use Alice/Bob keys |
| **CLI Redeem** | `basis_cli` | Command may not exist | Add `redeem` subcommand |
| **CLI Reserve Deploy** | `basis_cli` | May need completion | Add `deploy` subcommand |
| **Demo Keys Module** | Not present | No hardcoded keys | Create `demo_keys.rs` with test vectors |
| **Demo Config** | Not present | No config file | Create `demo/config.toml` |

### ❌ Need to Create

| Component | Location | Purpose |
|-----------|----------|---------|
| **Demo Orchestration** | `demo/run_demo.sh` | Single script to run full flow |
| **Demo Documentation** | `demo/README.md` | Step-by-step instructions |
| **Demo Output Dir** | `demo/output/` | Store generated files |

---

## Implementation Plan

### Phase 1: Demo Key Infrastructure (1-2 hours)

**Goal:** Establish hardcoded demo participants matching Scala demo.

#### 1.1 Create Demo Keys Module

**File:** `crates/basis_cli/src/demo_keys.rs`

**Content:**
```rust
//! Demo participant keys matching Scala demo.
//!
//! These keys are hardcoded for demonstration purposes and match
//! the Scala demo's ParticipantKeys for cross-compatibility testing.

use secp256k1::{SecretKey, PublicKey};

pub struct DemoParticipant {
    pub name: &'static str,
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
    pub address: String,
}

/// Alice - reserve owner (issuer of IOU notes)
pub fn alice() -> DemoParticipant {
    // From Scala demo: secrets/participants.local.csv
    let secret_hex = "..."; // Alice's secret from CSV
    let secret_key = SecretKey::from_slice(&hex::decode(secret_hex).unwrap()).unwrap();
    let public_key = secret_key.public_key(secp256k1::SECP256K1);
    
    DemoParticipant {
        name: "alice",
        secret_key,
        public_key,
        address: "...".to_string(), // Alice's Ergo address
    }
}

/// Bob - payee (recipient of IOU notes)
pub fn bob() -> DemoParticipant {
    let secret_hex = "..."; // Bob's secret from CSV
    let secret_key = SecretKey::from_slice(&hex::decode(secret_hex).unwrap()).unwrap();
    let public_key = secret_key.public_key(secp256k1::SECP256K1);
    
    DemoParticipant {
        name: "bob",
        secret_key,
        public_key,
        address: "...".to_string(),
    }
}

/// Tracker - off-chain debt tracker
pub fn tracker() -> DemoParticipant {
    let secret_hex = "..."; // Tracker's secret from CSV
    let secret_key = SecretKey::from_slice(&hex::decode(secret_hex).unwrap()).unwrap();
    let public_key = secret_key.public_key(secp256k1::SECP256K1);
    
    DemoParticipant {
        name: "tracker",
        secret_key,
        public_key,
        address: "...".to_string(),
    }
}
```

**Action:** Read actual secret values from `scala/secrets/participants.csv` or `scala/secrets/participants.local.csv`

#### 1.2 Create Demo Configuration

**File:** `demo/config.toml`

```toml
# Basis Simple Demo Configuration

[participants]
# Use "alice", "bob", "tracker" for demo mode
# Or provide custom keys
mode = "demo"  # "demo" | "custom"

[participants.demo_keys]
alice_secret = "<from_scala_csv>"
bob_secret = "<from_scala_csv>"
tracker_secret = "<from_scala_csv>"

[ergo_node]
url = "http://localhost:9053"
api_key = "hello"
network = "mainnet"

[server]
url = "http://localhost:8080"

[demo]
# Default debt amount: 0.05 ERG (50M nanoERG)
default_debt_amount = 50000000

# Fee configuration
fee_box_value = 250000
fee_box_count = 4

# Collateral
reserve_initial_collateral = 100000000  # 0.1 ERG

# Output directory
output_dir = "demo/output"
```

#### 1.3 Create Demo Output Directory

```bash
mkdir -p demo/output
```

---

### Phase 2: Note Creation Demo (2-3 hours)

**Goal:** Create IOU notes with dual signatures matching Scala output format.

#### 2.1 Enhance CLI Note Create Command

**File:** `crates/basis_cli/src/commands/note.rs`

**Add `--demo` flag:**
```rust
/// Create a new IOU note
#[derive(Parser)]
pub struct CreateNote {
    /// Use demo keys (Alice → Bob with Tracker signature)
    #[arg(long)]
    demo: bool,
    
    /// Issuer account name (if not using --demo)
    #[arg(long)]
    issuer: Option<String>,
    
    /// Recipient account name (if not using --demo)
    #[arg(long)]
    recipient: Option<String>,
    
    /// Debt amount in nanoERG
    #[arg(long)]
    amount: u64,
    
    /// Output file (default: stdout)
    #[arg(long)]
    output: Option<PathBuf>,
}
```

**Implementation flow:**
```rust
impl CreateNote {
    pub fn execute(&self) -> Result<()> {
        // 1. Get participant keys
        let (payer, payee) = if self.demo {
            (demo_keys::alice(), demo_keys::bob())
        } else {
            // Load from CLI accounts
            (load_account(&self.issuer)?, load_account(&self.recipient)?)
        };
        
        // 2. Build message
        let message = build_iou_message(
            &payer.public_key.serialize(),
            &payee.public_key.serialize(),
            self.amount,
            timestamp_ms(),
        );
        
        // 3. Sign with payer's key
        let payer_sig = schnorr_sign(&message, &payer.secret_key);
        
        // 4. Request tracker signature from server
        let tracker_sig = request_tracker_signature(&message, &config.server.url).await?;
        
        // 5. Build note JSON
        let note = build_note_json(
            &payer, &payee, self.amount, &payer_sig, &tracker_sig, &message
        );
        
        // 6. Output
        if let Some(path) = &self.output {
            fs::write(path, &note)?;
            eprintln!("Note saved to: {}", path.display());
        } else {
            println!("{}", note);  // stdout
        }
        
        // Human-readable to stderr
        eprintln!("=== IOU Note Created ===");
        eprintln!("Payer: {} -> Payee: {}", payer.name, payee.name);
        eprintln!("Amount: {} nanoERG", self.amount);
        eprintln!("Message: {}", hex::encode(&message));
        
        Ok(())
    }
}
```

#### 2.2 Verify Tracker Signature Endpoint

**File:** `crates/basis_server/src/handlers.rs`

**Verify endpoint exists:**
```rust
/// Request tracker signature on a message
async fn tracker_signature(
    State(state): State<AppState>,
    Json(req): Json<SignatureRequest>,
) -> Result<Json<SignatureResponse>> {
    // Sign message with tracker's secret key
    let sig = state.tracker.sign(&req.message)?;
    
    Ok(Json(SignatureResponse {
        signature_a: hex::encode(&sig.a),
        signature_z: hex::encode(&sig.z),
    }))
}
```

**Request format:**
```json
{
  "message": "<hex_encoded_48_byte_message>"
}
```

**Response format:**
```json
{
  "signature_a": "<33_byte_hex>",
  "signature_z": "<32_byte_hex>"
}
```

---

### Phase 3: Redemption Demo (3-4 hours)

**Goal:** Build redemption transactions with all context variables.

#### 3.1 Add CLI Redeem Command

**File:** `crates/basis_cli/src/commands/redeem.rs` (NEW)

```rust
/// Redeem an IOU note against reserve collateral
#[derive(Parser)]
pub struct RedeemNote {
    /// Path to note JSON file
    #[arg(long)]
    note_json: PathBuf,
    
    /// Reserve box ID (or "auto" to fetch from scanner)
    #[arg(long)]
    reserve_box: String,
    
    /// Tracker box ID (or "auto" to fetch from scanner)
    #[arg(long, default_value = "auto")]
    tracker_box: String,
    
    /// Fee box IDs (comma-separated, or "auto" to discover)
    #[arg(long)]
    fee_boxes: Option<String>,
    
    /// Output file (default: demo/output/redeem_tx.json)
    #[arg(long)]
    output: Option<PathBuf>,
}
```

**Implementation flow (matching `BasisNoteRedeemer.redeem()`):**
```rust
impl RedeemNote {
    pub fn execute(&self) -> Result<()> {
        // 1. Load and parse note JSON
        let note: IouNote = serde_json::from_str(
            &fs::read_to_string(&self.note_json)?
        )?;
        
        eprintln!("=== Basis Note Redeemer ===");
        eprintln!("Payer:   {}...", note.payer_key);
        eprintln!("Payee:   {}...", note.payee_key);
        eprintln!("Amount:  {} nanoERG", note.total_debt);
        
        // 2. Verify note signatures
        let message = hex::decode(&note.message)?;
        let payer_valid = verify_schnorr(
            &note.payer_key, &note.payer_signature.a, &note.payer_signature.z, &message
        );
        let tracker_valid = verify_schnorr(
            &config.tracker_public_key, 
            &note.tracker_signature.a, &note.tracker_signature.z, 
            &message
        );
        
        if !payer_valid || !tracker_valid {
            bail!("Invalid signatures in note");
        }
        eprintln!("✓ Signatures verified");
        
        // 3. Fetch reserve and tracker boxes
        let reserve_box = fetch_box(&self.reserve_box).await?;
        let tracker_box = fetch_box(&self.tracker_box).await?;
        
        // 4. Generate AVL proofs
        let debt_key = blake2b256(&[note.payer_key, note.payee_key].concat());
        
        // Tracker lookup proof (context var #8)
        let tracker_tree = extract_avl_tree(&tracker_box.r5)?;
        let tracker_proof = generate_lookup_proof(&tracker_tree, &debt_key)?;
        
        // Reserve insert proof (context var #5)
        let reserve_tree = extract_avl_tree(&reserve_box.r5)?;
        let (reserve_proof, updated_tree) = generate_insert_proof(
            &reserve_tree, &debt_key, &note.total_debt.to_le_bytes()
        )?;
        
        eprintln!("✓ AVL proofs generated");
        
        // 5. Generate redemption signatures
        let reserve_sig = schnorr_sign(&message, &demo_keys::alice().secret_key);
        let tracker_sig = schnorr_sign(&message, &demo_keys::tracker().secret_key);
        
        // 6. Build context variables
        let context_vars = ContextVars {
            action: 0x00,                              // #0: redeem
            receiver: note.payee_key,                  // #1: Bob's pubkey
            reserve_signature: reserve_sig,            // #2: Alice's sig
            total_debt: note.total_debt,               // #3: debt amount
            timestamp: note.timestamp,                 // #4: timestamp
            reserve_insert_proof: reserve_proof,       // #5: reserve proof
            tracker_signature: tracker_sig,            // #6: tracker sig
            tracker_lookup_proof: tracker_proof,       // #8: tracker proof
        };
        
        // 7. Build transaction
        let tx = build_redemption_transaction(
            &reserve_box,
            &tracker_box,
            &self.fee_boxes,
            &context_vars,
            &updated_tree,
            &note,
        )?;
        
        // 8. Output
        let output_path = self.output
            .clone()
            .unwrap_or_else(|| PathBuf::from("demo/output/redeem_tx.json"));
        
        fs::write(&output_path, serde_json::to_string_pretty(&tx)?)?;
        eprintln!("✓ Transaction saved to: {}", output_path.display());
        eprintln!("\n=== Next Steps ===");
        eprintln!("1. Sign: curl -X POST http://localhost:9053/wallet/transaction/sign ...");
        eprintln!("2. Broadcast: curl -X POST http://localhost:9053/transactions ...");
        
        Ok(())
    }
}
```

#### 3.2 Fee Box Discovery

**Add to CLI:** `basis-cli reserve fee-boxes`

**File:** `crates/basis_cli/src/commands/reserve.rs`

```rust
/// Find fee boxes for redemption transactions
#[derive(Parser)]
pub struct FindFeeBoxes {
    /// Number of fee boxes needed (default: 4)
    #[arg(long, default_value = "4")]
    count: usize,
    
    /// Fee box value in nanoERG (default: 250000)
    #[arg(long, default_value = "250000")]
    value: u64,
}
```

**Implementation:**
```rust
impl FindFeeBoxes {
    pub async fn execute(&self) -> Result<()> {
        // Query node API for unspent boxes
        let boxes = fetch_unspent_boxes(&config.node_url).await?;
        
        // Filter by value and no assets
        let fee_boxes: Vec<_> = boxes
            .into_iter()
            .filter(|b| b.value == self.value && b.assets.is_empty())
            .take(self.count)
            .collect();
        
        if fee_boxes.len() < self.count {
            bail!(
                "Only found {} fee boxes, need {}",
                fee_boxes.len(),
                self.count
            );
        }
        
        // Output comma-separated box IDs
        let box_ids: Vec<_> = fee_boxes.iter().map(|b| &b.box_id).collect();
        println!("{}", box_ids.join(","));
        
        Ok(())
    }
}
```

---

### Phase 4: Reserve Deployment Demo (1-2 hours)

**Goal:** Deploy reserve box on-chain (supporting infrastructure).

**Note:** This is needed once to set up the reserve. Can reuse existing `POST /reserves/create` endpoint.

#### 4.1 Add CLI Reserve Deploy Command

**File:** `crates/basis_cli/src/commands/reserve.rs`

```rust
/// Deploy a new reserve box
#[derive(Parser)]
pub struct DeployReserve {
    /// Owner account name (default: alice in demo mode)
    #[arg(long)]
    owner: Option<String>,
    
    /// Initial collateral in nanoERG (default: 100000000 = 0.1 ERG)
    #[arg(long)]
    collateral: Option<u64>,
    
    /// Output file (default: demo/output/reserve_tx.json)
    #[arg(long)]
    output: Option<PathBuf>,
}
```

**Implementation:**
```rust
impl DeployReserve {
    pub async fn execute(&self) -> Result<()> {
        // 1. Get owner key
        let owner = if let Some(name) = &self.owner {
            load_account(name)?
        } else {
            demo_keys::alice()
        };
        
        // 2. Generate empty AVL tree
        let empty_tree = create_empty_avl_tree(
            AvlTreeFlags::INSERT_ONLY,
            32,  // key length
            None, // value length
        )?;
        
        // 3. Build deployment transaction
        let collateral = self.collateral.unwrap_or(100_000_000);
        let tx = build_reserve_deployment_tx(
            &owner.public_key,
            &empty_tree,
            collateral,
            &config.tracker_nft_id,
        )?;
        
        // 4. Output
        let output_path = self.output
            .clone()
            .unwrap_or_else(|| PathBuf::from("demo/output/reserve_tx.json"));
        
        fs::write(&output_path, serde_json::to_string_pretty(&tx)?)?;
        eprintln!("✓ Reserve deployment transaction saved to: {}", output_path.display());
        eprintln!("\n=== Next Steps ===");
        eprintln!("1. Sign: curl -X POST http://localhost:9053/wallet/transaction/sign ...");
        eprintln!("2. Broadcast: curl -X POST http://localhost:9053/transactions ...");
        eprintln!("3. Wait for confirmation, then reserve box is ready");
        
        Ok(())
    }
}
```

---

### Phase 5: Demo Orchestration (1 hour)

**Goal:** Single script to run entire demo flow.

#### 5.1 Create Demo Runner Script

**File:** `demo/run_demo.sh`

```bash
#!/bin/bash
set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_DIR="$DEMO_DIR/output"
CLI="cargo run -p basis_cli --"

mkdir -p "$OUTPUT_DIR"

echo "========================================"
echo "  Basis Simple Demo"
echo "========================================"
echo ""
echo "This demo shows:"
echo "  1. Alice issues IOU note to Bob"
echo "  2. Bob redeems IOU against Alice's reserve"
echo ""
echo "Prerequisites:"
echo "  - Ergo node running on localhost:9053"
echo "  - Basis server running on localhost:8080"
echo "  - Reserve box already deployed"
echo "  - Fee boxes available (4x 250000 nanoERG)"
echo ""
read -p "Press Enter to continue..."

# Step 1: Create IOU Note
echo ""
echo "--- Step 1: Creating IOU Note ---"
echo "Alice issues 0.05 ERG IOU to Bob with Tracker signature"
echo ""

$CLI note create \
  --demo \
  --amount 50000000 \
  --output "$OUTPUT_DIR/note.json"

echo ""
echo "Note created: $OUTPUT_DIR/note.json"
echo ""

# Step 2: Find Fee Boxes
echo ""
echo "--- Step 2: Finding Fee Boxes ---"
echo ""

FEE_BOXES=$($CLI reserve fee-boxes --count 4)
echo "Fee boxes: $FEE_BOXES"
echo ""

# Step 3: Build Redemption Transaction
echo ""
echo "--- Step 3: Building Redemption Transaction ---"
echo "Bob redeems IOU against Alice's reserve"
echo ""

# Auto-detect reserve and tracker boxes (or use hardcoded values)
RESERVE_BOX="${RESERVE_BOX_ID:-auto}"
TRACKER_BOX="${TRACKER_BOX_ID:-auto}"

$CLI redeem \
  --note-json "$OUTPUT_DIR/note.json" \
  --reserve-box "$RESERVE_BOX" \
  --tracker-box "$TRACKER_BOX" \
  --fee-boxes "$FEE_BOXES" \
  --output "$OUTPUT_DIR/redeem_tx.json"

echo ""
echo "========================================"
echo "  Demo Complete!"
echo "========================================"
echo ""
echo "Generated files:"
echo "  - $OUTPUT_DIR/note.json (IOU note)"
echo "  - $OUTPUT_DIR/redeem_tx.json (redemption transaction)"
echo ""
echo "Next steps:"
echo "  1. Sign transaction:"
echo "     curl -X POST http://localhost:9053/wallet/transaction/sign \\"
echo "       -H 'api_key: hello' \\"
echo "       -H 'Content-Type: application/json' \\"
echo "       -d @$OUTPUT_DIR/redeem_tx.json"
echo ""
echo "  2. Broadcast transaction:"
echo "     curl -X POST http://localhost:9053/transactions \\"
echo "       -H 'api_key: hello' \\"
echo "       -H 'Content-Type: application/json' \\"
echo "       -d '{\"tx\": <signed_tx>}'"
echo ""
```

Make executable:
```bash
chmod +x demo/run_demo.sh
```

#### 5.2 Create Demo README

**File:** `demo/README.md`

Comprehensive documentation covering:
- Architecture diagram
- Prerequisites
- Quick start guide
- Manual step-by-step instructions
- Transaction structure details
- Troubleshooting guide
- Cross-compatibility testing with Scala

---

## File Creation Summary

| File | Purpose | Priority | Effort |
|------|---------|----------|--------|
| `crates/basis_cli/src/demo_keys.rs` | Demo participant keys | P0 | 1h |
| `demo/config.toml` | Demo configuration | P0 | 0.5h |
| `crates/basis_cli/src/commands/note.rs` (enhanced) | Add `--demo` flag | P0 | 1.5h |
| `crates/basis_cli/src/commands/redeem.rs` | New redeem command | P0 | 3h |
| `crates/basis_cli/src/commands/reserve.rs` (enhanced) | Add `deploy` and `fee-boxes` | P0 | 1.5h |
| `demo/run_demo.sh` | Demo orchestration script | P0 | 1h |
| `demo/README.md` | Demo documentation | P1 | 1h |
| `demo/output/` | Output directory | P0 | 0.1h |
| Tests for demo components | Cross-compatibility tests | P2 | 2h |

**Total Effort:** ~11-12 hours

---

## Execution Order

1. **Phase 1** → Demo keys + config (foundation)
2. **Phase 2** → Note creation with dual signatures
3. **Phase 3** → Redemption with AVL proofs and context variables
4. **Phase 4** → Reserve deployment (one-time setup)
5. **Phase 5** → Demo orchestration (user experience)
6. **Phase 6** → Testing & cross-compatibility verification

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

1. ✅ Can create an IOU note with payer + tracker signatures matching Scala format
2. ✅ Can build a redemption transaction with all context variables
3. ✅ Transaction can be signed and broadcast via Ergo node
4. ✅ All outputs match Scala demo format (cross-compatible)
5. ✅ Single `./demo/run_demo.sh` runs end-to-end
6. ✅ Documentation explains every step
7. ✅ Cross-compatibility verified: Scala note ↔ Rust verification and vice versa

---

## Next Steps

1. Read actual secret values from `scala/secrets/participants.local.csv`
2. Verify existing CLI commands and identify exact gaps
3. Start implementation with Phase 1 (demo keys)
4. Test each component against Scala demo outputs
