# Ergo P2PK Address Derivation API Specification

## Overview

This document specifies the user-facing API for deriving Pay-to-Public-Key (P2PK) addresses from elliptic curve points in the Ergo blockchain protocol. P2PK addresses represent the most basic form of Ergo address, directly encoding a public key that can be used to receive funds.

## API Functions

### Creating P2PK Address from Public Key

#### `Address::p2pk_from_pk(public_key) -> Address`
Creates a P2PK address from a public key.

**Parameters:**
- `public_key`: A `PublicKey` object (compressed format preferred)

**Returns:**
- `Address` object of type P2PK

**Example:**
```rust
use ergo_lib::chain::address::Address;
use k256::PublicKey;

// Given a public key
let public_key: PublicKey = /* obtained from key generation */;

// Create P2PK address
let p2pk_address = Address::p2pk_from_pk(public_key);
```

#### `AddressEncoder::address_to_str(address) -> String`
Encodes an address to a human-readable string.

**Parameters:**
- `address`: An `Address` object

**Returns:**
- `String` containing the Base58-encoded address with network prefix

**Example:**
```rust
use ergo_lib::chain::address::{AddressEncoder, NetworkPrefix};

let encoder = AddressEncoder::new(NetworkPrefix::Mainnet); // or Testnet
let address_string = encoder.address_to_str(&p2pk_address);
println!("Address: {}", address_string); // e.g., "2XXX..."
```

### Verifying Address-Public Key Correspondence

#### `AddressEncoder::str_to_address(address_str) -> Result<Address, Error>`
Decodes a string address to an Address object.

**Parameters:**
- `address_str`: String containing the Base58-encoded address

**Returns:**
- `Ok(Address)` on successful decoding
- `Err(Error)` on invalid format

**Example:**
```rust
use ergo_lib::chain::address::{AddressEncoder, NetworkPrefix};

let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
let address = encoder.str_to_address("2XXX...")?;
```

#### `Address::p2pk_pubkey_hash() -> Option<PubKeyHash>`
Extracts the public key hash from a P2PK address.

**Returns:**
- `Some(PubKeyHash)` if the address is P2PK type
- `None` if the address is not P2PK type

**Example:**
```rust
use ergo_lib::ergotree_ir::chain::address::Address;

if let Address::P2Pk(p2pk_addr) = &address {
    let pubkey_hash = &p2pk_addr.pubkey_hash;
    // Use the hash for verification
}
```

### Address Type Checking

#### `Address::address_type() -> AddressType`
Returns the type of the address.

**Returns:**
- `AddressType::P2Pk` for P2PK addresses
- Other types for different address kinds

**Example:**
```rust
use ergo_lib::ergotree_ir::chain::address::AddressType;

match address.address_type() {
    AddressType::P2Pk => println!("P2PK address"),
    AddressType::P2Sh => println!("P2SH address"),
    AddressType::Pay2S => println!("Pay-to-script address"),
    _ => println!("Unknown address type"),
}
```

## Complete Usage Examples

### Creating an Address from a Public Key
```rust
use ergo_lib::chain::address::{AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::chain::address::Address;
use k256::PublicKey;

// Assume we have a public key
let public_key: PublicKey = /* from key generation */;

// Create P2PK address from public key
let p2pk_address = Address::p2pk_from_pk(public_key);

// Encode to string for display/sharing
let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
let address_string = encoder.address_to_str(&p2pk_address);

println!("Generated address: {}", address_string);
```

### Verifying Address-Public Key Match
```rust
use ergo_lib::chain::address::{AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::chain::address::Address;
use k256::PublicKey;
use sigma_util::hash::blake2b160_hash;

// Given an address string and a public key
let address_str = "2XXX..."; // P2PK address
let public_key: PublicKey = /* public key to verify */;

// Decode the address
let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
let address = encoder.str_to_address(address_str)?;

// Check if it's a P2PK address
if let Address::P2Pk(p2pk_addr) = address {
    // Get the compressed public key bytes
    let compressed_pk_bytes = public_key.to_bytes();

    // Hash the public key
    let computed_hash = blake2b160_hash(&compressed_pk_bytes.as_slice());

    // Compare with address hash
    if computed_hash.as_slice() == p2pk_addr.pubkey_hash.as_slice() {
        println!("Public key matches the address");
    } else {
        println!("Public key does not match the address");
    }
} else {
    println!("Address is not a P2PK address");
}
```

### Address Validation
```rust
use ergo_lib::chain::address::{AddressEncoder, NetworkPrefix};

fn validate_address(address_str: &str, expected_network: NetworkPrefix) -> Result<bool, Box<dyn std::error::Error>> {
    let encoder = AddressEncoder::new(expected_network);
    let address = encoder.str_to_address(address_str)?;

    // Check if it's a P2PK address
    Ok(matches!(address.address_type(), ergo_lib::ergotree_ir::chain::address::AddressType::P2Pk))
}

// Usage
let is_valid = validate_address("2XXX...", NetworkPrefix::Mainnet)?;
```

## Network Support


### Setting Network
```rust
use ergo_lib::chain::address::NetworkPrefix;

let mainnet_encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
let testnet_encoder = AddressEncoder::new(NetworkPrefix::Testnet);
```

## Error Handling

### Common Error Types
- `InvalidAddress`: Address string has invalid format
- `InvalidChecksum`: Address checksum verification failed
- `UnsupportedAddressType`: Address type not supported by the operation

### Error Handling Example
```rust
use ergo_lib::chain::address::{AddressEncoder, NetworkPrefix};

let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
match encoder.str_to_address("invalid_address_string") {
    Ok(address) => {
        // Process valid address
        println!("Valid address: {:?}", address.address_type());
    }
    Err(e) => {
        eprintln!("Invalid address: {}", e);
    }
}
```


## References

- Ergo blockchain protocol specification
- ergo-lib documentation
- k256 elliptic curve library documentation