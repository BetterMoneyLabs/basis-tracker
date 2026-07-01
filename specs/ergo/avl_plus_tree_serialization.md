# AVL+ Tree Serialization Specification

## Overview

This document specifies the serialization format for AVL+ trees in the Ergo blockchain protocol. AVL+ trees are authenticated data structures used to efficiently authenticate potentially huge datasets with a key-value dictionary interface. The implementation in sigma-rust represents these trees with only the root hash, tree height, key length, optional value length, and access flags stored in the data structure.

## Data Structure Definition

### AvlTreeData
The core data structure representing an AVL+ tree is defined as `AvlTreeData`:

```rust
pub struct AvlTreeData {
    pub digest: ADDigest,                 // Authenticated tree digest: root hash along with tree height
    pub tree_flags: AvlTreeFlags,        // Allowed modifications flags
    pub key_length: u32,                 // All elements under the tree have the same key length
    pub value_length_opt: Option<Box<u32>>, // If non-empty, all values under the tree are of the same length
}
```

### ADDigest
- Type: `Digest<33>` (33-byte array)
- Purpose: Stores the root hash of the AVL+ tree along with the tree height (33 bytes total)
- The first 32 bytes represent the root hash (Blake2b hash)
- The 33rd byte represents the tree height

### AvlTreeFlags
- Type: `u8` (single byte)
- Purpose: Encodes allowed modification operations on the tree
- Bit layout:
  - Bit 0 (0x01): Insert allowed flag
  - Bit 1 (0x02): Update allowed flag
  - Bit 2 (0x04): Remove allowed flag
  - Bits 3-7: Reserved for future use

## Serialization Format

The serialization of `AvlTreeData` follows the Sigma serialization protocol:

### Serialized Fields Order
1. **type byte** (1 byte): The `SAvlTree` type identifier `0x64`
2. **digest** (33 bytes): The `ADDigest` (root hash + tree height)
3. **tree_flags** (1 byte): The `AvlTreeFlags` as a single byte
4. **key_length** (VLQ): The key length encoded as a Variable-Length Quantity. For the common case of 32-byte keys this is a single byte `0x20`.
5. **value_length_opt** (variable): Optional value length serialized as `Option<Box<u32>>`:
   - `None` is serialized as a single byte `0x00` (variable value length)
   - `Some(n)` is serialized as `0x01` followed by the 4-byte little-endian length

### Total Size for Tracker Trees

For a tracker AVL tree configured with `PlasmaParameters(32, None)` (32-byte keys, variable values):
- Type byte: 1 byte
- Digest: 33 bytes
- Flags: 1 byte
- Key length VLQ: 1 byte
- Value length option (`None`): 1 byte
- **Total: 37 bytes** (74 hex characters)

### Detailed Serialization Steps

1. **Type Identifier Serialization**:
   - The Sigma constant begins with the `SAvlTree` type byte `0x64`

2. **Digest Serialization**:
   - The `ADDigest` is serialized using Scorex serialization protocol
   - Results in 33 bytes (32 bytes for hash + 1 byte for height)

3. **Flags Serialization**:
   - The `AvlTreeFlags` is serialized as a single byte
   - The byte value is the raw internal value of the flags

4. **Key Length Serialization**:
   - The `key_length` is serialized using VLQ encoding
   - For values ≤ 127 this is a single byte
   - Example: 32-byte keys serialize as `0x20`

5. **Value Length Option Serialization**:
   - If `value_length_opt` is `Some(value)`:
     - First byte: `0x01` (indicating Some)
     - Following 4 bytes: The value as a 32-bit unsigned integer in little-endian format
   - If `value_length_opt` is `None`:
     - Single byte: `0x00` (indicating None / variable values)

### Example Values

Empty tree serialized as `SAvlTree` constant:
```
64000000000000000000000000000000000000000000000000000000000000000000012000
```

Real tracker tree example (digest: `d5d44e...6a01`):
```
64d5d44e152c7e42673dea178b918d9195c2ba689da94046384dc40c55a64c836a01012000
```

## Type System Integration

### SType
- **Type Code**: 100 (0x64 in hex)
- **Purpose**: Represents AVL+ tree type in the ErgoTree type system

## Working with Registers

### Serializing AVL+ Tree Values to Registers

To serialize an AVL+ tree value into a register:

1. **Create an AvlTreeData instance**: Initialize with the required parameters
2. **Serialize using Sigma serialization**: Convert the AvlTreeData to bytes
3. **Store in register**: Place the serialized bytes in the appropriate register

Rust code example:
```rust
use ergotree_ir::mir::avl_tree_data::{AvlTreeData, AvlTreeFlags};
use ergotree_ir::serialization::sigma_serialize;
use ergo_chain_types::ADDigest;
use sigma_ser::ScorexSerializable;
use ergotree_ir::types::stype::SType;

// Create an AVL+ tree with specific parameters
let avl_tree_data = AvlTreeData {
    digest: ADDigest::zero(), // 33-byte digest with root hash and height
    tree_flags: AvlTreeFlags::new(true, true, false), // insert/update allowed, remove disallowed
    key_length: 32, // 32-byte keys
    value_length_opt: Some(Box::new(64)), // 64-byte values (optional)
};

// Convert to a Value type for use in ergo-lib
let avl_tree_value: ergotree_ir::mir::value::Value = avl_tree_data.into();

// In practice, when using ergo-lib to construct a transaction:
use ergo_lib::chain::box::ErgoBox;
use ergo_lib::chain::register::NonMandatoryRegisters;
use std::collections::HashMap;

// Create registers map with AVL tree in register R4
let mut registers_map = HashMap::new();
registers_map.insert(ergo_lib::chain::register::RegisterNumber::R4, avl_tree_value);
let registers = NonMandatoryRegisters::from_map(registers_map);

// The AVL tree value can now be stored in a box when building a transaction
// using ergo-lib's transaction builder APIs
```

### Deserializing AVL+ Tree Values from Registers

To deserialize an AVL+ tree value from a register:

1. **Extract serialized bytes**: Get the bytes from the register
2. **Deserialize using ergo-lib APIs**: Access the AVL+ tree from the register value
3. **Access tree properties**: Use the deserialized structure

Rust code example:
```rust
use ergo_lib::chain::register::RegisterNumber;

// Example of reading an AVL tree from a box register using ergo-lib:
// let box_with_avl = /* get box from blockchain */;
// if let Some(register_value) = box_with_avl.get_register(RegisterNumber::R4) {
//     if let ergo_lib::ergotree_ir::mir::value::Value::AvlTree(tree_data) = register_value.as_value() {
//         Access properties of the AVL+ tree
//         let digest = &tree_data.digest;
//         let flags = &tree_data.tree_flags;
//         let key_length = tree_data.key_length;
//         let value_length = &tree_data.value_length_opt;
//
//         println!("Tree digest: {:?}", digest);
//         println!("Insert allowed: {}", flags.insert_allowed());
//         println!("Key length: {}", key_length);
//     }
// }
```


## References

- ErgoTree IR specification
- Sigma serialization protocol
- Register usage in Ergo