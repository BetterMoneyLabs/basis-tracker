# Basis Tracker Development Guide

## Build & Test Commands
- `cargo build` - Build all Rust crates
- `cargo check` - Type check without building
- `cargo test` - Run all Rust tests
- `cargo test -p <crate_name>` - Run tests for specific Rust crate
- `cargo test --test <test_name>` - Run specific Rust integration test
- `cargo clippy` - Lint Rust with Clippy
- `cargo fmt` - Format Rust code
- `cd scala && sbt compile` - Compile Scala reference implementation
- `cd scala && sbt test` - Run Scala contract tests

## Code Style Guidelines
- **Rust 2021 edition** with standard formatting
- **Scala 2.12** for the reference implementation under `scala/`
- **Imports**: Group std, external, internal crates with blank lines
- **Naming**: snake_case for variables/functions, PascalCase for types
- **Error handling**: Use `Result` and `?` operator, avoid unwrap()
- **Documentation**: Use /// doc comments for public items
- **Dependencies**: Use workspace dependencies when possible

## Project Structure
- Multi-crate workspace under `crates/` directory
- Each crate has specific purpose (app, server, store, cli, mcp, offchain)
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
- **Four actions**: redeem note (#0), top up (#1), initiate refund (#2), complete refund (#3)
- **Two-phase refund**: owner can initiate unilateral exit, creditors have ~2 months to redeem before completion
- **Two contract variants**:
  - `contract/basis.es` — ERG-collateralized reserve
  - `contract/basis-token.es` — token-collateralized reserve

### Cryptographic Operations
- **Note signing**: Issuers sign notes with their private keys
- **Signature verification**: Recipients verify issuer signatures
- **Public key management**: Proper handling of compressed secp256k1 keys
- **Message formatting**: Standardized signing message format for notes

### Signature Format
- **Public keys**: 33 bytes compressed secp256k1 (66 hex chars)
- **Signatures**: 65 bytes (130 hex chars) - 33-byte a + 32-byte z (Schnorr format)
- **Signing message**: `key || longToByteArray(totalDebt) || longToByteArray(timestamp)` (48 bytes)
  - Same format for both normal and emergency redemption
  - Where `key = blake2b256(ownerKeyBytes || receiverKeyBytes)` (32 bytes)
  - `ownerKey`: Reserve owner's public key (issuer of the IOU note)
  - `receiverKey`: Recipient's public key (creditor)
  - `totalDebt`: Total cumulative debt amount (8 bytes big-endian)
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

## Scala Reference Implementation

A Scala reference implementation for the Basis protocol lives under `scala/`:

- `scala/src/main/scala/basis/contracts/` — Contract compilation, deployment utilities, address/key helpers, and test-vector generation
- `scala/src/main/scala/basis/offchain/` — Reference Schnorr signing/verification
- `scala/src/test/scala/basis/contracts/` — On-chain contract tests (`BasisSpec`, `BasisTokenSpec`)

It is built with sbt and reads contracts from `../contract/` relative to the `scala/` directory.

### Running

```bash
cd scala
sbt compile
sbt test
sbt "runMain basis.contracts.TestVectorGenerator"
```

### Test Secrets

Tests require `scala/secrets/participants.csv` with valid `name,address,secret_hex` rows for `tracker`, `alice`, and `bob`. A committed test fixture is provided; for real deployments copy `scala/secrets/participants.csv.template` to `scala/secrets/participants.local.csv`.

## Schnorr Signature Implementation

### Chaincash-rs Integration
- **Signature format**: 65 bytes total (33-byte a + 32-byte z)
- **Signing algorithm**: Following chaincash-rs Schnorr signature approach
- **Challenge computation**: `e = H(a || message || issuer_pubkey)` using Blake2b256
- **Response computation**: `z = k + e * s (mod n)` using proper modular arithmetic
- **Verification**: Verify `g^z = a * x^e` using secp256k1 point operations

### Key Changes from Previous Implementation
- **Signature size**: Updated from 64 bytes to 65 bytes
- **Algorithm**: Replaced ECDSA-style with proper Schnorr signatures
- **Compatibility**: Matches chaincash-rs and ErgoScript contract requirements
- **Security**: Strong Fiat-Shamir transform with proper challenge computation
- **Module structure**: Schnorr operations extracted to dedicated `schnorr.rs` module

### Scala Compatibility
- **bitLength constraint**: Both Scala and Rust implementations enforce `z.bitLength <= 255`
- **Retry logic**: Signatures with `z.bitLength > 255` are automatically regenerated with a new nonce (no retry limit)
- **Cross-validation**: All signatures verified against hardcoded Scala test vectors (see specs/SCHNORR_SIGNATURE_SPEC.md)
- **Ergo node compatibility**: Basis server signatures are compatible with ErgoScript contract verification

## Ergo Blockchain Scanner

### Scanner Implementation
- **Chaincash-rs style scanner** - Following chaincash-rs pattern with background scanning tasks
- **Single unified implementation** - Clean, consistent API
- **Real blockchain integration** - Connects to actual Ergo nodes for production use
- **Mock scanner** - For testing and development without network dependency

### Scanner Features
- **Background scanning tasks** running independently
- **Event-driven architecture** similar to chaincash-rs
- **Automatic block waiting** and continuous scanning
- **Simplified API** with ServerState pattern
- **Event processing** for reserve creation, top-up, redemption, and spending
- **Unspent box querying** for current reserve state
- **Contract template filtering** for targeted scanning
- **Real Ergo node integration** - Connects to mainnet and testnet nodes

### Usage Example
```rust
use basis_store::ergo_scanner::{start_scanner, create_default_scanner};

// Create a scanner with default configuration
let state = create_default_scanner();

// Start the scanner (runs background tasks)
start_scanner(state).await.unwrap();

// Scanner runs in background, processing events automatically
```

### Real Scanner Usage
```rust
use basis_store::ergo_scanner::{NodeConfig, ServerState};

// Create real scanner for mainnet
let config = NodeConfig::default();
let scanner = ServerState::new(config, "http://159.89.116.15:11088".to_string());

// Test connectivity
let height = scanner.get_current_height().await?;
println!("Current blockchain height: {}", height);
```

### Event Types
- **ReserveCreated**: New reserve box created on-chain
- **ReserveToppedUp**: Existing reserve receives additional collateral
- **ReserveRedeemed**: Debt redemption from reserve
- **ReserveSpent**: Reserve box spent/closed

### Available Ergo Nodes
- **Mainnet**: `http://159.89.116.15:11088` (public)
- **Testnet**: `http://213.239.193.208:9052` (public)
- **Local**: `http://localhost:9053` (development)

### Configuration
- Node configuration stored in `config/basis.toml` under `[ergo.node]`
- Supports multiple networks (mainnet, testnet, local)
- Configurable timeouts and contract templates
- API key support for authenticated nodes

### Testing with Real Scanner
```bash
# Run real scanner integration tests (requires network)
cargo test -p basis_store --features ergo_scanner real_scanner_integration_tests -- --ignored

# Test script for real scanner
./test_real_scanner.sh
```

## Documentation Requirements

### Plans and Summaries
- **All development plans, design documents, and technical summaries MUST be written into the `specs/` folder only**
- Do not create planning documents in other directories
- Use appropriate file naming conventions within the specs/ folder
- Maintain consistency with existing spec documentation structure

### basis_trees Crate Documentation
- **All basis_trees crate related documentation MUST be placed in the `specs/trees/` folder**
- This includes AVL tree implementations, storage plans, recovery mechanisms, and persistence documentation
- Keep the main `specs/` directory for general project specifications only

## API Documentation References

The system makes use of the Ergo node API for blockchain interaction and scanning. See the [Ergo Node API specification](specs/ergo/openapi.yaml) for detailed information about the scan functionality used by the tracker to monitor blockchain boxes containing tracker NFTs and register values.

The tracker uses the `/scan` endpoints to efficiently monitor relevant boxes on the Ergo blockchain without having to scan the entire blockchain. Key endpoints include:
- `/scan/register`: Register a scan to monitor specific types of boxes
- `/scan/listAll`: List all registered scans
- `/scan/unspentBoxes/{scanId}`: Retrieve unspent boxes matching a scan
- `/scan/spentBoxes/{scanId}`: Retrieve spent boxes matching a scan

These scanning capabilities enable the tracker to efficiently monitor both reserve boxes and tracker commitment boxes containing R4 (tracker public key) and R5 (AVL+ tree root digest) register values.

## Changing the HTTP API

When adding, removing, or modifying a tracker server endpoint, update the following artifacts and run the consistency checks:

1. **Rust implementation** in `crates/basis_server/src/`.
   - Add the handler and route in `main.rs`.
   - Add or update request/response structs in `models.rs`, `reserve_api.rs`, or `redemption_build.rs`.
2. **OpenAPI specification** in `openapi.yaml`.
   - Add/update the path, operation, parameters, request body, and response schemas.
   - Ensure every `$ref` points to a schema defined in `components/schemas`.
3. **Human docs** in `docs/OPENAPI.md`.
   - Update the endpoint list and any affected examples or tables.
4. **CLI client** in `crates/basis_cli/src/api.rs`.
   - Add/update `TrackerClient` methods and request/response types if the endpoint is consumed by the CLI or MCP server.
5. **MCP server** in `crates/basis_mcp/src/server.rs`.
   - Add or update MCP tool definitions and parameter structs if the operation should be exposed to MCP clients.
6. **Run consistency checks** before committing:
   ```bash
   cargo test -p basis_server --test openapi_consistency -- --nocapture
   python3 -c "import yaml; yaml.safe_load(open('openapi.yaml'))"
   ```

CI will fail if `openapi.yaml` does not match the routes registered in `crates/basis_server/src/main.rs` or if the YAML is invalid.

## Server Authentication & TLS

The tracker server supports optional TLS and three authentication modes
(`none`, `api_key`, `signature`), configured under `[server]` and
`[server.auth]` in `config/basis.toml`.

When changing authentication behavior, update all of the following:

1. **Server implementation** in `crates/basis_server/src/`.
   - `config.rs` for `AuthConfig`, `AuthMode`, `ClientRole`, and defaults.
   - `auth_middleware.rs` for credential verification and replay protection.
   - `authorization.rs` for role-to-route mapping.
   - `main.rs` for wiring middleware, CORS, and HTTPS serving.
2. **CLI client** in `crates/basis_cli/src/api.rs`.
   - `TrackerAuth` enum and header generation in `TrackerClient`.
   - `config.rs` for persisting auth settings in `cli.toml`.
3. **MCP server and TUI wallet** in `crates/basis_mcp/src/server.rs` and
   `crates/basis_app/src/app.rs`.
   - `tracker_auth_from_env_or_config()` for reading auth from environment
     variables with a `~/.basis/cli.toml` fallback.
   - `TrackerAuth::from_config()` for the TUI wallet.
4. **OpenAPI specification** in `openapi.yaml`.
   - `components/securitySchemes` and per-path `security` requirements.
5. **Human docs** in `docs/OPENAPI.md` and `config/basis.toml.example`.

### Signature mode canonical string

Clients using signature authentication sign:

```text
<METHOD>\n<PATH>\n<QUERY>\n<TIMESTAMP>\n<NONCE>\n<BODY_HASH>
```

where `BODY_HASH` is the lowercase hex SHA-256 of the raw request body.

## Future Direction

The project intends to reduce manual duplication further by extracting shared API request/response types and deriving OpenAPI schemas from them. Until that work is complete, the consistency test above is the enforced guardrail.
