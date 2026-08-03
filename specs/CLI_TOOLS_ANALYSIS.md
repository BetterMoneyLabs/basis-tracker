# CLI Tools Analysis Report

## Executive Summary

This repository contains **4 compiled CLI binaries** and **7 shell scripts** that provide command-line interfaces for the Basis Tracker system. The primary CLI tool is `basis_cli` (Rust-based), the secondary is `basis_server` (Rust-based daemon), the third is `basis_app` (TUI wallet, also Rust-based), and the fourth is `basis_mcp` (MCP server for AI agents). Supporting shell scripts handle server lifecycle management, database cleanup, deployment, and TUI wallet launch. Integration testing is covered by Rust test suite (`cargo test`).

---

## Compiled CLI Binaries

### 1. `basis_cli` - Primary CLI Client

**Location**: `crates/basis_cli/`
**Library name**: `basis_cli_lib`
**Binary name**: `basis_cli`
**Language**: Rust (Edition 2021)
**Dependencies**: clap, tokio, secp256k1, serde, ureq, anyhow, ergo-lib

#### Architecture
The CLI is structured as a library crate with an optional binary feature. It uses a modular command structure with clap derive macros for argument parsing:

```
Cargo.toml (lib + bin configuration)
src/
├── lib.rs              (module declarations: account, api, commands, config, crypto, demo_keys, interactive, output)
├── main.rs             (entry point, command routing, --json flag, exit-code contract)
├── account.rs          (Account model & manager with persistent storage)
├── api.rs              (HTTP client for server API with redemption support)
├── config.rs           (Configuration management for ~/.basis/cli.toml)
├── crypto.rs           (Schnorr signature implementation using secp256k1)
├── demo_keys.rs        (Demo key fixtures loaded from secrets/participants.csv)
├── interactive.rs      (Interactive REPL mode)
├── output.rs           (JSON-mode flag + progress! macro routing diagnostics to stderr)
└── commands/
    ├── mod.rs          (Module declarations)
    ├── account.rs      (Account management: create, list, switch, info, export, import)
    ├── keypair.rs      (Keypair generation)
    ├── note.rs         (Note operations: create, list, get, redeem)
    ├── reserve.rs      (Reserve operations: create, status, collateralization)
    ├── status.rs       (Server status and recent events)
    ├── test_redemption.rs (Polling-based redemption test utility)
    └── transaction.rs  (Transaction generation: generate-redemption)
```

#### Command Reference

| Command | Subcommands | Description |
|---------|------------|-------------|
| `account` | `create <name>`, `list`, `switch <name>`, `info`, `export <name>`, `import <name> <key>` | Account management with persistent storage |
| `generate-keypair` | - | Generate secp256k1 keypair (33-byte pubkey, 32-byte privkey) |
| `note` | `create --recipient <pubkey> --amount <amount> [--demo]`, `list --issuer\|--recipient`, `get --issuer <pubkey> --recipient <pubkey>`, `redeem --issuer <pubkey> --amount <amount>` | IOU note lifecycle management |
| `reserve` | `create --nft-id <id> [--owner <pubkey>] --amount <amount>`, `status [--issuer <pubkey>]`, `collateralization [--issuer <pubkey>]` | Reserve creation and monitoring |
| `transaction` | `generate-redemption --issuer-pubkey <hex> --recipient-pubkey <hex> --amount <nanoERG> [--output-file <path>] [--emergency]` | Generate unsigned redemption transactions with Ergo node integration |
| `test` | `test-redemption [--output-file <path>] [--amount <nanoERG>] [--poll-interval <secs>]` | Polling-based redemption test utility |
| `interactive` | - | REPL mode with account-aware prompt |
| `status` | - | Check server health and display recent events |

#### Key Features
- **Account Management**: Persistent accounts stored in `~/.basis/cli.toml` with private keys
- **Schnorr Signatures**: 65-byte signatures (33-byte a + 32-byte z) with Blake2b256 challenge
- **Ergo Blockchain Integration**: P2PK address generation, box serialization, transaction building with context extension variables
- **Interactive Mode**: REPL with command history and contextual help
- **Demo Mode**: Pre-configured Alice/Bob/Tracker keys for testing (loaded from `secrets/participants.csv`)
- **Redemption Transaction Generation**: Full unsigned transaction generation with AVL proofs, signatures, and Ergo node box retrieval
- **Polling Test Utility**: Automated polling for redeemable notes with sufficient collateral

#### Agent-Friendly JSON Mode
- **Global `--json` flag**: every command prints a single JSON document to stdout (typed result structs defined per command module); human-readable output remains the default
- **Typed command cores**: each `commands/*` module exposes `pub` functions returning serde-serializable result structs, reused by `basis_mcp`
- **Exit-code contract**: `0` success, `1` error, `2` server unreachable; in JSON mode errors are printed as `{"error": ...}` to stderr
- See `docs/AGENT_INTERFACE.md` for per-command JSON examples

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

While `basis_server` is a compiled binary, it functions primarily as a background daemon with an HTTP API. It is not an interactive CLI tool but is included here for completeness as it provides the server that `basis_cli` and `basis_app` communicate with.

---

### 3. `basis_app` - TUI Wallet Application

**Location**: `crates/basis_app/`
**Library name**: `basis_app`
**Binary name**: `basis-ui`
**Language**: Rust (Edition 2021)
**Dependencies**: tokio, serde, secp256k1, blake2, ergo-lib, basis_cli_lib, basis_store

#### Architecture
The TUI wallet is a terminal-based interactive application built on top of `basis_cli_lib`:

```
Cargo.toml (lib + bin configuration)
src/
├── main.rs             (Entry point: creates App and runs UI)
├── app.rs              (Application state: screens, accounts, notes, reserves, notifications)
└── ui.rs               (Terminal UI rendering: menus, forms, banners, ANSI colors)
```

#### Screens
- **Intro** (first run only): shown when no account exists — a `default` account is auto-created and its public key is displayed to the user
- **MainMenu**: Primary navigation menu (Notes, Reserves, Redemption, My Acceptance Policy, Address Book, Settings)
- **Accounts**: Account management (create, switch, import, export) — reached via Settings
- **Notes**: Note listing and management
- **Reserves**: Reserve status and collateralization display
- **Transactions**: Redemption transaction generation
- **AddressBook**: Contact management (auto-synced with accounts)
- **Settings**: Server URL and accounts management
- **CreateNote**: Interactive note creation form
- **RedeemNote**: Interactive redemption workflow
- **CreateReserve**: Reserve creation form
- **GenerateTransaction**: Transaction generation interface
- **AcceptancePolicy**: Acceptance policy editor (collateral level, whitelist, blacklist)

#### Key Features
- **Terminal UI**: Full-screen interactive interface with ANSI colors and banners
- **Free Banking Branding**: "Free Banking For Everyone" tagline
- **First-Run Setup**: Auto-creates a default account on first launch and shows its public key in a welcome screen
- **Real-time Data**: Auto-refreshes reserve status, issued notes, and received notes
- **Address Book**: Contacts auto-synced from accounts, plus manual entries
- **Server Connectivity**: Health check and connection status display
- **Notification System**: Success/error messages with visual indicators

---

### 4. `basis_mcp` - MCP Server for AI Agents

**Location**: `crates/basis_mcp/`
**Binary name**: `basis-mcp`
**Language**: Rust (Edition 2021)
**Dependencies**: rmcp, schemars, clap, tokio, serde, basis_cli_lib, basis_core

An MCP (Model Context Protocol) server over stdio that exposes the Basis wallet to AI agents as typed tools. It wraps the typed command cores of `basis_cli_lib` and shares `~/.basis/cli.toml` (accounts) and `~/.basis/ui.toml` (acceptance policy) with the CLI and TUI.

#### Tools
- **Read-only** (`readOnlyHint`): `server_status`, `account_list`, `account_current`, `note_list`, `note_get`, `reserve_status`, `policy_get`
- **Write**: `account_create`, `account_switch`, `account_import`, `note_create`, `note_redeem` (local signing), `reserve_create`, `policy_set` (`destructiveHint` where applicable)

Private-key export is deliberately not exposed through any tool; signing happens in-process. See `docs/AGENT_INTERFACE.md` for the full tool reference and client configuration snippets.

---

### 5. `basis_store` - Test Runner

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

### 6. `tui.sh` - TUI Wallet Launcher

**Purpose**: Build (if needed) and run the interactive TUI wallet in the foreground
**Features**:
- Auto-detects project root regardless of where it is invoked from
- Builds the `basis-ui` binary if missing (`cargo build -p basis_app --release`)
- Runs `target/release/basis-ui` interactively
- Colored status output

**Usage**: `./tui.sh`

---

## Tool Interactions

```
┌─────────────────┐     HTTP API      ┌─────────────────┐
│   basis_cli     │ ◄──────────────► │  basis_server   │
│  (User CLI)     │                   │  (HTTP Daemon)  │
└─────────────────┘                   └─────────────────┘
         │                                     │
         │                                     │ Ergo Node API
         ▼                                     ▼
┌─────────────────┐                   ┌─────────────────┐
│   basis_app     │                   │  Ergo Node      │
│  (TUI Wallet)   │                   │  (Blockchain)   │
└─────────────────┘                   └─────────────────┘
         │
         │ Shell scripts
         ▼
┌─────────────────┐
│ run_server.sh   │
│ stop_server.sh  │
│ server_status.sh│
│ clean_database  │
│ redeploy.sh     │
│ tui.sh          │
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
| `secrets/participants.csv` | Demo participant keys (Alice, Bob, Tracker) - not committed |

---

## Security Considerations

1. **Private Key Storage**: `basis_cli` stores private keys in plaintext in `~/.basis/cli.toml`
2. **No Encryption**: No key derivation or encryption for stored accounts
3. **Demo Keys**: Hardcoded demo keys exist in `demo_keys.rs` for testing (loaded from `secrets/participants.csv`)

---

## Recommendations

1. **Add key encryption**: Encrypt stored private keys with a user passphrase
2. **Remove hardcoded secrets**: Move API keys to environment variables or config files
3. **Add CLI completion**: Generate shell completions for `basis_cli`
4. **Add logging**: CLI operations should have structured logging options
5. **Add `--dry-run` mode**: For transaction generation commands
6. **Consolidate scripts**: Consider converting shell scripts to subcommands of `basis_cli`
