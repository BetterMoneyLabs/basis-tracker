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
- **Format**: The tracker NFT ID should be stored in the serialized SColl(SByte) format using Sigma serialization

#### 1.2 Parsing Logic Updates
- **File**: `crates/basis_store/src/ergo_scanner.rs`
- **Change**: Update reserve box parsing to extract R6 register value
- **Register Serialization Details**: To properly work with byte arrays in registers according to the byte_array_register_serialization.md specification:
  - **Serializing Byte Arrays to Registers**:
    1. **Create a byte vector**: Initialize with the required byte data (the 32-byte tracker NFT ID)
    2. **Convert to ErgoTree collection**: Transform to `SColl(SByte)` type
    3. **Store in register**: Place the serialized bytes in the R6 register
  - Rust code example for serialization:
    ```rust
    use ergo_lib::chain::register::NonMandatoryRegisters;
    use std::collections::HashMap;
    use ergotree_ir::mir::value::Value;
    use ergotree_ir::types::stype::SType;
    use sigma_ser::serializer::SigmaSerializable;
    use ergo_lib::ergotree_ir::serialization::sigma_byte_reader::SigmaByteRead;
    use ergo_lib::ergotree_ir::serialization::sigma_byte_writer::SigmaByteWrite;

    // Create a byte array with the tracker NFT ID (32 bytes)
    // Example: tracker_nft_id_bytes = [0x01, 0x02, 0x03, ..., 0x20] (32 bytes)
    let tracker_nft_id_bytes: Vec<i8> = /* 32-byte tracker NFT ID as signed bytes */;

    // Convert to ErgoTree value
    let byte_array_value = Value::from(tracker_nft_id_bytes);

    // In practice, when using ergo-lib to construct a transaction:
    use ergo_lib::chain::register::RegisterNumber;

    // Create registers map with byte array in register R6
    let mut registers_map = HashMap::new();
    registers_map.insert(RegisterNumber::R6, byte_array_value.into());
    let registers = NonMandatoryRegisters::from_map(registers_map);

    // The byte array value can now be stored in a box when building a transaction
    // using ergo-lib's transaction builder APIs
    ```
  - Rust code example for deserialization (parsing from R6 register):
    ```rust
    use ergo_lib::chain::register::RegisterNumber;
    use ergotree_ir::mir::value::Value;
    use std::convert::TryInto;

    // Extract R6 register from a box
    if let Some(register_value) = box_with_bytes.get_register(RegisterNumber::R6) {
        match register_value.as_value() {
            Value::Coll(coll) => {
                // Verify it's a collection of bytes (SColl(SByte))
                if coll.elem_tpe == ergotree_ir::types::stype::SType::SByte {
                    // Extract the bytes as a vector of i8
                    let tracker_nft_bytes: Vec<i8> = coll.values.iter()
                        .map(|v| v.clone().try_into().unwrap_or(0i8))
                        .collect();
                    
                    // Verify the length is 32 bytes (the tracker NFT ID)
                    if tracker_nft_bytes.len() == 32 {
                        // Extract the 32-byte tracker NFT ID
                        let actual_tracker_nft_id: [u8; 32] = tracker_nft_bytes
                            .iter()
                            .map(|&b| b as u8)
                            .collect::<Vec<u8>>()
                            .try_into()
                            .unwrap_or([0u8; 32]);
                        
                        // Convert to hex string for storage (64 hex chars for 32 bytes)
                        let hex_encoded = actual_tracker_nft_id.iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        
                        println!("Tracker NFT ID: {}", hex_encoded); // Should be 64 hex chars
                    } else {
                        eprintln!("Invalid R6 register length: expected 32 bytes, got {}", tracker_nft_bytes.len());
                    }
                }
            },
            _ => eprintln!("R6 register does not contain a byte collection"),
        }
    }
    ```
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
- **Register Serialization Details**: To properly work with byte arrays in registers according to the byte_array_register_serialization.md specification:
  - **Serializing Byte Arrays to Registers**:
    1. **Create a byte vector**: Initialize with the required byte data (the 32-byte tracker NFT ID)
    2. **Convert to ErgoTree collection**: Transform to `SColl(SByte)` type
    3. **Store in register**: Place the serialized bytes in the R6 register
  - Rust code example for serialization:
    ```rust
    use ergo_lib::chain::register::NonMandatoryRegisters;
    use std::collections::HashMap;
    use ergotree_ir::mir::value::Value;
    use ergotree_ir::types::stype::SType;
    use sigma_ser::serializer::SigmaSerializable;
    use ergo_lib::ergotree_ir::serialization::sigma_byte_reader::SigmaByteRead;
    use ergo_lib::ergotree_ir::serialization::sigma_byte_writer::SigmaByteWrite;

    // Create a byte array with the tracker NFT ID (32 bytes)
    // Example: tracker_nft_id_bytes = [0x01, 0x02, 0x03, ..., 0x20] (32 bytes)
    let tracker_nft_id_bytes: Vec<i8> = /* 32-byte tracker NFT ID as signed bytes */;

    // Convert to ErgoTree value
    let byte_array_value = Value::from(tracker_nft_id_bytes);

    // In practice, when using ergo-lib to construct a transaction:
    use ergo_lib::chain::register::RegisterNumber;

    // Create registers map with byte array in register R6
    let mut registers_map = HashMap::new();
    registers_map.insert(RegisterNumber::R6, byte_array_value.into());
    let registers = NonMandatoryRegisters::from_map(registers_map);

    // The byte array value can now be stored in a box when building a transaction
    // using ergo-lib's transaction builder APIs
    ```
  - Rust code example for deserialization (parsing from R6 register):
    ```rust
    use ergo_lib::chain::register::RegisterNumber;
    use ergotree_ir::mir::value::Value;
    use std::convert::TryInto;

    // Extract R6 register from a box
    if let Some(register_value) = box_with_bytes.get_register(RegisterNumber::R6) {
        match register_value.as_value() {
            Value::Coll(coll) => {
                // Verify it's a collection of bytes (SColl(SByte))
                if coll.elem_tpe == ergotree_ir::types::stype::SType::SByte {
                    // Extract the bytes as a vector of i8
                    let tracker_nft_bytes: Vec<i8> = coll.values.iter()
                        .map(|v| v.clone().try_into().unwrap_or(0i8))
                        .collect();
                    
                    // Verify the length is 32 bytes (the tracker NFT ID)
                    if tracker_nft_bytes.len() == 32 {
                        // Extract the 32-byte tracker NFT ID
                        let actual_tracker_nft_id: [u8; 32] = tracker_nft_bytes
                            .iter()
                            .map(|&b| b as u8)
                            .collect::<Vec<u8>>()
                            .try_into()
                            .unwrap_or([0u8; 32]);
                        
                        // Convert to hex string for storage (64 hex chars for 32 bytes)
                        let hex_encoded = actual_tracker_nft_id.iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        
                        println!("Tracker NFT ID: {}", hex_encoded); // Should be 64 hex chars
                    } else {
                        eprintln!("Invalid R6 register length: expected 32 bytes, got {}", tracker_nft_bytes.len());
                    }
                }
            },
            _ => eprintln!("R6 register does not contain a byte collection"),
        }
    }
    ```
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

The `tracker_nft_id` field should store the hex-encoded representation of the tracker NFT ID as provided by ergo-lib API (the raw 32-byte tracker NFT ID):
- 32 bytes: The actual tracker NFT ID bytes (ergo-lib deserializes the register automatically)
- When stored as hex string: 64 characters total (2 hex chars per byte for 32 bytes)

### Concrete Examples
- **Example 1**: A tracker NFT ID of 32 zero bytes would be stored as:
  - Raw bytes: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
  - Hex string: `0000000000000000000000000000000000000000000000000000000000000000`

- **Example 2**: A tracker NFT ID with actual hash bytes would be stored as:
  - Raw bytes: [0x1a, 0xf2, 0x3d, 0x4e, 0x5f, 0x6a, 0x7b, 0x8c, 0x9d, 0xae, 0xbf, 0xc0, 0xd1, 0xe2, 0xf3, 0x04, 0x15, 0x26, 0x37, 0x48, 0x59, 0x6a, 0x7b, 0x8c, 0x9d, 0xae, 0xbf, 0xc0, 0xd1, 0xe2, 0xf3, 0x04]
  - Hex string: `1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304`

- **Example 3**: An actual tracker NFT ID from a SHA256 hash would be stored as:
  - Raw bytes: [0x8c, 0x69, 0x6f, 0x2d, 0x8d, 0x2b, 0x73, 0x6c, 0x3c, 0x1a, 0x4f, 0x9e, 0x8b, 0x7d, 0x6c, 0x5a, 0x4f, 0x3e, 0x2d, 0x1c, 0x0b, 0xfa, 0xe9, 0xd8, 0xc7, 0xb6, 0xa5, 0x94, 0x83, 0x72, 0x61, 0x50]
  - Hex string: `8c696f2d8d2b736c3c1a4f9e8b7d6c5a4f3e2d1c0bfae9d8c7b6a59483726150`

**Register Serialization Details**: To properly work with byte arrays in registers according to the byte_array_register_serialization.md specification:
- **Serializing Byte Arrays to Registers**:
  1. **Create a byte vector**: Initialize with the required byte data (the 32-byte tracker NFT ID)
  2. **Convert to ErgoTree collection**: Transform to `SColl(SByte)` type
  3. **Store in register**: Place the serialized bytes in the R6 register
- Rust code example for serialization:
  ```rust
  use ergo_lib::chain::register::NonMandatoryRegisters;
  use std::collections::HashMap;
  use ergotree_ir::mir::value::Value;
  use ergotree_ir::types::stype::SType;
  use sigma_ser::serializer::SigmaSerializable;
  use ergo_lib::ergotree_ir::serialization::sigma_byte_reader::SigmaByteRead;
  use ergo_lib::ergotree_ir::serialization::sigma_byte_writer::SigmaByteWrite;

  // Create a byte array with the tracker NFT ID (32 bytes)
  // Example: tracker_nft_id_bytes = [0x01, 0x02, 0x03, ..., 0x20] (32 bytes)
  let tracker_nft_id_bytes: Vec<i8> = /* 32-byte tracker NFT ID as signed bytes */;

  // Convert to ErgoTree value
  let byte_array_value = Value::from(tracker_nft_id_bytes);

  // In practice, when using ergo-lib to construct a transaction:
  use ergo_lib::chain::register::RegisterNumber;

  // Create registers map with byte array in register R6
  let mut registers_map = HashMap::new();
  registers_map.insert(RegisterNumber::R6, byte_array_value.into());
  let registers = NonMandatoryRegisters::from_map(registers_map);

  // The byte array value can now be stored in a box when building a transaction
  // using ergo-lib's transaction builder APIs
  ```
- Rust code example for deserialization (parsing from R6 register):
  ```rust
  use ergo_lib::chain::register::RegisterNumber;
  use ergotree_ir::mir::value::Value;
  use std::convert::TryInto;

  // Extract R6 register from a box
  if let Some(register_value) = box_with_bytes.get_register(RegisterNumber::R6) {
      match register_value.as_value() {
          Value::Coll(coll) => {
              // Verify it's a collection of bytes (SColl(SByte))
              if coll.elem_tpe == ergotree_ir::types::stype::SType::SByte {
                  // Extract the bytes as a vector of i8
                  let tracker_nft_bytes: Vec<i8> = coll.values.iter()
                      .map(|v| v.clone().try_into().unwrap_or(0i8))
                      .collect();
                  
                  // Verify the length is 32 bytes (the tracker NFT ID)
                  if tracker_nft_bytes.len() == 32 {
                      // Extract the 32-byte tracker NFT ID
                      let actual_tracker_nft_id: [u8; 32] = tracker_nft_bytes
                          .iter()
                          .map(|&b| b as u8)
                          .collect::<Vec<u8>>()
                          .try_into()
                          .unwrap_or([0u8; 32]);

                      // Convert to hex string for storage (64 hex chars for 32 bytes)
                      let hex_encoded = actual_tracker_nft_id.iter()
                          .map(|b| format!("{:02x}", b))
                          .collect::<String>();

                      println!("Tracker NFT ID: {}", hex_encoded); // Should be 64 hex chars
                  } else {
                      eprintln!("Invalid R6 register length: expected 32 bytes, got {}", tracker_nft_bytes.len());
                  }
              }
          },
          _ => eprintln!("R6 register does not contain a byte collection"),
      }
  }
  ```

#### 4.2 API Response Models
- Update API response models to include `tracker_nft_id` field
- Ensure proper serialization of R6 register information

### 5. Configuration Updates

#### 5.1 Tracker NFT ID Validation
- **File**: `crates/basis_server/src/config.rs`
- **Change**: Ensure configured tracker NFT ID matches R6 register values in reserve boxes
- **Validation**: Verify that reserve boxes have correct tracker NFT ID in R6 register
- **Serialization Comparison**: When validating tracker NFT ID, compare the 32-byte NFT ID provided by ergo-lib API directly with the configured tracker NFT ID
- **Format Validation**: Verify that the R6 register contains exactly 32 bytes when retrieved via ergo-lib API

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

## Deserialization Process

### 1. R6 Register Deserialization Steps
The process for deserializing the R6 register value follows these steps (note: ergo-lib automatically deserializes register values):

1. **Extract Register Value**: Retrieve the R6 register value from the box as a `Value` type
2. **Validate Type**: Confirm the value is a collection of bytes (`SColl(SByte)`)
3. **Convert to Bytes**: Extract the underlying byte array from the collection
4. **Validate Length**: Verify the byte array has exactly 32 bytes (the tracker NFT ID)
5. **Return Result**: Return the 32-byte NFT ID for comparison/validation

### 2. Implementation Example
```rust
use ergo_lib::chain::register::RegisterNumber;
use ergotree_ir::mir::value::Value;
use std::convert::TryInto;

fn deserialize_r6_register(box_with_bytes: &ErgoBox) -> Result<[u8; 32], String> {
    // Step 1: Extract R6 register from a box
    let register_value = box_with_bytes.get_register(RegisterNumber::R6)
        .ok_or("R6 register not found".to_string())?;
    
    // Step 2: Validate the value type
    let coll = match register_value.as_value() {
        Value::Coll(coll) => {
            if coll.elem_tpe != ergotree_ir::types::stype::SType::SByte {
                return Err("R6 register does not contain a byte collection".to_string());
            }
            coll
        },
        _ => return Err("R6 register does not contain a collection".to_string()),
    };
    
    // Step 3: Extract the bytes as a vector of i8
    let tracker_nft_bytes: Vec<i8> = coll.values.iter()
        .map(|v| v.clone().try_into().unwrap_or(0i8))
        .collect();
    
    // Step 4: Validate the length is 32 bytes (the tracker NFT ID)
    if tracker_nft_bytes.len() != 32 {
        return Err(format!("Invalid R6 register length: expected 32 bytes, got {}", tracker_nft_bytes.len()));
    }
    
    // Step 5: Convert to unsigned bytes and return as array
    let actual_tracker_nft_id: [u8; 32] = tracker_nft_bytes
        .iter()
        .map(|&b| b as u8)
        .collect::<Vec<u8>>()
        .try_into()
        .map_err(|_| "Failed to convert to 32-byte array".to_string())?;
    
    // Step 6: Return the extracted NFT ID
    Ok(actual_tracker_nft_id)
}
```

## Validation Requirements

### 1. Reserve Box Validation
- R6 register must contain a valid tracker NFT ID encoded as a byte array (SColl(SByte) in ErgoTree)
- When retrieved via ergo-lib API, the register value must be exactly 32 bytes (the tracker NFT ID)
- The 32 bytes represent the tracker NFT ID directly (ergo-lib deserializes automatically)
- R6 register value must match the configured tracker NFT ID
- Missing R6 register should result in appropriate error handling

### 2. Transaction Validation
- R6 register value must be preserved from input to output in redemption transactions
- R6 register in output box must match R6 register in input box
- Transaction validation should verify R6 register consistency
- The R6 register value must be properly serialized as a byte array (SColl(SByte)) using Sigma serialization:
  - When creating output boxes, the 32-byte tracker NFT ID should be properly serialized to register format
  - For 32-byte NFT IDs, the serialized format includes a VLQ length prefix (0x20) followed by 32 bytes of NFT ID

### 3. API Validation
- API responses must include R6 register value for all reserve endpoints
- R6 register value should be properly formatted as hex-encoded bytes representing the 32-byte tracker NFT ID (as provided by ergo-lib)
- The hex-encoded value should be exactly 64 hex characters for 32-byte NFT IDs (2 hex chars per byte)
- Error responses should handle cases where R6 register is missing

## Error Handling

### 1. Missing R6 Register
- If a reserve box is missing R6 register, log warning and skip processing
- API should return appropriate error when R6 register is expected but missing
- **Error Code**: `MISSING_R6_REGISTER`
- **Error Message**: "Reserve box {box_id} is missing required R6 register containing tracker NFT ID"
- **Action**: Skip processing the reserve box and continue with other boxes
- **Logging Level**: WARN

### 2. Invalid R6 Register Value
- If R6 register contains invalid tracker NFT ID format, log error and skip processing
- Validation should ensure R6 register contains proper format (ergo-lib provides deserialized values):
  - The register value should be exactly 32 bytes of tracker NFT ID
  - Total length should be 32 bytes (ergo-lib deserializes automatically)
- The hex-encoded value should be exactly 64 characters for 32-byte NFT IDs
- **Error Code**: `INVALID_R6_REGISTER_FORMAT`
- **Error Message**: "R6 register in box {box_id} has invalid format: {error_details}"
- **Action**: Skip processing the reserve box and continue with other boxes
- **Logging Level**: ERROR

### 3. Mismatched Tracker NFT ID
- If R6 register value doesn't match configured tracker NFT ID, log warning
- System should handle reserves with different tracker NFT IDs appropriately
- **Error Code**: `TRACKER_NFT_ID_MISMATCH`
- **Error Message**: "R6 register tracker NFT ID {actual_id} does not match expected {expected_id} in box {box_id}"
- **Action**: Skip processing the reserve box if strict validation is enabled, otherwise log warning and continue
- **Logging Level**: WARN or ERROR depending on configuration

### 4. Deserialization Errors
- If deserialization of R6 register fails due to corrupted data
- **Error Code**: `R6_DESERIALIZATION_FAILED`
- **Error Message**: "Failed to deserialize R6 register from box {box_id}: {error_details}"
- **Action**: Skip processing the reserve box and continue with other boxes
- **Logging Level**: ERROR

### 5. Serialization Errors
- If serialization of R6 register fails during transaction construction
- **Error Code**: `R6_SERIALIZATION_FAILED`
- **Error Message**: "Failed to serialize R6 register for output box in transaction: {error_details}"
- **Action**: Abort transaction construction and return error to caller
- **Logging Level**: ERROR

### 6. Validation Functions
- Implement dedicated validation functions for R6 register handling:

#### 6.1 Format Validation
- `validate_r6_register_format(value: &[u8]) -> Result<(), ValidationError>`: Validates the raw byte format
- **Purpose**: Verifies that the R6 register value (already deserialized by ergo-lib) contains exactly 32 bytes
- **Implementation**:
  ```rust
  fn validate_r6_register_format(value: &[u8]) -> Result<(), ValidationError> {
      // Check that the value contains exactly 32 bytes (the tracker NFT ID)
      if value.len() != 32 {
          return Err(ValidationError::InvalidLength(
              format!("Expected 32 bytes for tracker NFT ID, got {}", value.len())
          ));
      }

      Ok(())
  }
  ```

#### 6.2 Tracker NFT ID Matching Validation
- `validate_tracker_nft_id_match(configured_id: &[u8; 32], register_id: &[u8; 32]) -> Result<(), ValidationError>`: Validates ID matching
- **Purpose**: Compares the tracker NFT ID in the register with the configured tracker NFT ID
- **Implementation**:
  ```rust
  fn validate_tracker_nft_id_match(configured_id: &[u8; 32], register_id: &[u8; 32]) -> Result<(), ValidationError> {
      if configured_id != register_id {
          return Err(ValidationError::Mismatch(
              format!(
                  "Tracker NFT ID mismatch: configured={}, register={}",
                  hex::encode(configured_id),
                  hex::encode(register_id)
              )
          ));
      }
      Ok(())
  }
  ```

#### 6.3 Register to Hex Conversion
- `parse_r6_register_to_hex(value: &[u8]) -> Result<String, ValidationError>`: Converts register value to hex string
- **Purpose**: Converts the raw register bytes to a hex-encoded string for storage
- **Implementation**:
  ```rust
  fn parse_r6_register_to_hex(value: &[u8]) -> Result<String, ValidationError> {
      // Validate format first
      validate_r6_register_format(value)?;
      
      // Convert to hex string
      Ok(value.iter()
          .map(|b| format!("{:02x}", b))
          .collect::<String>())
  }
  ```

#### 6.4 Complete Validation Pipeline
- `validate_r6_register_complete(expected_tracker_id: &[u8; 32], register_value: &[u8]) -> Result<[u8; 32], ValidationError>`: Complete validation pipeline
- **Purpose**: Performs all validations in sequence and extracts the tracker NFT ID
- **Implementation**:
  ```rust
  fn validate_r6_register_complete(expected_tracker_id: &[u8; 32], register_value: &[u8]) -> Result<[u8; 32], ValidationError> {
      // Step 1: Validate format
      validate_r6_register_format(register_value)?;
      
      // Step 2: The register value is already the 32-byte tracker NFT ID (deserialized by ergo-lib)
      let extracted_id: [u8; 32] = register_value.try_into()
          .map_err(|_| ValidationError::InvalidLength("Could not convert slice to array".to_string()))?;
      
      // Step 3: Validate ID match
      validate_tracker_nft_id_match(expected_tracker_id, &extracted_id)?;
      
      // Step 4: Return the validated ID
      Ok(extracted_id)
  }
  ```

## Integration Points

### 1. Ergo Node Integration
- Reserve scanner must properly parse R6 register from Ergo node API responses
- The R6 register value from Ergo node is automatically deserialized by ergo-lib to the raw 32-byte tracker NFT ID
- Transaction builder must format R6 register correctly for Ergo node transaction API, serializing the 32-byte tracker NFT ID to proper register format

### 2. Tracker Integration
- R6 register in reserve boxes must match the tracker NFT ID used by the tracker system
- Cross-verification between tracker commitments and reserve boxes should use R6 register information from reserve boxes

### 3. Client Integration
- API responses should provide R6 register information to clients
- Clients should be able to use R6 register information to verify tracker associations

## Testing Strategy

### 1. Unit Tests
- Test R6 register parsing from reserve box data (ergo-lib provides deserialized 32-byte values)
- Test R6 register preservation in transaction building with proper Sigma serialization
- Test R6 register validation logic for correct 32-byte format
- Test extraction of tracker NFT ID from register values provided by ergo-lib

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