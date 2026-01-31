# Byte Array Register Serialization Specification

## Overview

This document specifies how byte arrays are serialized to and from registers in the Ergo blockchain protocol. Byte arrays (represented as `Coll[Byte]` in ErgoTree) are commonly stored in box registers to persist data between transactions.

## Data Structure Definition

### Byte Arrays in ErgoTree
- **Type**: `SColl(SByte)` (collection of bytes)
- **Representation**: Variable-length sequence of 8-bit signed integers
- **Usage**: Commonly used for storing binary data, hashes, signatures, or serialized objects

## Serialization Format

The serialization of byte arrays follows the Sigma serialization protocol for collections:

### Serialized Fields Order
1. **length** (variable): The number of elements in the collection, encoded as VLQ (Variable-Length Quantity)
2. **elements** (variable): Individual byte values, each as a signed 8-bit integer

### Detailed Serialization Steps

1. **Length Encoding**:
   - The count of elements is serialized using VLQ encoding
   - For arrays up to 127 bytes: 1 byte
   - For larger arrays: multiple bytes using VLQ format

2. **Element Serialization**:
   - Each byte is serialized as an 8-bit signed integer
   - Values range from -128 to 127
   - Elements are stored consecutively in order

## Working with Registers

### Serializing Byte Arrays to Registers

To serialize a byte array value into a register:

1. **Create a byte vector**: Initialize with the required byte data
2. **Convert to ErgoTree collection**: Transform to `SColl(SByte)` type
3. **Store in register**: Place the serialized bytes in the appropriate register

Rust code example:
```rust
use ergo_lib::chain::register::NonMandatoryRegisters;
use std::collections::HashMap;
use ergotree_ir::mir::value::Value;
use ergotree_ir::types::stype::SType;
use sigma_ser::serializer::SigmaSerializable;

// Create a byte array
let byte_data: Vec<i8> = vec![0x45, 0x72, 0x67, 0x6F]; // "Ergo" as ASCII bytes

// Convert to ErgoTree value
let byte_array_value = Value::from(byte_data);

// In practice, when using ergo-lib to construct a transaction:
use ergo_lib::chain::register::RegisterNumber;

// Create registers map with byte array in register R4
let mut registers_map = HashMap::new();
registers_map.insert(RegisterNumber::R4, byte_array_value.into());
let registers = NonMandatoryRegisters::from_map(registers_map);

// The byte array value can now be stored in a box when building a transaction
// using ergo-lib's transaction builder APIs
```

### Deserializing Byte Arrays from Registers

To deserialize a byte array value from a register:

1. **Extract register value**: Get the value from the register
2. **Deserialize using ergo-lib APIs**: Access the byte array from the register value
3. **Access array properties**: Use the deserialized structure

Rust code example:
```rust
use ergo_lib::chain::register::RegisterNumber;

// Example of reading a byte array from a box register using ergo-lib:
// let box_with_bytes = /* get box from blockchain */;
// if let Some(register_value) = box_with_bytes.get_register(RegisterNumber::R4) {
//     if let ergo_lib::ergotree_ir::mir::value::Value::Coll(coll) = register_value.as_value() {
//         // Access properties of the byte array
//         let byte_count = coll.values.len();
//         let bytes: Vec<i8> = coll.values.iter().map(|v| *v).collect();
//         
//         println!("Byte array length: {}", byte_count);
//         println!("Bytes: {:?}", bytes);
//     }
// }
```

## References

- ErgoTree IR specification
- Sigma serialization protocol
- Register usage in Ergo