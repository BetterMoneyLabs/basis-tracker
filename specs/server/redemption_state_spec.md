# Redemption State Specification

## Overview
This document specifies the state management and process flow for redemption operations in the Basis Tracker system. Redemption allows holders of IOU notes to claim collateral from the issuer's reserve based on the outstanding debt represented by the note. The contract tracks cumulative redeemed amounts using AVL trees to prevent double redemptions.

## Redemption Data Structures

### RedemptionRequest
The redemption request contains the parameters needed to initiate a redemption:

```rust
pub struct RedemptionRequest {
    /// Issuer's public key (hex encoded, 33-byte compressed secp256k1)
    pub issuer_pubkey: String,
    /// Recipient's public key (hex encoded, 33-byte compressed secp256k1)
    pub recipient_pubkey: String,
    /// Amount to redeem (nanoERG)
    pub amount: u64,
    /// Timestamp of the note being redeemed (milliseconds since Unix epoch)
    pub timestamp: u64,
    /// Reserve contract box ID being spent
    pub reserve_box_id: String,
    /// Tracker commitment box ID used as data input
    pub tracker_box_id: String,
    /// Tracker NFT ID from the reserve box's R6 register (32 bytes hex)
    pub tracker_nft_id: String,
    /// Current blockchain height
    pub current_height: u64,
    /// Recipient's address for the redemption output
    pub recipient_address: String,
    /// Change address for transaction outputs
    pub change_address: String,
    /// Issuer's 65-byte Schnorr signature (130 hex chars)
    pub issuer_signature: String,
    /// Whether this is an emergency redemption
    pub emergency: bool,
    /// Optional tracker 65-byte Schnorr signature (server-generated if omitted)
    pub tracker_signature: Option<String>,
    /// Value of the reserve box being spent (nanoERG)
    pub reserve_box_value: u64,
    /// Optional wallet-owned fee input box IDs
    pub fee_input_box_ids: Vec<String>,
    /// Total value provided by the fee input boxes (must be >= fee)
    pub fee_input_total_value: u64,
    /// Refund initiation height from the reserve box's R7 register (0 if none)
    pub reserve_refund_initiation_height: u64,
}
```

### RedemptionData
The redemption data structure contains the complete information for a processed redemption:

```rust
pub struct RedemptionData {
    /// Unique redemption ID
    pub redemption_id: String,
    /// Total debt amount from tracker's AVL tree
    pub total_debt: u64,
    /// Already redeemed amount from reserve's AVL tree (0 for first redemption)
    pub already_redeemed: u64,
    /// Amount being redeemed in this transaction
    pub redeem_amount: u64,
    /// AVL proof for tracker tree lookup (context var #8)
    pub tracker_lookup_proof: Vec<u8>,
    /// AVL proof for reserve tree lookup (context var #7, optional)
    pub reserve_lookup_proof: Option<Vec<u8>>,
    /// AVL proof for reserve tree insert/update (context var #5)
    pub reserve_insert_proof: Vec<u8>,
    /// Reserve owner's signature (65-byte Schnorr signature)
    pub reserve_signature: Vec<u8>,
    /// Tracker's signature (65-byte Schnorr signature)
    pub tracker_signature: Vec<u8>,
    /// Tracker's public key
    pub tracker_pubkey: String,
    /// Tracker state digest (33-byte AVL tree root)
    pub tracker_state_digest: Vec<u8>,
    /// Reserve state digest (33-byte AVL tree root)
    pub reserve_state_digest: Vec<u8>,
    /// Current blockchain height
    pub block_height: u32,
    /// Whether this is first redemption (reserve_lookup_proof can be omitted)
    pub is_first_redemption: bool,
}
```

### RedemptionError
Possible error conditions during redemption:

```rust
pub enum RedemptionError {
    #[error("Note not found in tracker state")]
    NoteNotFound,
    #[error("Invalid reserve owner signature")]
    InvalidReserveSignature,
    #[error("Invalid tracker signature")]
    InvalidTrackerSignature,
    #[error("Emergency redemption not yet available: {0} blocks remaining", 3 * 720 - .0)]
    EmergencyRedemptionTooEarly(u32),
    #[error("Insufficient debt: trying to redeem {0} but only {1} available")]
    InsufficientDebt(u64, u64),
    #[error("Reserve not found: {0}")]
    ReserveNotFound(String),
    #[error("Tracker box not found or invalid")]
    TrackerBoxNotFound,
    #[error("Transaction building error: {0}")]
    TransactionError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("AVL proof generation failed: {0}")]
    AvlProofError(String),
    #[error("Tracker debt mismatch: expected {0}, found {1}")]
    TrackerDebtMismatch(u64, u64),
    #[error("Redemption amount must be positive")]
    InvalidRedemptionAmount,
}
```

## Redemption Process Flow

### 1. Initiate Redemption
The redemption process begins when a recipient initiates a redemption request:

1. **Validate Request Parameters**
   - Parse and validate issuer and recipient public keys
   - Verify the total_debt amount is positive
   - Check emergency flag

2. **Lookup Total Debt in Tracker State**
   - Compute key: `key = blake2b256(issuer_pubkey_bytes || recipient_pubkey_bytes)`
   - Query tracker's AVL tree for `totalDebt` using the key
   - Return `RedemptionError::NoteNotFound` if key not found

3. **Lookup Already Redeemed in Reserve State**
   - Query reserve's AVL tree for `alreadyRedeemed` using the same key
   - If key not found, this is first redemption (`alreadyRedeemed = 0`)

4. **Validate Redemption Amount**
   - Calculate available debt: `availableDebt = totalDebt - alreadyRedeemed`
   - Verify redemption amount <= availableDebt
   - Return `RedemptionError::InsufficientDebt` if exceeded

5. **Check Emergency Redemption Eligibility** (if emergency flag is set)
   - Get tracker box creation height from data input
   - Calculate blocks elapsed: `blocksElapsed = currentHeight - trackerCreationHeight`
   - Verify `blocksElapsed > 3 * 720` (3 days)
   - Return `RedemptionError::EmergencyRedemptionTooEarly` if not met

6. **Find Matching Reserve**
   - Look up reserve associated with the issuer
   - Use normalized public key matching (handle "07" prefix)
   - Return `RedemptionError::ReserveNotFound` if no matching reserve

7. **Verify Tracker Box**
   - Ensure tracker box exists and is valid
   - Verify tracker NFT ID matches reserve's R6
   - Return `RedemptionError::TrackerBoxNotFound` if invalid

8. **Generate AVL Proofs**
   - Generate tracker tree lookup proof (context var #8)
   - Generate reserve tree lookup proof (context var #7, omit for first redemption)
   - Generate reserve tree insert/update proof (context var #5)
   - Return `RedemptionError::AvlProofError` if proof generation fails

9. **Request Signatures**
    - Build signing message: `message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)` (48 bytes)
    - Emergency redemption uses the same message format; tracker signature verification is bypassed after 3 days
    - Request reserve owner's signature on message (generated locally by the redeemer's CLI using the issuer's secret key)
    - Request tracker's signature on message from the tracker server via its `/tracker/signature` API
    - The server signs locally if `tracker_secret_key` is configured, otherwise it delegates to the Ergo node's `/utils/schnorrSign` endpoint
    - Return `RedemptionError::InvalidReserveSignature` or `InvalidTrackerSignature` if invalid

10. **Build Transaction**
     - Construct a redemption transaction ready for the Ergo node's `/wallet/transaction/sign` endpoint
     - Include the reserve box as the first input with all context extension variables (#0-#8)
     - Include wallet-owned fee inputs with empty extensions
     - Include the tracker box as a data input
     - Provide `inputsRaw` (serialized bytes of all inputs) and `dataInputsRaw` (serialized tracker box)
     - Provide `secrets.dlog` with the recipient's private key so the node can satisfy the `proveDlog(receiver)` spend condition
     - Include the recipient output, updated reserve output, fee output, and optional change output
     - Return `RedemptionError::TransactionError` if building fails

11. **Return Redemption Data**
    - Generate unique redemption ID
    - Package all redemption information
    - Return success with `RedemptionData`

### 2. Complete Redemption
After the unsigned redemption transaction is built:

1. **Sign Transaction**
    - POST the unsigned transaction JSON to the Ergo node's `/wallet/transaction/sign` endpoint
    - The node uses `secrets.dlog` (recipient private key) to satisfy the `proveDlog(receiver)` condition
    - Obtain the signed transaction bytes from the response
    - Return `RedemptionError::TransactionError` if signing fails

2. **Broadcast Transaction**
    - POST the signed transaction to `/transactions` to broadcast it to the network
    - Return `RedemptionError::TransactionError` if broadcast fails

3. **Confirm on Chain**
    - Wait for the transaction to be included in a block
    - Update reserve's cumulative redeemed amount in local storage from blockchain events
    - Update note state if tracking separately
    - Store transaction ID for reference
    - Handle any reorganization scenarios

## State Transitions

### Tracker State (Off-chain)
```
[No Debt] -(Payment)-> [Debt Recorded] -(More Payments)-> [Increased Debt] -(Transfer)-> [Debt Reassigned]
```

- No Debt: No record in tracker's AVL tree for (issuer, recipient) pair
- Debt Recorded: `hash(issuerKey||recipientKey) -> totalDebt` inserted
- Increased Debt: Value updated with cumulative total
- Debt Reassigned: Debt transferred to new creditor (novation)

### Reserve State (On-chain AVL Tree in R5)
```
[Empty Tree] -(First Redemption)-> [Single Entry] -(More Redemptions)-> [Multiple Entries]
```

- Empty Tree: No redemptions yet (empty AVL tree)
- First Redemption: `hash(ownerKey||recipientKey) -> redeemedAmount` inserted
- More Redemptions: Values updated with cumulative redeemed amounts

### Redemption State (Per Note)
```
[Not Redeemed] -(Partial Redemption)-> [Partially Redeemed] -(Full Redemption)-> [Fully Redeemed]
```

- Not Redeemed: `alreadyRedeemed = 0`, `outstandingDebt = totalDebt`
- Partially Redeemed: `0 < alreadyRedeemed < totalDebt`, `outstandingDebt = totalDebt - alreadyRedeemed`
- Fully Redeemed: `alreadyRedeemed = totalDebt`, `outstandingDebt = 0`

## Validation Rules

### Pre-Redemption Validation
1. **Public Key Format**: Both issuer and recipient public keys must be valid hex-encoded 33-byte values
2. **Total Debt Bounds**: Total debt must be > 0 and match tracker's AVL tree value
3. **Redemption Amount**: Must be > 0 and <= (totalDebt - alreadyRedeemed)
4. **Emergency Time Lock**: For emergency redemption, current height must be > trackerCreationHeight + 3 * 720
5. **Reserve Existence**: A matching reserve must exist for the issuer
6. **Sufficient Collateral**: Reserve must have sufficient collateral to cover redemption
7. **Tracker Box Validity**: Tracker box must exist and NFT ID must match reserve's R6

### Post-Redemption Validation
1. **State Consistency**: Reserve's AVL tree must be properly updated
2. **Balance Integrity**: Total system balances must be preserved
3. **Signature Verification**: Both reserve owner and tracker signatures must be valid
4. **Blockchain Confirmation**: Redemption transaction must be confirmed on blockchain
5. **AVL Proof Validity**: All AVL proofs must verify against respective tree roots

## Error Handling

### Recovery Procedures
1. **Failed Redemption**: If redemption fails, roll back any partial state changes
2. **Incomplete Transaction**: If blockchain transaction fails, restore previous state
3. **Signature Mismatch**: If signatures don't match, reject redemption and log incident
4. **Double Spend Prevention**: AVL tree in reserve contract prevents multiple redemptions of same debt
5. **Tracker Unavailable**: Emergency redemption available after 3 days

### Logging Requirements
1. **Redemption Attempts**: Log all redemption attempts with success/failure status
2. **Security Events**: Log any validation failures or suspicious activities
3. **State Changes**: Log all state transitions for audit purposes
4. **Error Details**: Log detailed error information for debugging
5. **Emergency Redemptions**: Log all emergency redemptions with justification

## API Endpoints

### POST /redeem
Initiates a redemption process for an IOU note.

**Request Body:**
```json
{
  "issuer_pubkey": "hex_encoded_public_key",
  "recipient_pubkey": "hex_encoded_public_key",
  "total_debt": 5000000000,
  "emergency": false
}
```

**Response:**
- Success: `200 OK` with redemption details
- Failure: `400 Bad Request` or `500 Internal Server Error` with error message

**Success Response:**
```json
{
  "success": true,
  "data": {
    "redemption_id": "unique_redemption_identifier",
    "total_debt": 5000000000,
    "already_redeemed": 0,
    "redeem_amount": 500000000,
    "tracker_lookup_proof": "hex_encoded_proof",
    "reserve_insert_proof": "hex_encoded_proof",
    "tracker_signature": "hex_encoded_signature",
    "reserve_signature": "hex_encoded_signature",
    "is_first_redemption": true,
    "transaction_pending": true
  },
  "error": null
}
```

### POST /redemption/prepare
Prepare a complete redemption with real AVL proofs and tracker signatures from Ergo node.

**Request Body:**
```json
{
  "issuer_pubkey": "hex_encoded_public_key",
  "recipient_pubkey": "hex_encoded_public_key",
  "total_debt": 5000000000
}
```

**Response:**
- Success: `200 OK` with complete redemption preparation data
- Failure: `400 Bad Request` or `500 Internal Server Error` with error message

**Success Response:**
```json
{
  "success": true,
  "data": {
    "redemption_id": "redemption_unique_id",
    "total_debt": 5000000000,
    "already_redeemed": 0,
    "tracker_lookup_proof": "hex_encoded_avl_proof_context_var_8",
    "reserve_lookup_proof": null,
    "reserve_insert_proof": "hex_encoded_avl_proof_context_var_5",
    "tracker_signature": "hex_encoded_tracker_signature_from_ergo_node",
    "reserve_signature": "hex_encoded_reserve_owner_signature",
    "tracker_pubkey": "hex_encoded_tracker_public_key",
    "tracker_state_digest": "hex_encoded_tracker_state_digest",
    "reserve_state_digest": "hex_encoded_reserve_state_digest",
    "block_height": 1234567,
    "is_first_redemption": true
  },
  "error": null
}
```

### GET /proof/redemption
Get redemption-specific proof with tracker and reserve state digests.

**Query Parameters:**
- `issuer_pubkey`: Issuer's public key (hex encoded)
- `recipient_pubkey`: Recipient's public key (hex encoded)

**Response:**
- Success: `200 OK` with redemption proof
- Failure: `400 Bad Request` with error message

**Success Response:**
```json
{
  "success": true,
  "data": {
    "tracker_lookup_proof": "hex_encoded_avl_proof_context_var_8",
    "reserve_lookup_proof": "hex_encoded_avl_proof_context_var_7_or_null",
    "reserve_insert_proof": "hex_encoded_avl_proof_context_var_5",
    "tracker_state_digest": "hex_encoded_tracker_state_digest",
    "reserve_state_digest": "hex_encoded_reserve_state_digest",
    "total_debt": 5000000000,
    "already_redeemed": 0,
    "proof_valid": true,
    "is_first_redemption": true
  },
  "error": null
}
```

### POST /tracker/signature
Request tracker signature for redemption.

**Request Body:**
```json
{
  "issuer_pubkey": "hex_encoded_public_key",
  "recipient_pubkey": "hex_encoded_public_key",
  "total_debt": 5000000000,
  "emergency": false
}
```

**Response:**
- Success: `200 OK` with tracker signature
- Failure: `400 Bad Request` or `500 Internal Server Error` with error message

**Success Response:**
```json
{
  "success": true,
  "data": {
    "tracker_signature": "hex_encoded_65_byte_schnorr_signature",
    "tracker_pubkey": "hex_encoded_tracker_public_key",
    "message_signed": "hex_encoded_48_byte_message_key_totalDebt_timestamp",
    "is_emergency": false
  },
  "error": null
}
```

## Tracker-Assisted 2-Phase Endpoints (`/redemption/build`, `/redemption/submit`)

In this flow the tracker builds the unsigned transaction and signs the fee input(s)
locally (using the configured `tracker_secret_key`); the client (CLI/TUI/cold signer)
adds the reserve input's `proveDlog(recipient)` over the identical `bytes_to_sign` and
returns the fully-signed transaction for broadcast. Neither end reorders the reserve
input's context extension — the build emits it in the node's canonical (Scala index)
order, and the client splices the proof into that exact transaction.

### POST /redemption/build

Builds the unsigned redemption transaction, generates all AVL proofs, computes the
tracker Schnorr signature (context var `#6`), selects the reserve and fee boxes, and
signs the fee input(s).

**Reserve selection:** smallest sufficient reserve that is unspent (`/utxo/byId`) and
whose on-chain R5 digest equals the tracker's local reserve-tree digest. A mismatch
means local state is stale and the reserve is skipped.

**`new_already_redeemed`:** computed from the **reserve-tree lookup** (previous
cumulative value for this reserve, `0` for a first redemption) plus the requested
amount — *not* from the note record. With multiple reserves per issuer (each reserve
has its own AVL tree) the note's `amount_redeemed` can differ from a given reserve's
tree entry; the on-chain contract only sees the spent reserve's tree.

**Response (excerpt):**
```json
{
  "unsigned_tx": { "...": "node-canonical unsigned tx (extension in Scala order)" },
  "partial_tx": { "...": "fee-signed tx; reserve input proof empty" },
  "input_box_binaries": ["sigma-serialized boxes, tx-input order"],
  "data_box_binaries": ["tracker box"],
  "headers": ["last 10 block headers for PreHeader/ErgoStateContext"],
  "reserve_box_id": "...",
  "new_already_redeemed": 100000000,
  "is_first_redemption": true,
  "fee": 1000000
}
```

### POST /redemption/submit

Broadcasts the fully-signed transaction via the node and then **syncs local state**.

**Request Body:**
```json
{
  "signed_tx": { "...": "fully-signed transaction JSON" },
  "issuer_pubkey": "hex_encoded_33_byte_key",
  "recipient_pubkey": "hex_encoded_33_byte_key",
  "redeemed_amount": 100000000,
  "new_already_redeemed": 100000000
}
```

**State-sync contract (required):** after a successful broadcast the tracker MUST:

1. Increment the note's `amount_redeemed` by `redeemed_amount` (note accounting;
   cumulative redeemed against total debt), refreshing the note timestamp.
2. Sync the reserve AVL tree entry to `new_already_redeemed` keyed with the note's
   **pre-refresh** payment timestamp (the on-chain reserve tree value is
   `payment_timestamp || already_redeemed`). This value comes from the build response,
   not from the note record — the two diverge for fresh reserves or repaired state.

If the sync fails, the tx is already on-chain: the error is logged and the response
still returns the tx id; state must then be repaired manually (see below). A submit
that skips this sync leaves the reserve tree stale, and the next `/redemption/build`
will find no reserve whose on-chain R5 matches the local digest.

### POST /redeem/complete (manual completion / repair)

The legacy completion endpoint accepts an optional `new_already_redeemed` field; when
provided it is used as the reserve-tree value instead of the note's cumulative amount.
This is the supported way to repair local state after an out-of-band redemption:

```json
{
  "redemption_id": "repair-<txid>",
  "issuer_pubkey": "...",
  "recipient_pubkey": "...",
  "redeemed_amount": 100000000,
  "new_already_redeemed": 100000000
}
```

### Known contract limitation / upgrade

The legacy reserve contract (`contract/basis.es:345`) used strict `insert` into the reserve AVL tree. With that contract a redemption only verifies against a reserve whose tree does not yet contain the note key — in practice a freshly created empty-tree reserve. Repeated redemptions against one reserve fail local/node evaluation with `AvlTree: Incorrect insert`.

The current compiled reserve contract uses `insertOrUpdate` for the reserve AVL tree, which removes this limitation: a note can be redeemed multiple times against the same reserve as long as the cumulative redeemed amount is increased correctly and the R7 refund initiation height is preserved. The tracker code reflects this:

- `basis_trees/src/avl_tree.rs::generate_insert_proof` uses `Operation::InsertOrUpdate` so proofs are valid for both new and existing reserve-tree keys.
- `RedemptionRequest` carries `reserve_refund_initiation_height` and the transaction builder preserves the value in the updated reserve output's `R7` register.
- Acceptance policies can include a `no_pending_refund` predicate to reject notes backed by a reserve with a non-zero R7 refund height.

Deploying systems should ensure the configured reserve contract P2S matches the contract they intend to use; the tracker will emit the correct transaction format for either, but the strict-insert contract cannot support consecutive redemptions. The legacy P2S begins with `4ZhBzJfN...`; the current default P2S begins with `3PQnJ92K...`. Both constants are maintained in `crates/basis_store/src/contract_compiler.rs`.

## Integration with Blockchain Scanner

The redemption process integrates with the blockchain scanner to:

1. **Monitor Reserves**: Track reserve boxes on the blockchain for collateral updates
2. **Verify Transactions**: Confirm redemption transactions are processed on-chain
3. **Update State**: Reflect blockchain state changes in the local tracker
4. **Detect Double Spending**: AVL tree in contract prevents multiple redemptions
5. **Track Tracker Boxes**: Monitor tracker commitment boxes for state digests

## Integration with Ergo Node API

The redemption process integrates with the Ergo node API to:

1. **Tracker Schnorr Signatures**: The tracker server either signs redemption messages locally using a configured `tracker_secret_key`, or delegates to the Ergo node's `/utils/schnorrSign` endpoint. Redeemers request the tracker signature through the tracker server's `/tracker/signature` API, not directly from the Ergo node.
2. **Transaction Signing**: Redemption transactions are built in the format expected by `/wallet/transaction/sign`, with `inputsRaw`, `dataInputsRaw`, and `secrets.dlog`, so the node can satisfy the recipient's `proveDlog` spend condition.
3. **Transaction Broadcast**: Signed redemption transactions are broadcast to the network via `/transactions`.
4. **State Verification**: Access current blockchain state for redemption validation, including reserve boxes, tracker boxes, and current height.
5. **Tracker Box Lookup**: Query tracker box information including creation height and registers.
6. **Box Retrieval**: Fetch serialized box bytes (`/utxo/byId/binary`) for inclusion in `inputsRaw` and `dataInputsRaw`, and wallet boxes (`/wallet/boxes/unspent`) for fee inputs.

## Emergency Redemption

### Overview
If the tracker becomes unavailable, emergency redemption is available after 3 days (3 * 720 blocks) from tracker creation.

### Conditions
- **Time Lock**: `currentHeight - trackerCreationHeight > 3 * 720`
- **Scope**: All debts associated with this tracker become eligible simultaneously
- **Signature**: Tracker signature bytes must still be provided in context var #6, but verification is bypassed

### Message Format
```
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)
where key = blake2b256(ownerKeyBytes || receiverBytes)
```

### Process Changes
1. Build message with timestamp (same as normal redemption)
2. Request signatures on message
3. Contract checks `enoughTimeSpent` flag
4. Tracker signature verification bypassed if enough time spent

### Security Considerations
- Emergency redemption is a last resort mechanism
- All debts become eligible simultaneously (not per-debt)
- Tracker signature still required in transaction (verification bypassed)
- Designed for tracker unavailability scenarios

## Security Considerations

1. **Signature Verification**: Both reserve owner and tracker signatures required (except emergency)
2. **Time Locks**: Emergency redemption has 3-day time lock from tracker creation
3. **Collateral Checks**: Verify sufficient collateral exists before redemption
4. **AVL Tree Tracking**: Cumulative redeemed amounts tracked in on-chain AVL tree
5. **Tracker Verification**: totalDebt must match value committed in tracker's AVL tree
6. **Double Redemption Prevention**: AVL tree design prevents redeeming same debt twice
7. **Tracker Signature Handling**: Tracker signatures are either generated locally by the server using a configured `tracker_secret_key`, or delegated to the Ergo node's `/utils/schnorrSign` endpoint. In both cases, redeemers obtain the signature through the tracker server's `/tracker/signature` API and never need the tracker private key.
8. **Proof Verification**: All AVL proofs verified against on-chain tree commitments

## Context Extension Variables Summary

For redemption transactions:

| Variable | Type | Description | Source |
|----------|------|-------------|--------|
| #0 | Byte | Action byte (0x00) | Constant |
| #1 | GroupElement | Receiver pubkey | Request |
| #2 | Coll[Byte] | Reserve owner signature | Signature API |
| #3 | Long | Total debt amount | Tracker AVL tree |
| #5 | Coll[Byte] | Reserve insert/update proof | AVL proof generator |
| #6 | Coll[Byte] | Tracker signature | Tracker server (`/tracker/signature`) |
| #7 | Coll[Byte] | Reserve lookup proof | AVL proof generator (optional) |
| #8 | Coll[Byte] | Tracker lookup proof | AVL proof generator |

This specification provides complete state management and process flow for redemption operations in the Basis Tracker system.
