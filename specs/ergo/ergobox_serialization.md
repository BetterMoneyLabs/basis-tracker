# ErgoBox Serialization Specification

## Overview

This document specifies how ErgoBoxes are serialized to and from byte arrays in the Ergo blockchain protocol. ErgoBoxes represent state elements in the UTXO model, containing value, tokens, registers, and smart contracts that define spending conditions.

## Data Structure Definition

### ErgoBox Structure
An ErgoBox contains the following components:

- **value**: Amount of NanoErgs (64-bit unsigned integer)
- **ergo_tree**: The guarding script that defines spending conditions (serialized ErgoTree)
- **registers**: Optional non-mandatory registers (R4-R9) containing additional data
- **tokens**: Optional list of tokens associated with the box
- **creation_height**: Block height when the box was created (32-bit unsigned integer)
- **transaction_id**: ID of the transaction that created the box (32-byte digest)
- **index**: Index of the box in the transaction's outputs (16-bit unsigned integer)

## Serialization Format

The serialization of ErgoBox follows the Sigma serialization protocol:

### Serialized Fields Order
1. **value** (8 bytes): Amount in NanoErgs as a 64-bit unsigned integer (little-endian)
2. **ergo_tree** (variable): Serialized ErgoTree script
3. **registers** (variable): Optional register values (may be empty)
4. **tokens** (variable): Optional list of tokens (may be empty)
5. **creation_height** (4 bytes): Creation height as a 32-bit unsigned integer (little-endian)
6. **transaction_id** (32 bytes): Transaction ID as a 32-byte digest
7. **index** (2 bytes): Output index as a 16-bit unsigned integer (little-endian)

### Detailed Serialization Steps

1. **Value Serialization**:
   - The `value` field is serialized as an 8-byte little-endian unsigned integer
   - Represents the amount in NanoErgs (1 Erg = 10^9 NanoErgs)

2. **ErgoTree Serialization**:
   - The guarding script is serialized using the ErgoTree serialization format
   - Contains the proposition that must be proven to spend the box

3. **Registers Serialization**:
   - Non-mandatory registers (R4-R9) are serialized as a collection
   - Each register is identified by its index and contains a serialized value
   - Empty if no registers are present

4. **Tokens Serialization**:
   - Tokens are serialized as a collection of (ID, amount) pairs
   - Each token ID is a 32-byte digest
   - Each amount is a 64-bit unsigned integer
   - Empty if no tokens are present

5. **Creation Height Serialization**:
   - Serialized as a 4-byte little-endian unsigned integer

6. **Transaction ID Serialization**:
   - Serialized as a 32-byte digest (Blake2b hash)

7. **Index Serialization**:
   - Serialized as a 2-byte little-endian unsigned integer

## Serialization and Deserialization Operations

### Serializing ErgoBox to Bytes

To serialize an ErgoBox to bytes:

1. **Prepare the ErgoBox**: Ensure all fields are properly initialized
2. **Serialize using Sigma protocol**: Convert the ErgoBox to bytes
3. **Obtain byte array**: The resulting byte array represents the serialized box

Rust code example:
```rust
use ergo_lib::chain::box_wrapper::ErgoBox;
use ergo_lib::serialization::SigmaSerializable;
use sigma_ser::serializer::SigmaSerializeResult;

// Assuming we have an ErgoBox instance
// let ergo_box: ErgoBox = /* obtain from blockchain or construct */;

// Serialize the ErgoBox to bytes
// let serialized_bytes: Vec<u8> = ergo_box.sigma_serialize_bytes().unwrap();

// The serialized bytes can be stored, transmitted, or processed
// println!("Serialized box size: {} bytes", serialized_bytes.len());
```

### Deserializing ErgoBox from Bytes

To deserialize an ErgoBox from bytes:

1. **Obtain serialized bytes**: Get the byte array containing the serialized box
2. **Deserialize using Sigma protocol**: Convert bytes back to ErgoBox
3. **Validate the box**: Ensure the deserialized box is valid

Rust code example:
```rust
use ergo_lib::serialization::SigmaParsingResult;
use ergo_lib::chain::box_wrapper::ErgoBox;

// Assuming we have serialized bytes
// let serialized_bytes: Vec<u8> = /* obtained from storage or transmission */;

// Deserialize the bytes back to an ErgoBox
// let deserialized_box: ErgoBox = ErgoBox::sigma_parse_bytes(&serialized_bytes).unwrap();

// Access properties of the deserialized box
// println!("Box value: {} NanoErgs", deserialized_box.value().as_u64());
// println!("Box creation height: {}", deserialized_box.creation_height());
// println!("Number of tokens: {}", deserialized_box.tokens().len());
```

## References

- Ergo blockchain protocol specification
- ErgoTree serialization specification
- UTXO model documentation
- Sigma serialization protocol