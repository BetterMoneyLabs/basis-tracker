# Redemption State Specification

## Overview
This document specifies the state management and process flow for redemption operations in the Basis Tracker system. Redemption allows holders of IOU notes to claim collateral from the issuer's reserve based on the outstanding debt represented by the note. The contract tracks cumulative redeemed amounts using AVL trees to prevent double redemptions.

## Redemption Data Structures

### RedemptionRequest
The redemption request contains the parameters needed to initiate a redemption:

```rust
pub struct RedemptionRequest {
    /// Issuer's public key (hex encoded)
    pub issuer_pubkey: String,
    /// Recipient's public key (hex encoded)
    pub recipient_pubkey: String,
    /// Total cumulative debt amount (not just redemption amount)
    pub total_debt: u64,
    /// Flag indicating if this is emergency redemption
    pub emergency: bool,
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
    /// AVL proof for reserve tree insertion (context var #5)
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
   - Generate reserve tree insert proof (context var #5)
   - Return `RedemptionError::AvlProofError` if proof generation fails

9. **Request Signatures**
   - Build signing message: `message = key || longToByteArray(totalDebt)`
   - For emergency: `message = key || longToByteArray(totalDebt) || longToByteArray(0L)`
   - Request reserve owner's signature on message
   - Request tracker's signature on message via Ergo node API
   - Return `RedemptionError::InvalidReserveSignature` or `InvalidTrackerSignature` if invalid

10. **Build Transaction**
    - Construct redemption transaction with all required components
    - Include reserve box ID, tracker box ID, recipient address
    - Set context extension variables (#0-#8)
    - Return `RedemptionError::TransactionError` if building fails

11. **Return Redemption Data**
    - Generate unique redemption ID
    - Package all redemption information
    - Return success with `RedemptionData`

### 2. Complete Redemption
After the redemption transaction is successfully submitted to the blockchain:

1. **Update Local State**
    - Update reserve's cumulative redeemed amount in local storage
    - Update note state if tracking separately
    - Store transaction ID for reference

2. **Monitor Blockchain**
    - Wait for transaction confirmation
    - Update reserve state from blockchain events
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
    "block_height": 1500,
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
    "message_signed": "hex_encoded_message_key_totalDebt_or_key_totalDebt_0L",
    "is_emergency": false
  },
  "error": null
}
```

## Integration with Blockchain Scanner

The redemption process integrates with the blockchain scanner to:

1. **Monitor Reserves**: Track reserve boxes on the blockchain for collateral updates
2. **Verify Transactions**: Confirm redemption transactions are processed on-chain
3. **Update State**: Reflect blockchain state changes in the local tracker
4. **Detect Double Spending**: AVL tree in contract prevents multiple redemptions
5. **Track Tracker Boxes**: Monitor tracker commitment boxes for state digests

## Integration with Ergo Node API

The redemption process integrates with the Ergo node API to:

1. **Real Schnorr Signatures**: Use the `/utils/schnorrSign` endpoint to generate tracker signatures securely
2. **Transaction Submission**: Prepare transactions that can be submitted to the Ergo node for blockchain inclusion
3. **State Verification**: Access current blockchain state for redemption validation
4. **Tracker Box Lookup**: Query tracker box information including creation height and registers

## Emergency Redemption

### Overview
If the tracker becomes unavailable, emergency redemption is available after 3 days (3 * 720 blocks) from tracker creation.

### Conditions
- **Time Lock**: `currentHeight - trackerCreationHeight > 3 * 720`
- **Scope**: All debts associated with this tracker become eligible simultaneously
- **Signature**: Tracker signature still required but verification is bypassed

### Message Format
```
message = key || longToByteArray(totalDebt) || longToByteArray(0L)
where key = blake2b256(ownerKeyBytes || receiverBytes)
```

### Process Changes
1. Build message with appended `0L` (longToByteArray(0L))
2. Request signatures on modified message
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
7. **Remote Signature Generation**: Tracker signatures generated via Ergo node API to protect private keys
8. **Proof Verification**: All AVL proofs verified against on-chain tree commitments

## Context Extension Variables Summary

For redemption transactions:

| Variable | Type | Description | Source |
|----------|------|-------------|--------|
| #0 | Byte | Action byte (0x00) | Constant |
| #1 | GroupElement | Receiver pubkey | Request |
| #2 | Coll[Byte] | Reserve owner signature | Signature API |
| #3 | Long | Total debt amount | Tracker AVL tree |
| #5 | Coll[Byte] | Reserve insert proof | AVL proof generator |
| #6 | Coll[Byte] | Tracker signature | Ergo node API |
| #7 | Coll[Byte] | Reserve lookup proof | AVL proof generator (optional) |
| #8 | Coll[Byte] | Tracker lookup proof | AVL proof generator |

This specification provides complete state management and process flow for redemption operations in the Basis Tracker system.
