# Group Point Register Serialization Specification

## Overview

This document specifies how group points are serialized to and from registers in the Ergo blockchain protocol. Group points (represented as `GroupElement` in ErgoTree) are elements of an elliptic curve group, commonly used for cryptographic operations like digital signatures and Diffie-Hellman key exchanges.

## Data Structure Definition

### Group Points in ErgoTree
- **Type**: `SGroupElement` 
- **Representation**: Elliptic curve point in compressed or uncompressed form
- **Curve**: Typically secp256k1 (same as Bitcoin)
- **Usage**: Used for cryptographic operations, public keys, and zero-knowledge proofs

### Group Point Structure
- **Format**: Compressed format (33 bytes) or uncompressed format (65 bytes)
- **Compressed**: 1-byte prefix (0x02 or 0x03) + 32-byte x-coordinate
- **Uncompressed**: 1-byte prefix (0x04) + 32-byte x-coordinate + 32-byte y-coordinate
- **Infinity**: Special representation for the point at infinity

## Serialization Format

The serialization of group points follows the Sigma serialization protocol:

### Serialized Fields Order
1. **point data** (33 or 65 bytes): The compressed or uncompressed point representation

### Detailed Serialization Steps

1. **Point Encoding**:
   - Points are serialized in compressed format by default (33 bytes)
   - Compressed format: 0x02 if y-coordinate is even, 0x03 if odd, followed by x-coordinate
   - Uncompressed format: 0x04 prefix followed by x and y coordinates (65 bytes total)

2. **Validation**:
   - The serialized bytes must represent a valid point on the curve
   - Validation occurs during deserialization

## Working with Registers

### Serializing Group Points to Registers

To serialize a group point value into a register:

1. **Create a GroupElement**: Initialize with the required point data
2. **Convert to ErgoTree value**: Transform to `SGroupElement` type
3. **Store in register**: Place the serialized point in the appropriate register

Rust code example:
```rust
use ergo_lib::chain::register::NonMandatoryRegisters;
use std::collections::HashMap;
use ergotree_ir::mir::value::Value;
use k256::elliptic_curve::{AffinePoint, ProjectivePoint};
use k256::Secp256k1;

// Create a group element (elliptic curve point)
// This example uses a sample public key point
let x_bytes = [0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b, 0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8, 0x17, 0x98]; // Sample x coordinate
let y_bytes = [0x48, 0x3a, 0xda, 0x77, 0x26, 0xa3, 0xc4, 0x65, 0x5d, 0xa4, 0xfb, 0xfc, 0x0e, 0x11, 0x08, 0xa8, 0xfd, 0x17, 0xb4, 0x48, 0xa6, 0x85, 0x54, 0x19, 0x9c, 0x47, 0xd0, 0x8f, 0xfb, 0x10, 0xd4, 0xb8]; // Sample y coordinate

// Construct the affine point
let point_affine = k256::AffinePoint::from_xy(
    k256::FieldElement::from_bytes(&x_bytes.into()),
    k256::FieldElement::from_bytes(&y_bytes.into()),
).unwrap();

// Convert to ErgoTree value
let group_element_value = Value::from(point_affine);

// In practice, when using ergo-lib to construct a transaction:
use ergo_lib::chain::register::RegisterNumber;

// Create registers map with group element in register R4
let mut registers_map = HashMap::new();
registers_map.insert(RegisterNumber::R4, group_element_value.into());
let registers = NonMandatoryRegisters::from_map(registers_map);

// The group element value can now be stored in a box when building a transaction
// using ergo-lib's transaction builder APIs
```

### Deserializing Group Points from Registers

To deserialize a group point value from a register:

1. **Extract register value**: Get the value from the register
2. **Deserialize using ergo-lib APIs**: Access the group element from the register value
3. **Access point properties**: Use the deserialized structure

Rust code example:
```rust
use ergo_lib::chain::register::RegisterNumber;

// Example of reading a group element from a box register using ergo-lib:
// let box_with_point = /* get box from blockchain */;
// if let Some(register_value) = box_with_point.get_register(RegisterNumber::R4) {
//     if let ergo_lib::ergotree_ir::mir::value::Value::GroupElement(group_elem) = register_value.as_value() {
//         // Access properties of the group element
//         let point_bytes = group_elem.to_compressed_vec(); // Get compressed representation
//         
//         println!("Group element compressed bytes: {:?}", point_bytes);
//         println!("Group element size: {} bytes", point_bytes.len());
//     }
// }
```

## References

- ErgoTree IR specification
- Sigma serialization protocol
- Register usage in Ergo
- SEC 1: Elliptic Curve Cryptography