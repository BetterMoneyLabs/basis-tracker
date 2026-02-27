# Basis Offchain Crate Specification

## Overview

The `basis_offchain` crate implements the off-chain logic for the Basis protocol, which enables off-chain payments with on-chain redemption capabilities. This crate handles transaction building, Schnorr signature operations, AVL tree proof generation, and all off-chain functionality required for the Basis system.

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
  - AVL proof bytes (for both reserve and tracker trees)
  - Issuer signature (65-byte Schnorr signature)
  - Tracker signature (65-byte Schnorr signature)
  - Transaction fee
  - Total debt amount
  - Already redeemed amount (if any)

- **`RedemptionTransactionBuilder`**: Provides methods to prepare, validate, and build redemption transactions

#### Key Functions

- **`build_unsigned_redemption_transaction`**: Creates unsigned Ergo transaction with full validation
- **`build_redemption_transaction`**: Creates actual Ergo transaction with all required context extensions
- **`prepare_redemption_context`**: Prepares context extension variables for redemption:
  - #1: Receiver pubkey (GroupElement)
  - #2: Reserve owner's signature bytes
  - #3: Total debt amount (Long)
  - #5: AVL proof for reserve tree insertion
  - #6: Tracker's signature bytes
  - #7: AVL proof for reserve tree lookup (optional, omit for first redemption)
  - #8: AVL proof for tracker tree lookup (required)

### 2. Schnorr Signature Module

The `schnorr` module implements Schnorr signature operations following the Ergo blockchain standards.

#### Key Types

- **`PubKey`**: 33-byte compressed secp256k1 public key
- **`Signature`**: 65-byte Schnorr signature (33 bytes for 'a' component + 32 bytes for 'z' component)

#### Key Functions

- **`schnorr_sign`**: Creates Schnorr signatures using secret key
- **`schnorr_verify`**: Verifies Schnorr signatures against public key and message
- **`signing_message`**: Creates signing messages by concatenating `(key || totalDebt)` or `(key || totalDebt || 0L)` for emergency redemption
  - Where `key = blake2b256(ownerKeyBytes || receiverBytes)`
- **`generate_keypair`**: Generates new key pairs for testing/development
- **`validate_signature_format`**: Ensures signature has correct 65-byte format
- **`validate_public_key`**: Validates compressed secp256k1 public keys

### 3. AVL Proof Module

The `avl_proof` module handles AVL tree proof generation and verification for both tracker and reserve trees.

#### Key Functions

- **`generate_tracker_lookup_proof`**: Generates proof for looking up `hash(ownerKey||receiverKey) -> totalDebt` in tracker's AVL tree (context var #8)
- **`generate_reserve_lookup_proof`**: Generates proof for looking up `hash(ownerKey||receiverKey) -> redeemedDebt` in reserve's AVL tree (context var #7, optional)
- **`generate_reserve_insert_proof`**: Generates proof for inserting updated redeemed amount into reserve's AVL tree (context var #5)
- **`verify_tracker_commitment`**: Verifies that totalDebt matches the value committed in tracker's AVL tree

### 4. Error Handling

The crate defines comprehensive error types:

- **`TransactionBuilderError`**: Handles transaction building, insufficient funds, and configuration errors
- **`NoteError`**: Handles signature validation, amount overflow, timestamp, redemption timing, collateral, and storage errors
- **`AvlProofError`**: Handles AVL tree proof generation and verification errors

## Functionality

### Redemption Process

1. **Validation**: Parameters are validated to ensure sufficient collateral and proper time lock expiration
2. **Tracker Lookup**: Query tracker for totalDebt using `hash(ownerKey||receiverKey)`
3. **Reserve Lookup**: Query reserve's AVL tree for already redeemed amount (if any)
4. **Proof Generation**: 
   - AVL proof for tracker tree lookup (context var #8, required)
   - AVL proof for reserve tree lookup (context var #7, optional for first redemption)
   - AVL proof for reserve tree insertion (context var #5)
5. **Signature Collection**: 
   - Reserve owner's signature on `key || totalDebt` (or `key || totalDebt || 0L` for emergency)
   - Tracker's signature on `key || totalDebt` (or `key || totalDebt || 0L` for emergency)
6. **Transaction Building**: Final transaction is built with all required components including context extensions

### Emergency Redemption

- If tracker becomes unavailable, emergency redemption is possible after 3 days (3 * 720 blocks) from tracker creation
- Emergency redemption uses modified message format: `key || totalDebt || 0L`
- Both signatures are still required, but tracker signature verification is bypassed after timeout
- **NOTE**: All debts associated with this tracker become eligible for emergency redemption simultaneously after 3 days

### Time Lock Validation

The system enforces a minimum time lock before redemption can occur. For emergency redemption, the time lock is 3 days from tracker creation.

### Fee Management

The transaction builder follows the suggested transaction fee pattern, with a default fee of 0.001 ERG (1,000,000 nanoERG).

## Testing

The crate includes comprehensive test coverage:
- Transaction building with various amounts and fees
- Parameter validation scenarios
- Error condition testing
- Schnorr signature generation and verification
- AVL proof generation and verification
- Emergency redemption scenarios
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
- Tracker servers (state management and signatures)
- AVL trees (debt verification and proofs)
- Ergo blockchain (transaction submission)
- Wallet systems (key management)
- Ergo node API (Schnorr signature generation via `/utils/schnorrSign`)

## Context Extension Variables Reference

### For Redemption (Action #0)

| Variable | Type | Description | Required |
|----------|------|-------------|----------|
| #0 | Byte | Action byte (0x00 for redemption) | Yes |
| #1 | GroupElement | Receiver pubkey | Yes |
| #2 | Coll[Byte] | Reserve owner's signature bytes | Yes |
| #3 | Long | Total debt amount | Yes |
| #5 | Coll[Byte] | AVL proof for reserve tree insertion | Yes |
| #6 | Coll[Byte] | Tracker's signature bytes | Yes |
| #7 | Coll[Byte] | AVL proof for reserve tree lookup | No (optional, omit for first redemption) |
| #8 | Coll[Byte] | AVL proof for tracker tree lookup | Yes |

### Message Format for Signatures

**Normal Redemption:**
```
message = key || longToByteArray(totalDebt)
where key = blake2b256(ownerKeyBytes || receiverBytes)
```

**Emergency Redemption (after 3 days):**
```
message = key || longToByteArray(totalDebt) || longToByteArray(0L)
```

## Tracker Integration

The offchain crate integrates with tracker servers to:
- Obtain totalDebt values for (owner, receiver) pairs
- Request tracker signatures for redemption authorization
- Fetch AVL tree proofs for tracker state verification
- Verify tracker commitments against on-chain state

## Reserve Contract Interaction

The offchain crate interacts with the reserve contract by:
- Building transactions that satisfy all spending conditions
- Providing AVL proofs for reserve tree operations
- Ensuring proper update of redeemed debt tracking
- Handling both first-time and subsequent redemptions
