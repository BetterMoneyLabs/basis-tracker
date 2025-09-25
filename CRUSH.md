# Basis Tracker Development Guide

## Build & Test Commands
- `cargo build` - Build all crates
- `cargo check` - Type check without building
- `cargo test` - Run all tests
- `cargo test -p <crate_name>` - Run tests for specific crate
- `cargo test --test <test_name>` - Run specific test
- `cargo clippy` - Lint with Clippy
- `cargo fmt` - Format code

## Code Style Guidelines
- **Rust 2021 edition** with standard formatting
- **Imports**: Group std, external, internal crates with blank lines
- **Naming**: snake_case for variables/functions, PascalCase for types
- **Error handling**: Use `Result` and `?` operator, avoid unwrap()
- **Documentation**: Use /// doc comments for public items
- **Dependencies**: Use workspace dependencies when possible

## Project Structure
- Multi-crate workspace under `crates/` directory
- Each crate has specific purpose (app, server, store, cli, offchain)
- Shared dependencies in workspace Cargo.toml

## Testing
- Unit tests in `src/` files with `#[cfg(test)]` mod
- Integration tests in `tests/` directory
- Use `#[test]` attribute for test functions

## Common Patterns
- Async/await with Tokio runtime
- Tracing for logging
- Serde for serialization
- Ergo blockchain integration

## Basis Contract & Cryptography

### Signature Algorithm
- **secp256k1** elliptic curve cryptography used for all signatures
- **33-byte public keys** compressed format (66 hex characters)
- **65-byte Schnorr signatures** (130 hex characters) - following chaincash-rs approach
- Signature verification ensures only authorized issuers can create notes

### Basis Reserve Contract (ErgoScript)
- **On-chain collateral management** for debt issuance
- **Reserve tracking** to ensure proper collateralization
- **Event emission** for off-chain tracking of reserve changes
- **Collateralization ratio enforcement** to prevent over-issuance

### Cryptographic Operations
- **Note signing**: Issuers sign notes with their private keys
- **Signature verification**: Recipients verify issuer signatures
- **Public key management**: Proper handling of compressed secp256k1 keys
- **Message formatting**: Standardized signing message format for notes

### Signature Format
- **Public keys**: 33 bytes compressed secp256k1 (66 hex chars)
- **Signatures**: 65 bytes (130 hex chars) - 33-byte a + 32-byte z (Schnorr format)
- **Signing message**: `recipient_pubkey || amount_be_bytes || timestamp_be_bytes`
- **Verification**: Schnorr signature verification following chaincash-rs approach
- **Algorithm**: `g^z = a * x^e` where:
  - `g` is generator point
  - `z` is response from signature
  - `a` is random point from signature
  - `x` is issuer public key
  - `e` is challenge: `H(a || message || issuer_pubkey)`

### Contract Integration
- **Ergo node communication** for on-chain state monitoring
- **Reserve event parsing** from blockchain transactions
- **Collateralization calculation** based on on-chain reserves
- **State commitment** to ensure consistency between on-chain and off-chain states

### Contract Deployment
- **Contract template**: Standardized Basis reserve contract template
- **Deployment parameters**: Customizable collateral requirements and ratios
- **Address generation**: Deterministic contract address from template and parameters
- **Multi-chain support**: Designed for deployment on Ergo mainnet and testnets

## Schnorr Signature Implementation

### Chaincash-rs Integration
- **Signature format**: 65 bytes total (33-byte a + 32-byte z)
- **Signing algorithm**: Following chaincash-rs Schnorr signature approach
- **Challenge computation**: `e = H(a || message || issuer_pubkey)` using Blake2b512
- **Response computation**: `z = k + e * s (mod n)` using proper modular arithmetic
- **Verification**: Verify `g^z = a * x^e` using secp256k1 point operations

### Key Changes from Previous Implementation
- **Signature size**: Updated from 64 bytes to 65 bytes
- **Algorithm**: Replaced ECDSA-style with proper Schnorr signatures
- **Compatibility**: Matches chaincash-rs and ErgoScript contract requirements
- **Security**: Strong Fiat-Shamir transform with proper challenge computation