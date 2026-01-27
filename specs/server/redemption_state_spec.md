# Redemption State Specification

## Overview
This document specifies the state management and process flow for redemption operations in the Basis Tracker system. Redemption allows holders of IOU notes to claim collateral from the issuer's reserve based on the outstanding debt represented by the note.

## Redemption Data Structures

### RedemptionRequest
The redemption request contains the parameters needed to initiate a redemption:

```rust
pub struct RedemptionRequest {
    /// Issuer's public key (hex encoded)
    pub issuer_pubkey: String,
    /// Recipient's public key (hex encoded)
    pub recipient_pubkey: String,
    /// Amount to redeem
    pub amount: u64,
    /// Timestamp of the note being redeemed
    pub timestamp: u64,
    /// Reserve contract box ID (hex encoded)
    pub reserve_box_id: String,
    /// Recipient's address for redemption output
    pub recipient_address: String,
}
```

### RedemptionData
The redemption data structure contains the complete information for a processed redemption:

```rust
pub struct RedemptionData {
    /// Unique redemption ID
    pub redemption_id: String,
    /// The note being redeemed
    pub note: IouNote,
    /// AVL tree proof for the note
    pub avl_proof: Vec<u8>,
    /// Redemption transaction bytes (hex encoded)
    pub transaction_bytes: String,
    /// Required signatures for the transaction
    pub required_signatures: Vec<String>,
    /// Estimated transaction fee
    pub estimated_fee: u64,
    /// Timestamp when redemption can be executed
    pub redemption_time: u64,
}
```

### RedemptionError
Possible error conditions during redemption:

```rust
pub enum RedemptionError {
    #[error("Note not found")]
    NoteNotFound,
    #[error("Invalid note signature")]
    InvalidNoteSignature,
    #[error("Redemption too early: {0} < {1}")]
    RedemptionTooEarly(u64, u64),
    #[error("Insufficient collateral: {0} < {1}")]
    InsufficientCollateral(u64, u64),
    #[error("Reserve not found: {0}")]
    ReserveNotFound(String),
    #[error("Transaction building error: {0}")]
    TransactionError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
}
```

## Redemption Process Flow

### 1. Initiate Redemption
The redemption process begins when a recipient initiates a redemption request:

1. **Validate Request Parameters**
   - Parse and validate issuer and recipient public keys
   - Verify the redemption amount is positive
   - Check that the timestamp is valid

2. **Lookup IOU Note**
   - Search for the note using issuer and recipient public keys
   - Return `RedemptionError::NoteNotFound` if not found

3. **Verify Note Signature**
   - Validate the note's signature against the issuer's public key
   - Return `RedemptionError::InvalidNoteSignature` if invalid

4. **Check Outstanding Debt**
   - Verify that redemption amount ≤ `note.outstanding_debt()`
   - Return `RedemptionError::InsufficientCollateral` if exceeded

5. **Validate Time Lock**
   - Check that current time ≥ `note.timestamp + TIME_LOCK_PERIOD`
   - For testing: TIME_LOCK_PERIOD = 60 seconds (1 minute)
   - For production: TIME_LOCK_PERIOD = 604800 seconds (1 week)
   - Return `RedemptionError::RedemptionTooEarly` if not met

6. **Find Matching Reserve**
   - Look up reserve associated with the issuer
   - Use normalized public key matching (handle "07" prefix)
   - Return `RedemptionError::ReserveNotFound` if no matching reserve

7. **Generate Proof**
   - Create AVL tree proof for the note
   - Return `RedemptionError::StorageError` if proof generation fails

8. **Build Transaction**
   - Construct redemption transaction with all required components
   - Include reserve box ID, tracker box ID, recipient address
   - Return `RedemptionError::TransactionError` if building fails

9. **Return Redemption Data**
   - Generate unique redemption ID
   - Package all redemption information
   - Return success with `RedemptionData`

### 2. Complete Redemption
After the redemption transaction is successfully submitted to the blockchain:

1. **Update Note State**
   - Increment `note.amount_redeemed` by redeemed amount
   - Update `note.timestamp` to current time
   - Store updated note in tracker state

2. **Update Reserve State**
   - Decrease reserve's collateral by redeemed amount
   - Update reserve's total debt
   - Store updated reserve in tracker state

3. **Update AVL Tree**
   - Update the AVL tree with the new note state
   - Generate new root digest
   - Store updated tree state

## State Transitions

### Note State Transitions
```
[Initial State] -(Redemption Request)-> [Partially Redeemed] -(Full Redemption)-> [Fully Redeemed]
```

- Initial State: `amount_redeemed = 0`, `outstanding_debt = amount_collected`
- Partially Redeemed: `0 < amount_redeemed < amount_collected`, `outstanding_debt = amount_collected - amount_redeemed`
- Fully Redeemed: `amount_redeemed = amount_collected`, `outstanding_debt = 0`

### Reserve State Transitions
```
[Created] -(Redemption Occurs)-> [Updated Collateral] -(More Redemptions)-> [Reduced Collateral]
```

- Each redemption reduces the reserve's available collateral
- The reserve's total debt decreases as notes are redeemed

## Validation Rules

### Pre-Redemption Validation
1. **Public Key Format**: Both issuer and recipient public keys must be valid hex-encoded 33-byte values
2. **Amount Bounds**: Redemption amount must be > 0 and ≤ note's outstanding debt
3. **Time Lock**: Current time must be ≥ note timestamp + time lock period
4. **Reserve Existence**: A matching reserve must exist for the issuer
5. **Sufficient Collateral**: Reserve must have sufficient collateral to cover redemption

### Post-Redemption Validation
1. **State Consistency**: Note and reserve states must remain consistent
2. **Balance Integrity**: Total system balances must be preserved
3. **Signature Verification**: All required signatures must be valid
4. **Blockchain Confirmation**: Redemption transaction must be confirmed on blockchain

## Error Handling

### Recovery Procedures
1. **Failed Redemption**: If redemption fails, roll back any partial state changes
2. **Incomplete Transaction**: If blockchain transaction fails, restore previous state
3. **Signature Mismatch**: If signatures don't match, reject redemption and log incident
4. **Double Spend Prevention**: Prevent multiple redemptions of the same note

### Logging Requirements
1. **Redemption Attempts**: Log all redemption attempts with success/failure status
2. **Security Events**: Log any validation failures or suspicious activities
3. **State Changes**: Log all state transitions for audit purposes
4. **Error Details**: Log detailed error information for debugging

## API Endpoints

### POST /redeem
Initiates a redemption process for an IOU note.

**Request Body:**
```json
{
  "issuer_pubkey": "hex_encoded_public_key",
  "recipient_pubkey": "hex_encoded_public_key",
  "amount": 1000,
  "timestamp": 1672531200
}
```

**Response:**
- Success: `200 OK` with redemption details
- Failure: `400 Bad Request` with error message

**Success Response:**
```json
{
  "success": true,
  "data": {
    "redemption_id": "unique_redemption_identifier",
    "amount": 1000,
    "timestamp": 1672531200,
    "proof_available": true,
    "transaction_pending": true,
    "transaction_data": {
      "transaction_bytes": "hex_encoded_transaction",
      "required_signatures": ["pubkey1", "pubkey2"],
      "estimated_fee": 1000000
    }
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
  "amount": 1000,
  "timestamp": 1672531200
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
    "avl_proof": "hex_encoded_avl_proof",
    "tracker_signature": "hex_encoded_tracker_signature_from_ergo_node",
    "tracker_pubkey": "hex_encoded_tracker_public_key",
    "tracker_state_digest": "hex_encoded_tracker_state_digest",
    "block_height": 1500
  },
  "error": null
}
```

### GET /proof/redemption
Get redemption-specific proof with tracker state digest.

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
    "avl_proof": "hex_encoded_avl_proof",
    "tracker_state_digest": "hex_encoded_tracker_state_digest",
    "proof_valid": true
  },
  "error": null
}
```

## Integration with Blockchain Scanner

The redemption process integrates with the blockchain scanner to:

1. **Monitor Reserves**: Track reserve boxes on the blockchain for collateral updates
2. **Verify Transactions**: Confirm redemption transactions are processed on-chain
3. **Update State**: Reflect blockchain state changes in the local tracker
4. **Detect Double Spending**: Prevent multiple redemptions of the same note

## Integration with Ergo Node API

The redemption process integrates with the Ergo node API to:

1. **Real Schnorr Signatures**: Use the `/utils/schnorrSign` endpoint to generate tracker signatures securely
2. **Transaction Submission**: Prepare transactions that can be submitted to the Ergo node for blockchain inclusion
3. **State Verification**: Access current blockchain state for redemption validation

## Security Considerations

1. **Signature Verification**: All notes must have valid signatures from the issuer
2. **Time Locks**: Enforce minimum time locks to prevent immediate redemptions
3. **Collateral Checks**: Verify sufficient collateral exists before redemption
4. **Rate Limiting**: Prevent abuse through rate limiting mechanisms
5. **Access Control**: Restrict redemption to legitimate note holders only
6. **Real Schnorr Signature Integration**: Tracker signatures are generated via Ergo node's `/utils/schnorrSign` API to keep private keys secure in the node
7. **AVL Proof Verification**: Redemption transactions include AVL tree proofs that can be verified against on-chain commitments