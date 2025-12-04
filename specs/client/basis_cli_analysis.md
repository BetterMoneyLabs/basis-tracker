# Basis CLI Crate Analysis

## Overview
The `basis_cli` crate is a command-line interface client for the Basis Tracker system. It provides a user-friendly way to interact with the Basis Tracker server, manage accounts, create and manage IOU notes, and handle reserve operations.

## Project Structure
```
crates/basis_cli/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── account.rs
│   ├── api.rs
│   ├── config.rs
│   ├── crypto.rs
│   ├── interactive.rs
│   └── commands/
│       ├── mod.rs
│       ├── account.rs
│       ├── note.rs
│       ├── reserve.rs
│       └── status.rs
└── tests/
```

## Dependencies
### Core Dependencies
- `clap` - Command line argument parsing with derive features
- `ureq` - HTTP client library with JSON support
- `tokio` - Async runtime with full features
- `secp256k1` - Cryptographic library for key management with rand-std and global-context
- `serde`/`serde_json` - Serialization/deserialization support
- `hex` - Hexadecimal encoding/decoding
- `anyhow` - Error handling
- `config`, `dirs`, `toml` - Configuration management
- `rand`, `blake2` - Random number generation and hashing
- `basis_store` - Local dependency for storage functionality

### Features
- Default feature "bin" enables building the binary
- Binary only built when "bin" feature is enabled

## Architecture
### Main Components

#### 1. CLI Interface (`main.rs`)
- Uses `clap` for command parsing with derive macros
- Defines subcommands: Account, Note, Reserve, Interactive, Status
- Manages configuration loading and account management
- Initializes the API client for server communication

#### 2. Command Handlers (`commands/`)
Different modules handle various command categories:
- `account.rs` - Account management (create, list, switch, import/export)
- `note.rs` - IOU note operations
- `reserve.rs` - Reserve and collateral operations
- `status.rs` - System status checks

#### 3. Account Management (`account.rs`)
- Manages cryptographic key pairs (secp256k1)
- Handles account creation, switching, and persistence
- Supports import/export of private keys
- Maintains both in-memory and persistent accounts

#### 4. API Client (`api.rs`)
- HTTP client using `ureq` to communicate with the server
- Handles various API endpoints:
  - Note operations (create, retrieve by issuer/recipient)
  - Reserve status (debt, collateral, collateralization ratios)
  - Redemption processes
  - Event querying
  - Proof retrieval
- Uses strongly-typed request/response structures

#### 5. Configuration Management (`config.rs`)
- Manages CLI configuration in TOML format
- Persists accounts and settings to `~/.basis/cli.toml`
- Handles account configuration with key data

#### 6. Cryptographic Functions (`crypto.rs`)
- Key generation and management using secp256k1
- Signature and verification functions
- Hashing utilities using Blake2

#### 7. Interactive Mode (`interactive.rs`)
- Provides an interactive REPL-style interface
- Allows users to perform multiple operations without re-launching

## Functionality

### Account Management
- Create new accounts with secp256k1 key pairs
- List all accounts (both in-memory and persisted)
- Switch between accounts
- Show current account information
- Import/export private keys (with security warnings)

### Note Operations
- Creation and management of IOU notes
- Querying notes by issuer or recipient
- Support for signed notes with cryptographic verification

### Reserve Operations
- Checking reserve status and collateralization ratios
- Managing debt and collateral positions
- Integration with the tracking system

### Redemption Functions
- Initiating redemption requests
- Completing redemption processes
- Retrieving proofs for redemption verification

### Status and Monitoring
- Server health checks
- Event querying and monitoring
- System status reporting

## Communication Protocol
- RESTful API communication with the server at http://127.0.0.1:3048 by default
- JSON-based request/response format
- Standard HTTP methods (GET, POST) with structured data
- Error responses with success/error indicators

## Security Features
- Secure key storage with encrypted private keys
- Cryptographic signatures for transaction authenticity
- Secure random number generation
- Warning messages for sensitive operations like key exports

## Configuration
- Default server URL: http://127.0.0.1:3048
- User configuration stored in `~/.basis/cli.toml`
- Support for custom configuration paths
- Persistent account management between sessions

## Error Handling
- Comprehensive error handling using `anyhow` crate
- Structured API responses with success/error indicators
- Input validation and meaningful error messages
- Graceful handling of network and server errors

## Design Patterns
- Modular architecture with clear separation of concerns
- Command pattern for CLI command handling
- Configuration management pattern for persistent settings
- Service layer pattern for API communication
- State management for accounts and session data