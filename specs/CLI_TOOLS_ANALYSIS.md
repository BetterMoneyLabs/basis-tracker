# CLI Tools Analysis Report

## Executive Summary

This repository contains **2 compiled CLI binaries** and **6 shell scripts** that provide command-line interfaces for the Basis Tracker system. The primary CLI tool is `basis_cli` (Rust-based), and the secondary is `basis_server` (also Rust-based, but primarily a daemon). Supporting shell scripts handle server lifecycle management, database cleanup, and deployment. Integration testing is covered by Rust test suite (`cargo test`).

---

## Compiled CLI Binaries

### 1. `basis_cli` - Primary CLI Client

**Location**: `crates/basis_cli/`
**Binary name**: `basis_cli`
**Language**: Rust (Edition 2021)
**Dependencies**: clap, tokio, secp256k1, serde, ureq, anyhow, ergo-lib

#### Architecture
The CLI uses a modular command structure with clap derive macros for argument parsing:

```
main.rs (entry point, command routing)
├── commands/
│   ├── account.rs      (Account management)
│   ├── keypair.rs      (Keypair generation)
│   ├── note.rs         (Note operations)
│   ├── reserve.rs      (Reserve operations)
│   ├── status.rs       (Server status)
│   ├── transaction.rs  (Transaction generation)
│   └── test_redemption.rs (Test utilities)
├── account.rs          (Account model & manager)
├── api.rs              (HTTP client for server API)
├── config.rs           (Configuration management)
├── crypto.rs           (Schnorr signature implementation)
├── demo_keys.rs        (Demo key fixtures)
└── interactive.rs      (Interactive REPL mode)
```

#### Command Reference

| Command | Subcommands | Description |
|---------|------------|-------------|
| `account` | `create <name>`, `list`, `switch <name>`, `info`, `export <name>`, `import <name> <key>` | Account management with persistent storage |
| `generate-keypair` | - | Generate secp256k1 keypair (33-byte pubkey, 32-byte privkey) |
| `note` | `create`, `list`, `get`, `redeem` | IOU note lifecycle management |
| `reserve` | `create`, `status`, `collateralization` | Reserve creation and monitoring |
| `transaction` | `generate-redemption` | Generate unsigned redemption transactions |
| `test` | `test-redemption` | Polling-based redemption test utility |
| `interactive` | - | REPL mode with account-aware prompt |
| `status` | - | Check server health and display recent events |

#### Key Features
- **Account Management**: Persistent accounts stored in `~/.basis/cli.toml` with private keys
- **Schnorr Signatures**: 65-byte signatures (33-byte a + 32-byte z) with Blake2b256 challenge
- **Ergo Blockchain Integration**: P2PK address generation, box serialization, transaction building
- **Interactive Mode**: REPL with command history and contextual help
- **Demo Mode**: Pre-configured Alice/Bob/Tracker keys for testing

#### Cryptographic Details
- **Curve**: secp256k1
- **Public Keys**: 33 bytes compressed format
- **Signatures**: 65 bytes Schnorr (a || z format)
- **Signing Message**: `blake2b256(ownerKey || receiverKey) || totalDebt || timestamp` (48 bytes)
- **Verification**: `g^z = a * x^e` where `e = H(a || message || pubkey)`

---

### 2. `basis_server` - Server Daemon

**Location**: `crates/basis_server/`
**Binary name**: `basis_server`
**Language**: Rust
**Primary Role**: HTTP API server and blockchain scanner (not primarily CLI-interactive)

While `basis_server` is a compiled binary, it functions primarily as a background daemon with an HTTP API. It is not an interactive CLI tool but is included here for completeness as it provides the server that `basis_cli` communicates with.

---

### 3. `basis_store` - Test Runner

**Location**: `crates/basis_store/src/main.rs`
**Binary name**: Not explicitly defined in Cargo.toml `[[bin]]`, but has a `main.rs`
**Purpose**: Manual test runner for basis_store internal tests

This is a minimal utility that runs `basis_store::tests::run_all_tests()` and exits. Not a primary CLI tool.

---

## Shell Scripts

### 1. `run_server.sh` - Server Startup

**Purpose**: Start the `basis_server` daemon in the background
**Features**:
- Checks for binary existence, builds if missing (`cargo build -p basis_server --release`)
- PID file management (`server.pid`)
- Log redirection (`server.log`)
- Colored status output
- Prevents duplicate starts

**Usage**: `./run_server.sh`

---

### 2. `stop_server.sh` - Server Shutdown

**Purpose**: Gracefully stop the running server
**Features**:
- Reads PID from `server.pid`
- Sends SIGTERM, waits up to 10 seconds
- Falls back to SIGKILL if necessary
- Cleans up stale PID files
- Colored output

**Usage**: `./stop_server.sh`

---

### 3. `server_status.sh` - Server Monitoring

**Purpose**: Check server health and display process info
**Features**:
- Verifies process is running via PID file
- Shows CPU/memory usage (`ps` output)
- Displays log file size and line count
- Shows last 5 log entries
- Colored output

**Usage**: `./server_status.sh`

---

### 4. `clean_database.sh` - Database Cleanup

**Purpose**: Safely remove all database files and server runtime files
**Features**:
- Stops running server before cleanup
- Removes multiple database directories (`data/`, server data dirs)
- Removes log and PID files
- Optional backup creation (`-b` flag)
- Auto-confirm mode (`-y` flag)
- Interactive confirmation prompt
- Recreates directory structure after cleanup

**Usage**: `./clean_database.sh [-y|--yes] [-b|--backup] [-h|--help]`

---

### 5. `redeploy.sh` - Deployment Automation

**Purpose**: Full redeployment workflow
**Features**:
- `git pull origin master`
- `cargo clean`
- `./run_server.sh`
- Colored output with error handling

**Usage**: `./redeploy.sh`

---



## Tool Interactions

```
┌─────────────────┐     HTTP API      ┌─────────────────┐
│   basis_cli     │ ◄──────────────► │  basis_server   │
│  (User CLI)     │                   │  (HTTP Daemon)  │
└─────────────────┘                   └─────────────────┘
         │                                     │
         │ Shell scripts                       │ Ergo Node API
         ▼                                     ▼
┌─────────────────┐                   ┌─────────────────┐
│ run_server.sh   │                   │  Ergo Node      │
│ stop_server.sh  │                   │  (Blockchain)   │
│ server_status.sh│                   └─────────────────┘
│ clean_database  │
│ redeploy.sh     │
└─────────────────┘
└─────────────────┘
```

---

## Configuration Files

| File | Purpose |
|------|---------|
| `~/.basis/cli.toml` | CLI account storage (TOML format with private keys) |
| `config/basis.toml` | Server configuration (Ergo node, tracker settings) |
| `server.pid` | Runtime PID file for server management |
| `server.log` | Server log output |

---

## Security Considerations

1. **Private Key Storage**: `basis_cli` stores private keys in plaintext in `~/.basis/cli.toml`
2. **No Encryption**: No key derivation or encryption for stored accounts
3. **Demo Keys**: Hardcoded demo keys exist in `demo_keys.rs` for testing

---

## Recommendations

1. **Add key encryption**: Encrypt stored private keys with a user passphrase
2. **Remove hardcoded secrets**: Move API keys to environment variables or config files
3. **Add CLI completion**: Generate shell completions for `basis_cli`
4. **Add logging**: CLI operations should have structured logging options
5. **Add `--dry-run` mode**: For transaction generation commands
6. **Consolidate scripts**: Consider converting shell scripts to subcommands of `basis_cli`
