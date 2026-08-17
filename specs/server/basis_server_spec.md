# Basis Server Crate Specification

## Overview

The `basis_server` crate is a Rust web server built with the Axum framework that provides an HTTP API for the Basis Tracker system. It serves as the core component for managing IOU notes, tracking reserve events on the Ergo blockchain, providing proof mechanisms for the Basis protocol, and facilitating redemption with tracker signatures.

## Architecture

### Main Components

1. **API Module**: Contains all HTTP route handlers for the web server
2. **Reserve API Module**: Handles reserve-specific endpoints
3. **Models Module**: Defines data structures for API requests/responses
4. **Store Module**: Implements event storage functionality
5. **Auth Middleware**: Authentication middleware supporting anonymous, API-key, and secp256k1-signature modes
6. **Authorization Middleware**: Role-based access control (RBAC) enforcing `Read`, `Write`, and `Admin` privileges per route
7. **Config Module**: Handles application configuration, including TLS and authentication settings
8. **Tracker Thread**: Background task that processes commands via message passing
9. **AVL Tree Manager**: Manages the tracker's AVL tree state and proof generation

### Communication Pattern

The server uses an actor-like pattern with a dedicated tracker thread that processes commands via a channel:

- Web handlers send commands through an MPSC channel
- A blocking thread processes tracker commands
- Results are returned via oneshot channels

## Dependencies

- `axum`: Web framework for routing and HTTP handling
- `tokio`: Async runtime for concurrency
- `tracing`: Logging and instrumentation
- `serde/serde_json`: Serialization/deserialization
- `tower-http`: HTTP middleware (CORS, tracing)
- `basis_store`: Core business logic and data structures
- `ergo-lib`: Ergo blockchain interaction

## API Endpoints

### Core Endpoints

- `GET /` - Root endpoint returning "Hello, Basis Tracker API!"
- `POST /notes` - Create a new IOU note
- `GET /notes` - Get all IOU notes in the system
- `GET /notes/issuer/{pubkey}` - Get all notes issued by a public key
- `GET /notes/recipient/{pubkey}` - Get all notes received by a public key
- `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}` - Get specific note between two parties
- `POST /redeem` - Initiate redemption process
- `POST /redeem/complete` - Complete redemption process
- `POST /tracker/signature` - Request tracker signature for redemption (real Schnorr signature generation)
- `POST /redemption/prepare` - Prepare redemption with all necessary data (real AVL proofs + tracker signature)
- `GET /proof/redemption` - Get redemption-specific proof with tracker state digest

### Reserve Endpoints

- `GET /reserves` - Get all reserve information
- `GET /reserves/issuer/{pubkey}` - Get reserves for a specific issuer
- `GET /key-status/{pubkey}` - Get status information for a public key
- `POST /reserves/create` - Create a reserve creation payload for Ergo node's `/wallet/payment/send` API
- `POST /reserves/submit` - Submit a reserve creation payload to the tracker's configured Ergo node for broadcast

### Event Tracking

- `GET /events` - Get recent tracker events
- `GET /events/paginated?page=0&page_size=20` - Get paginated events

## Data Models

### Tracker Event Types

- `NoteUpdated`: When an IOU note is created/modified
- `ReserveCreated`: When a new reserve box is created
- `ReserveToppedUp`: When collateral is added to a reserve
- `ReserveRedeemed`: When collateral is redeemed from a reserve
- `ReserveSpent`: When a reserve box is spent
- `Commitment`: Commitment to tracker state
- `CollateralAlert`: When collateralization ratio falls below threshold
- `DebtTransfer`: When debt is transferred between creditors (novation)

### Tracker Box Registers

The tracker box uses Ergo registers R4 and R5 to store commitment information:

- **R4**: Contains the tracker public key (33-byte compressed secp256k1 point, serialized as `GroupElement`)
- **R5**: Contains the AVL tree root digest serialized as `SAvlTree` (37 bytes total: `0x64` type byte + 33-byte digest + 1-byte flags + VLQ key length + VLQ value length)

The tracker box must also preserve the tracker NFT token (identified by `tracker_nft_id`) in its assets.

The reserve box uses Ergo registers R4, R5, and R6 to store commitment and identification information:

- **R4**: Contains the issuer's public key (GroupElement / 33-byte compressed secp256k1 point) that identifies the reserve owner
- **R5**: Contains the AVL tree root digest (33-byte commitment)
  - Stores: `hash(ownerKey || receiverKey) -> cumulativeRedeemedAmount`
  - Updated when notes are redeemed
- **R6**: Contains the NFT ID of the tracker server (bytes) - identifies which tracker server this reserve is linked to

### IOU Note Structure

The server handles IOU (I Owe You) notes that represent debt obligations:

For most endpoints:
- `recipient_pubkey`: Public key of the recipient
- `amount_collected`: Total amount collected (cumulative debt)
- `amount_redeemed`: Amount already redeemed
- `timestamp`: Creation timestamp
- `signature`: Cryptographic signature (Schnorr signature on `hash(issuer||recipient) || totalDebt`)

For the `GET /notes` endpoint (all notes), additional fields are included:
- `issuer_pubkey`: Public key of the issuer
- `age_seconds`: Age of the note in seconds (calculated from timestamp)

### Tracker Signature Request Structure

The `/tracker/signature` endpoint accepts requests with the following structure:
- `issuer_pubkey`: Public key of the note issuer (hex-encoded, 33 bytes)
- `recipient_pubkey`: Public key of the note recipient (hex-encoded, 33 bytes)
- `total_debt`: Total cumulative debt amount in nanoERG
- `emergency`: Boolean indicating if this is an emergency redemption (tracker signature optional after 3 days)

### Tracker Signature Response Structure

The `/tracker/signature` endpoint returns responses with the following structure:
- `success`: Boolean indicating if the signature generation was successful
- `tracker_signature`: 65-byte Schnorr signature (hex-encoded, 130 characters) proving tracker authorization
- `tracker_pubkey`: Tracker's public key (hex-encoded, 66 characters)
- `message_signed`: The hex-encoded message that was signed
  - Normal and emergency: `hash(issuerKey||recipientKey) || longToByteArray(totalDebt) || longToByteArray(timestamp)` (48 bytes)

### Redemption Preparation Request Structure

The `/redemption/prepare` endpoint accepts requests with the following structure:
- `issuer_pubkey`: Public key of the note issuer (hex-encoded, 33 bytes)
- `recipient_pubkey`: Public key of the note recipient (hex-encoded, 33 bytes)
- `total_debt`: Total cumulative debt amount in nanoERG

### Redemption Preparation Response Structure

The `/redemption/prepare` endpoint returns responses with the following structure:
- `redemption_id`: Unique identifier for the redemption process
- `tracker_lookup_proof`: AVL tree lookup proof for tracker's tree (context var #8, hex-encoded bytes)
- `reserve_lookup_proof`: AVL tree lookup proof for reserve's tree (context var #7, optional, hex-encoded bytes)
- `reserve_insert_proof`: AVL tree insert/update proof for reserve's tree (context var #5, hex-encoded bytes)
- `tracker_signature`: 65-byte Schnorr signature from tracker (hex-encoded, 130 characters)
- `tracker_pubkey`: Tracker's public key (hex-encoded, 66 characters)
- `tracker_state_digest`: 33-byte AVL tree root digest (hex-encoded, 66 characters) representing current tracker state
- `block_height`: Current blockchain height at time of proof generation
- `is_first_redemption`: Boolean indicating if this is the first redemption (reserve_lookup_proof can be omitted)

### Redemption Proof Response Structure

The `/proof/redemption` endpoint returns responses with the following structure:
- `issuer_pubkey`: Public key of the note issuer (hex-encoded, 66 characters)
- `recipient_pubkey`: Public key of the note recipient (hex-encoded, 66 characters)
- `tracker_lookup_proof`: AVL tree lookup proof for tracker's tree (context var #8, hex-encoded bytes)
- `reserve_lookup_proof`: AVL tree lookup proof for reserve's tree (context var #7, optional, hex-encoded bytes)
- `reserve_insert_proof`: AVL tree insert/update proof for reserve's tree (context var #5, hex-encoded bytes)
- `tracker_state_digest`: 33-byte AVL tree root digest (hex-encoded, 66 characters) representing current tracker state
- `reserve_state_digest`: 33-byte AVL tree root digest (hex-encoded, 66 characters) representing current reserve state
- `block_height`: Current blockchain height at time of proof generation
- `timestamp`: Unix timestamp of the proof generation
- `total_debt`: Total cumulative debt from tracker's tree
- `already_redeemed`: Already redeemed amount from reserve's tree (0 if first redemption)

### Real Cryptographic Implementation

The server now implements real cryptographic functionality using the Ergo node's Schnorr signing API instead of mock implementations:

#### Schnorr Signature Generation
- **Local Signing (Primary)**: If `tracker_secret_key` is configured in server config, signatures are generated locally using the tracker's secret key
- **Remote Fallback**: If no secret key is configured, falls back to Ergo node's `/utils/schnorrSign` API
- **Format**: 65-byte signatures (33 bytes for 'a' component + 32 bytes for 'z' component)
- **Structure**: Properly formatted with compressed public key prefix (0x02 or 0x03) followed by the signature components
- **Security**: Supports both local signing (secret key in config) and remote signing (private keys secured within Ergo node)
- **Authentication**: Remote requests to the signing API are authenticated using the tracker API key
- **Implementation**: Tracker signature endpoints (`/tracker/signature` and `/redemption/prepare`) try local signing first, then fall back to Ergo node API
- **Message Format**: 
  - Normal and emergency: `blake2b256(issuerKey||recipientKey) || longToByteArray(totalDebt) || longToByteArray(timestamp)` (48 bytes)
  - Emergency redemption (after 3 days): same 48-byte message format, tracker signature becomes optional

#### AVL Tree Proof Generation
- **Real Proofs**: All proof endpoints now generate actual AVL tree lookup and insert/update proofs from the tracker's and reserve's AVL tree state
- **Format**: Properly formatted proof data that demonstrates existence of key-value pairs in the AVL tree
- **State Commitment**: Tracker state digest properly formatted as 33-byte AVL tree root (1 byte height + 32 bytes hash)
- **Integration**: Proofs are generated by the actual tracker state manager using the AVL tree implementation
- **Context Variables**: Proofs are generated for specific context extension variables:
  - #5: Reserve tree insert/update proof
  - #7: Reserve tree lookup proof (optional)
  - #8: Tracker tree lookup proof (required)

#### Tracker State Management
- **Shared State**: Tracker state is maintained in shared state accessible via `state.shared_tracker_state`
- **Real Digests**: Tracker state digests come from actual AVL tree root, not mock implementations
- **Consistency**: All endpoints return consistent tracker state commitments that match the current AVL tree state
- **Debt Tracking**: Tracker maintains cumulative debt for each (issuer, recipient) pair

### Reserve Creation Payload Structure

The server provides an endpoint to generate reserve creation payloads for Ergo node's `/wallet/payment/send` API:

- `POST /reserves/create` - accepts a request with:
  - `nft_id`: String - the NFT ID to be stored in the reserve box (hex-encoded)
  - `owner_pubkey`: String - the 33-byte compressed public key (hex-encoded) of the reserve owner
  - `erg_amount`: u64 - the amount of ERG to lock in the reserve (in nanoERG)

- Returns a JSON response compatible with Ergo's `/wallet/payment/send` API:
  - `requests`: Array of payment requests
    - `address`: Reserve contract P2S address (hardcoded in configuration)
    - `value`: ERG amount from request
    - `assets`: Array containing the NFT asset
      - `token_id`: NFT ID from request (snake_case in the response; converted to `tokenId` by the submission endpoint)
      - `amount`: Always 1 for NFTs
    - `registers`: Map of register values
      - `R4`: Owner public key from request (GroupElement)
      - `R5`: Initial AVL tree (empty tree for new reserve)
      - `R6`: Tracker NFT ID (bytes) - identifies which tracker server this reserve is linked to
  - `fee`: Transaction fee amount from configuration
  - `change_address`: Change address derived from tracker public key configuration (fallback to owner pubkey if unavailable)

- `POST /reserves/submit` - Submit a previously generated reserve creation payload to the tracker's configured Ergo node for on-chain broadcast.
  - Accepts the same `ReserveCreationResponse` JSON returned by `/reserves/create`.
  - Converts `token_id` to the camelCase `tokenId` required by the Ergo node.
  - Forwards the `requests` array to `POST {ergo_node}/wallet/payment/send` using the configured `api_key`.
  - Returns:
    - `tx_id`: String - the transaction id returned by the Ergo node.
  - Errors:
    - `503 Service Unavailable` if no Ergo node is configured.
    - `502 Bad Gateway` if the Ergo node returns an error or is unreachable.

### Debt Transfer Support

The server supports debt transfer (novation) operations:

- `POST /debt/transfer` - Request debt transfer from one creditor to another
  - Request structure:
    - `debtor_pubkey`: Public key of the debtor (hex-encoded)
    - `current_creditor_pubkey`: Public key of the current creditor (hex-encoded)
    - `new_creditor_pubkey`: Public key of the new creditor (hex-encoded)
    - `transfer_amount`: Amount to transfer in nanoERG
  - Process:
    1. Server verifies debtor has sufficient debt to current creditor
    2. Server requests debtor's signature on transfer message
    3. Server atomically updates both debt records
    4. Server posts updated AVL tree commitment

## Configuration

The server supports configuration via:

1. Configuration files (config/basis.toml)
2. Environment variables (with BASIS_ prefix)
3. Default fallback values

Key configuration includes:
- Server host/port
- **`server.data_dir`**: Base directory for all on-disk storage (databases, indices,
  scanner metadata). Defaults to `data` relative to the server's working directory.
  Can be overridden with the `BASIS_SERVER_DATA_DIR` environment variable.
  The legacy `server.database_url` field is kept for compatibility but is currently unused.
- **`server.tls_cert_path` / `server.tls_key_path`**: Paths to PEM-encoded TLS certificate and key.
  When both are set, the server listens on HTTPS. Strongly recommended whenever auth is enabled.
- **`server.auth`**: Authentication and authorization settings:
  - `mode`: `none`, `api_key`, or `signature`.
  - `api_key`: shared secret used when `mode = "api_key"`.
  - `authorized_clients`: list of `{ pubkey, role }` entries used when `mode = "signature"`.
  - `allowed_origins`: CORS allow-list used when auth is enabled.
  - `signature_timestamp_tolerance_ms`: maximum age of a request signature in milliseconds (default 30 s).
- **Ergo node connection details** (required): The server will abort with exit code 1 if `ergo.node.node_url` is not provided in the configuration - no default localhost value is used
- Reserve contract P2S address
- Tracker NFT ID (for tracker scanner registration and state commitment monitoring)
- Tracker public key (for identifying the tracker server)
- Tracker API key (for authenticating requests to the Ergo node's signing API)
- Transaction fees

**Critical Requirements**:
1. The server requires a valid Ergo node URL to be provided in the configuration (`ergo.node.node_url` field). If this is missing or empty, the server will immediately exit with status code 1 during startup.
2. The server requires access to an Ergo node with the Schnorr signing API (`/utils/schnorrSign`) enabled for endpoints that require tracker signatures. The tracker private key must be available in the Ergo node's wallet for signature generation.
3. The tracker public key must be provided in the configuration for signature verification purposes.
4. The tracker API key must be provided to authenticate requests to the Ergo node's signing API.
5. When `server.auth.mode` is not `none`, configure TLS (`server.tls_cert_path` and `server.tls_key_path`) for production deployments so credentials and signatures are not sent in plaintext.

## Blockchain Integration

The server integrates with the Ergo blockchain through:

1. **Ergo Scanner**: Monitors the blockchain for reserve box events
2. **Tracker Scanner**: Monitors tracker state commitment boxes using the tracker NFT ID to enable cross-verification and state synchronization
3. **Reserve Event Processing**: Handles reserve creation, top-ups, and redemptions
4. **Real-time Updates**: Tracks collateralization ratios and reserve status
5. **Scan Registration**: Automatically registers both reserve and tracker scans with the Ergo node using the `/scan` API
6. **AVL Tree Verification**: Verifies on-chain AVL tree commitments match off-chain state

## Event Store

The server maintains an in-memory event store with:
- Sequential ID generation
- Pagination support
- Thread-safe operations using async mutex
- Planned persistence layer

## Error Handling

The server implements comprehensive error handling:

- Validation of hex-encoded public keys and signatures
- Proper HTTP status codes (200, 400, 500)
- Detailed error messages for debugging
- Graceful fallback when blockchain scanner is unavailable
- AVL tree proof validation errors
- Emergency redemption timeout handling
- Acceptance policy enforcement at redemption time: normal redemptions via `POST /redeem` and `POST /redemption/build` are rejected with HTTP 400 when they would newly violate another debt holder's acceptance policy (failure id `failed_policy_violation`) or, on a distressed reserve, when the redeemer does not hold the issuer's oldest outstanding note (failure id `failed_not_oldest_note`). Configurable via `[redemption] enforce_acceptance_policy` (default `true`); see `specs/redemption_acceptance_policy.md`

## Authentication & Authorization

The server supports three authentication modes configured under `[server.auth]`:

- `none`: anonymous access (backward-compatible local development).
- `api_key`: shared secret via `Authorization: Bearer <key>` or `X-API-Key: <key>`.
- `signature`: per-client secp256k1 Schnorr request signatures.

Signature mode uses a canonical message:

```text
<METHOD>\n<PATH>\n<QUERY>\n<TIMESTAMP>\n<NONCE>\n<BODY_HASH>
```

where `BODY_HASH` is the lowercase hex SHA-256 of the raw request body. The server verifies signatures with `basis_offchain::schnorr::schnorr_verify`, enforces a timestamp tolerance (default 30 s), and rejects replayed `(pubkey, nonce)` pairs.

Role-based access control assigns each request a role:

- `Read`: `GET` endpoints and read-only `POST` queries (`/notes/state`, `/acceptance/check`).
- `Write`: state-changing endpoints such as `/notes`, `/redeem`, `/redemption/prepare`, `/tracker/signature`.
- `Admin`: reserve creation/submission (`/reserves/create`, `/reserves/submit`) and policy management (`/acceptance/policy`).

In `none` and `api_key` modes all authenticated requests receive `Admin`. In `signature` mode the role is taken from the matching `authorized_clients` entry.

TLS should be enabled (`tls_cert_path` / `tls_key_path`) whenever authentication is used in production, otherwise credentials and signatures travel in plaintext.

For full details see `specs/server/authentication_authorization.md`.

## Security Considerations

- CORS headers configured for cross-origin requests, with origin restriction when auth is enabled and `allowed_origins` is non-empty
- Input validation for all public keys and amounts
- Signature verification for note creation and debt transfer
- Channel-based communication to ensure thread safety
- Remote signature generation to protect private keys
- AVL tree proof verification to prevent fraud
- Authentication and RBAC on all endpoints except the health-check root (`GET /`)

## Blockchain Height Caching

The server implements intelligent blockchain height caching to reduce Ergo node API calls:

- **Cache Storage**: Blockchain height and fetch timestamp stored in `scanner_metadata` database partition
- **TTL**: 10 minutes (600,000 milliseconds)
- **Cache Key**: `"blockchain_height"`
- **Cache Value**: 16 bytes (8 bytes height + 8 bytes timestamp, both big-endian u64)
- **Behavior**: 
  - Returns cached height if < 10 minutes old
  - Fetches from Ergo node `/info` endpoint if cache expired or missing
  - Stores new height with current timestamp after fetching
  - Implemented in both `ergo_scanner.rs` and `tracker_scanner.rs`

## Current State Summary

The basis_server crate is a fully functional HTTP API server that:
- Manages IOU notes and redemption processes
- Monitors Ergo blockchain reserve events
- Provides real AVL tree proof mechanisms for the Basis protocol
- Generates real Schnorr signatures via Ergo node's signing API for redemption transactions
- Implements proper async/await patterns and error handling
- Supports configuration and event storage
- Includes comprehensive API endpoints for all Basis features
- Provides endpoints for real tracker signature generation (`/tracker/signature`)
- Offers redemption preparation with real proofs and signatures (`/redemption/prepare`)
- Supports redemption-specific proof generation (`/proof/redemption`)
- Integrates with shared tracker state for consistent AVL tree root commitments
- Uses secure remote signing via Ergo node API to protect private keys
- Supports debt transfer (novation) for triangular trade
- Handles emergency redemption after 3-day timeout

This crate serves as the central hub for the Basis Tracker system, connecting the blockchain layer with client applications through a well-defined HTTP interface with real cryptographic operations while maintaining security through remote signing.

## Context Extension Variables Reference

For redemption transactions prepared by the server:

| Variable | Type | Description | Required |
|----------|------|-------------|----------|
| #0 | Byte | Action byte (0x00 for redemption) | Yes |
| #1 | GroupElement | Receiver pubkey | Yes |
| #2 | Coll[Byte] | Reserve owner's signature bytes | Yes |
| #3 | Long | Total debt amount | Yes |
| #5 | Coll[Byte] | AVL proof for reserve tree insert/update | Yes |
| #6 | Coll[Byte] | Tracker's signature bytes | Yes |
| #7 | Coll[Byte] | AVL proof for reserve tree lookup | No (omit for first redemption) |
| #8 | Coll[Byte] | AVL proof for tracker tree lookup | Yes |
