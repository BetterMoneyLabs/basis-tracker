# Basis Offchain Crate Specification

## Overview

The `basis_offchain` crate implements the off-chain logic for the Basis protocol, which enables off-chain payments with on-chain redemption capabilities. This crate handles transaction building, Schnorr signature operations, and all off-chain functionality required for the Basis system.

## Core Components

### 1. Transaction Builder Module

The `transaction_builder` module contains the core logic for creating redemption transactions that interact with the Basis reserve contract on the Ergo blockchain.

#### Key Structures

- **`TxContext`**: Contains blockchain context information including:
  - Current blockchain height
  - Transaction fee (in nanoERG)
  - Change address
  - Network prefix (0 for mainnet, 16 for testnet)

- **`RedemptionTransactionData`**: Complete transaction data structure containing:
  - Reserve box ID being spent
  - Tracker box ID as data input
  - Redemption amount
  - Recipient address
  - AVL proof bytes
  - Issuer signature (65-byte Schnorr signature)
  - Tracker signature (65-byte Schnorr signature)
  - Transaction fee

- **`RedemptionTransactionBuilder`**: Provides methods to prepare, validate, and build redemption transactions

#### Key Functions

- **`prepare_redemption_transaction`**: Validates parameters and prepares transaction structure
- **`validate_redemption_parameters`**: Ensures sufficient collateral and time lock expiration
- **`build_redemption_transaction`**: Creates actual Ergo transaction (when blockchain integration is complete)
- **`create_mock_transaction_bytes`**: Creates human-readable transaction representation for testing

### 2. Schnorr Signature Module

The `schnorr` module implements Schnorr signature operations following the chaincash-rs approach.

#### Key Types

- **`PubKey`**: 33-byte compressed secp256k1 public key
- **`Signature`**: 65-byte Schnorr signature (33 bytes for 'a' component + 32 bytes for 'z' component)

#### Key Functions

- **`schnorr_sign`**: Creates Schnorr signatures using secret key
- **`schnorr_verify`**: Verifies Schnorr signatures against public key and message
- **`signing_message`**: Creates signing messages by concatenating (recipient_pubkey || amount || timestamp)
- **`generate_keypair`**: Generates new key pairs for testing/development
- **`validate_signature_format`**: Ensures signature has correct 65-byte format
- **`validate_public_key`**: Validates compressed secp256k1 public keys

### 3. Error Handling

The crate defines comprehensive error types:

- **`TransactionBuilderError`**: Handles transaction building, insufficient funds, and configuration errors
- **`NoteError`**: Handles signature validation, amount overflow, timestamp, redemption timing, collateral, and storage errors

## Functionality

### Redemption Process

1. **Preparation**: The system prepares redemption transaction data using `prepare_redemption_transaction`
2. **Validation**: Parameters are validated to ensure sufficient collateral and proper time lock expiration
3. **Proof Generation**: AVL proofs are created to prove debt exists in the tracker's state
4. **Signature Collection**: Both issuer and tracker signatures are collected
5. **Transaction Building**: Final transaction is built with all required components

### Time Lock Validation

The system enforces a minimum 1-week (7 days) time lock before redemption can occur. This prevents immediate redemption and gives the tracker time to update the state.

### Fee Management

The transaction builder follows the suggested transaction fee pattern from chaincash-rs, with a default fee of 0.001 ERG (1,000,000 nanoERG).

## Testing

The crate includes comprehensive test coverage:
- Transaction building with various amounts and fees
- Parameter validation scenarios
- Error condition testing
- Schnorr signature generation and verification
- Integration with test helper functions

## Dependencies

- `ergo-lib`: Ergo blockchain library integration
- `secp256k1`: Elliptic curve cryptography for signatures
- `blake2`: Cryptographic hashing (Blake2b)
- `serde`: Serialization/deserialization
- `thiserror`: Error handling
- `num-bigint`: Big integer arithmetic for signature operations
- `rand`: Random number generation for nonces
- `hex`: Hexadecimal encoding/decoding

## Integration Points

The offchain crate is designed to work with:
- Basis smart contracts (on-chain redemption)
- Tracker servers (state management)
- AVL trees (debt verification)
- Ergo blockchain (transaction submission)
- Wallet systems (key management)