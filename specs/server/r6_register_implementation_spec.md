# R6 Register Implementation Specification

## Overview

This document specifies the implementation requirements for supporting the R6 register in the Basis Tracker system. The R6 register in reserve boxes contains the tracker NFT ID (identifying which tracker server the reserve is linked to).

This specification focuses on the implementation changes needed to properly handle the R6 register in reserve boxes.

## Problem Statement

The Basis Tracker system currently does not properly handle the R6 register in reserve boxes. According to the contract specification, reserve boxes must have an R6 register containing the tracker NFT ID to identify which tracker server the reserve is linked to. The system needs to be updated to:

1. Parse and store the R6 register value from reserve boxes
2. Include the R6 register in redemption transactions
3. Expose the R6 register value through API endpoints
4. Ensure proper preservation of R6 register during transaction processing

## Implementation Requirements

### 1. Reserve Scanner Updates

#### 1.1 Data Structure Updates
- **File**: `crates/basis_store/src/models.rs`
- **Change**: Update `ReserveInfo` struct to include R6 register field
- **New field**: `tracker_nft_id: String` - The hex-encoded serialized tracker NFT ID from R6 register
- **Format**: The tracker NFT ID should be stored in the serialized SColl(SByte) format using Sigma serialization:
  - First byte: VLQ (Variable-Length Quantity) encoding of the length (0x20 for 32-byte NFT ID)
  - Following 32 bytes: The actual tracker NFT ID bytes
  - When hex-encoded: 66 characters total (first 2 chars for length, next 64 chars for NFT ID)

#### 1.2 Parsing Logic Updates
- **File**: `crates/basis_store/src/ergo_scanner.rs`
- **Change**: Update reserve box parsing to extract R6 register value
- **Serialization Format**: The R6 register value is a serialized SColl(SByte) containing the tracker NFT ID:
  - Parse the register value as a serialized collection using Sigma deserialization
  - The first byte is the VLQ length encoding (should be 0x20 for 32-byte NFT ID)
  - The following 32 bytes are the tracker NFT ID
  - Store the entire serialized value as hex-encoded string
- **Validation**: Verify that R6 register contains a valid tracker NFT ID matching configuration

#### 1.3 Storage Updates
- **File**: `crates/basis_store/src/persistence.rs`
- **Change**: Update `ReserveStorage` to store and retrieve R6 register value
- **Field**: Store tracker NFT ID from R6 register in persistent storage

### 2. Transaction Builder Updates

#### 2.1 Redemption Transaction Creation
- **File**: `crates/basis_store/src/transaction_builder.rs`
- **Change**: Update redemption transaction creation to preserve R6 register from input to output
- **Logic**: Copy R6 register value from input reserve box to output reserve box
- **Serialization Format**: When creating the output box, ensure R6 register follows the proper SColl(SByte) serialization:
  - The R6 register value must be a serialized collection (SColl(SByte)) using Sigma serialization
  - For 32-byte tracker NFT IDs, this includes a VLQ length prefix (0x20) followed by 32 bytes of NFT ID
  - The serialized bytes should be properly formatted for inclusion in the transaction
- **Validation**: Ensure R6 register value remains consistent between input and output boxes

#### 2.2 Register Value Handling
- **Requirement**: The R6 register in the output reserve box must contain the same tracker NFT ID as the input box
- **Implementation**: Extract R6 register from input reserve box and include in output box registers

### 3. API Endpoint Updates

#### 3.1 Reserve Information Endpoints
- **Endpoint**: `GET /reserves/issuer/{pubkey}`
- **Change**: Include R6 register value (tracker NFT ID) in response
- **Field**: Add `tracker_nft_id` field to reserve information in API response

#### 3.2 All Reserves Endpoint
- **Endpoint**: `GET /reserves`
- **Change**: Include R6 register value (tracker NFT ID) for each reserve
- **Field**: Add `tracker_nft_id` field to each reserve in the list response

#### 3.3 Specific Reserve Endpoint
- **Endpoint**: `GET /reserves/{box_id}`
- **Change**: Include R6 register value (tracker NFT ID) in response
- **Field**: Add `tracker_nft_id` field to specific reserve information

### 4. Data Model Updates

#### 4.1 ReserveInfo Structure
```rust
pub struct ReserveInfo {
    pub box_id: String,
    pub owner_pubkey: String,
    pub collateral_amount: u64,
    pub total_debt: u64,
    pub tracker_nft_id: String,  // NEW: From R6 register (hex-encoded serialized SColl(SByte) format)
    pub last_updated_height: u64,
    pub last_updated_timestamp: u64,
}
```

The `tracker_nft_id` field should store the hex-encoded serialized representation of the tracker NFT ID as it appears in the R6 register, following the Sigma serialization format:
- First byte: VLQ length encoding (0x20 for 32-byte NFT ID)
- Next 32 bytes: The actual tracker NFT ID bytes
- When stored as hex string: 66 characters total (2 for length + 64 for NFT ID)

#### 4.2 API Response Models
- Update API response models to include `tracker_nft_id` field
- Ensure proper serialization of R6 register information

### 5. Configuration Updates

#### 5.1 Tracker NFT ID Validation
- **File**: `crates/basis_server/src/config.rs`
- **Change**: Ensure configured tracker NFT ID matches R6 register values in reserve boxes
- **Validation**: Verify that reserve boxes have correct tracker NFT ID in R6 register
- **Serialization Comparison**: When validating tracker NFT ID, compare the actual 32-byte NFT ID portion of the R6 register (after the VLQ length prefix), not the entire serialized collection
- **Format Validation**: Verify that the R6 register contains a properly serialized SColl(SByte) with correct length prefix

## Implementation Steps

### Step 1: Update Data Models
1. Add `tracker_nft_id` field to `ReserveInfo` struct
2. Update serialization/deserialization for the new field
3. Update all related data structures to include R6 register information

### Step 2: Update Reserve Scanner
1. Modify parsing logic to extract R6 register from reserve boxes
2. Store R6 register value in reserve storage
3. Validate that R6 register contains correct tracker NFT ID

### Step 3: Update Transaction Builder
1. Modify redemption transaction creation to preserve R6 register
2. Ensure R6 register is included in output box registers
3. Validate that R6 register value is consistent between input and output

### Step 4: Update API Endpoints
1. Modify reserve-related endpoints to include R6 register information
2. Update response models to include tracker NFT ID
3. Ensure proper error handling for missing R6 register values

### Step 5: Testing
1. Create unit tests for R6 register parsing
2. Create integration tests for redemption transactions with R6 register
3. Test API endpoints to ensure R6 register information is properly returned

## Validation Requirements

### 1. Reserve Box Validation
- R6 register must contain a valid tracker NFT ID encoded as a byte array (SColl(SByte) in ErgoTree)
- The byte array must be exactly 32 bytes (representing a SHA256 hash of the NFT ID)
- The byte array must be serialized using the Sigma serialization protocol for collections:
  - First byte: VLQ (Variable-Length Quantity) encoding of the length (0x20 for 32 bytes)
  - Following 32 bytes: The actual NFT ID bytes
- R6 register value must match the configured tracker NFT ID
- Missing R6 register should result in appropriate error handling

### 2. Transaction Validation
- R6 register value must be preserved from input to output in redemption transactions
- R6 register in output box must match R6 register in input box
- Transaction validation should verify R6 register consistency
- The R6 register value must be properly serialized as a byte array (SColl(SByte)) using Sigma serialization:
  - Length encoded as VLQ (Variable-Length Quantity) prefix
  - For 32-byte NFT IDs, the serialized format starts with 0x20 followed by 32 bytes of NFT ID

### 3. API Validation
- API responses must include R6 register value for all reserve endpoints
- R6 register value should be properly formatted as hex-encoded bytes representing the serialized SColl(SByte) format
- The hex-encoded value should include the VLQ length prefix followed by the actual NFT ID bytes
- For 32-byte NFT IDs, the hex-encoded value should start with "20" followed by 64 hex characters (32 bytes) of NFT ID
- Error responses should handle cases where R6 register is missing

## Error Handling

### 1. Missing R6 Register
- If a reserve box is missing R6 register, log warning and skip processing
- API should return appropriate error when R6 register is expected but missing

### 2. Invalid R6 Register Value
- If R6 register contains invalid tracker NFT ID format, log error and skip processing
- Validation should ensure R6 register contains proper serialized SColl(SByte) format:
  - First byte should be a valid VLQ length encoding (0x20 for 32-byte NFT ID)
  - Following bytes should be exactly 32 bytes of tracker NFT ID
  - Total serialized length should be 33 bytes (1 length byte + 32 NFT ID bytes)
- The hex-encoded value should be exactly 66 characters for 32-byte NFT IDs

### 3. Mismatched Tracker NFT ID
- If R6 register value doesn't match configured tracker NFT ID, log warning
- System should handle reserves with different tracker NFT IDs appropriately

## Integration Points

### 1. Ergo Node Integration
- Reserve scanner must properly parse R6 register from Ergo node API responses
- The R6 register value from Ergo node is serialized as SColl(SByte) using Sigma serialization:
  - First byte: VLQ length encoding (0x20 for 32-byte NFT ID)
  - Following 32 bytes: The actual tracker NFT ID
- Transaction builder must format R6 register correctly for Ergo node transaction API using the same serialization format

### 2. Tracker Integration
- R6 register in reserve boxes must match the tracker NFT ID used by the tracker system
- Cross-verification between tracker commitments and reserve boxes should use R6 register information from reserve boxes

### 3. Client Integration
- API responses should provide R6 register information to clients
- Clients should be able to use R6 register information to verify tracker associations

## Testing Strategy

### 1. Unit Tests
- Test R6 register parsing from reserve box data in serialized SColl(SByte) format
- Test R6 register preservation in transaction building with proper Sigma serialization
- Test R6 register validation logic for correct VLQ length encoding and byte array format
- Test deserialization of R6 register values to extract the actual tracker NFT ID

### 2. Integration Tests
- Test reserve scanner with boxes containing R6 register
- Test redemption transactions with R6 register preservation
- Test API endpoints returning R6 register information

### 3. End-to-End Tests
- Test complete redemption flow with R6 register handling
- Verify that R6 register information persists through the entire process
- Test error conditions with missing or invalid R6 registers

## Performance Considerations

### 1. Storage Impact
- Additional storage required for R6 register values in reserve storage
- Indexing considerations for R6 register values for efficient queries

### 2. Processing Overhead
- Minimal overhead for parsing additional register value
- No significant impact on transaction processing performance

## Security Considerations

### 1. Tracker Association Verification
- R6 register ensures reserves are properly associated with correct tracker
- Prevents cross-tracker confusion in multi-tracker environments

### 2. Register Validation
- Proper validation of R6 register values prevents malformed data
- Ensures only valid tracker NFT IDs are accepted

## Backward Compatibility

### 1. Existing Data
- System should handle reserve boxes without R6 register gracefully
- Default behavior for boxes missing R6 register should be well-defined

### 2. API Changes
- New API fields should be optional or have sensible defaults
- Existing API consumers should continue to function normally

## Deployment Considerations

### 1. Migration
- Existing reserve data may need migration to include R6 register information
- Consider how to handle legacy reserve boxes without R6 register

### 2. Configuration
- Ensure tracker NFT ID is properly configured before deployment
- Verify that Ergo node connections can access register information

This specification ensures that the Basis Tracker system properly handles the R6 register in reserve boxes, maintaining the link between reserves and their associated tracker servers as required by the contract specification.