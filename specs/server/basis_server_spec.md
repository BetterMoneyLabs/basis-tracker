# Basis Server Crate Specification

## Overview

The `basis_server` crate is a Rust web server built with the Axum framework that provides an HTTP API for the Basis Tracker system. It serves as the core component for managing IOU notes, tracking reserve events on the Ergo blockchain, and providing proof mechanisms for the Basis protocol.

## Architecture

### Main Components

1. **API Module**: Contains all HTTP route handlers for the web server
2. **Reserve API Module**: Handles reserve-specific endpoints
3. **Models Module**: Defines data structures for API requests/responses
4. **Store Module**: Implements event storage functionality
5. **Config Module**: Handles application configuration
6. **Tracker Thread**: Background task that processes commands via message passing

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
- `GET /proof` - Get proof for a specific note
- `POST /tracker/signature` - Request tracker signature for redemption (real Schnorr signature generation)
- `POST /redemption/prepare` - Prepare redemption with all necessary data (real AVL proof + tracker signature)
- `GET /proof/redemption` - Get redemption-specific proof with tracker state digest

### Reserve Endpoints

- `GET /reserves` - Get all reserve information
- `GET /reserves/issuer/{pubkey}` - Get reserves for a specific issuer
- `GET /key-status/{pubkey}` - Get status information for a public key
- `POST /reserves/create` - Create a reserve creation payload for Ergo node's `/wallet/payment/send` API

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

### Tracker Box Registers

The tracker box uses Ergo registers R4 and R5 to store commitment information:

- `R4`: Contains the tracker's public key (33-byte compressed secp256k1 point) that identifies the tracker server
- `R5`: Contains the AVL+ tree root digest (33-byte commitment to all notes in the system), updated whenever notes are added or modified

### Tracker State Digest Format

The tracker state digest follows the AVL+ tree format (33 bytes total):
- **Byte 1**: Tree height (1 byte) - indicates the depth of the AVL tree
- **Bytes 2-33**: 32-byte hash of the AVL tree root (64 hex characters when encoded)
- **Total**: 33 bytes (66 hex characters when hex-encoded)
- **Type Identifier**: When serialized as SAvlTree, includes a type identifier (0x64) as the first byte of the serialized format

### IOU Note Structure

The server handles IOU (I Owe You) notes that represent debt obligations:

For most endpoints:
- `recipient_pubkey`: Public key of the recipient
- `amount_collected`: Total amount collected
- `amount_redeemed`: Amount already redeemed
- `timestamp`: Creation timestamp
- `signature`: Cryptographic signature

For the `GET /notes` endpoint (all notes), additional fields are included:
- `issuer_pubkey`: Public key of the issuer
- `age_seconds`: Age of the note in seconds (calculated from timestamp)

### Tracker Signature Request Structure

The `/tracker/signature` endpoint accepts requests with the following structure:
- `issuer_pubkey`: Public key of the note issuer (hex-encoded, 33 bytes)
- `recipient_pubkey`: Public key of the note recipient (hex-encoded, 33 bytes)
- `amount`: Redemption amount in nanoERG
- `timestamp`: Unix timestamp of the redemption request
- `recipient_address`: Ergo address where funds should be sent
- `reserve_box_id`: ID of the reserve box being spent

### Tracker Signature Response Structure

The `/tracker/signature` endpoint returns responses with the following structure:
- `success`: Boolean indicating if the signature generation was successful
- `tracker_signature`: 65-byte Schnorr signature (hex-encoded, 130 characters) proving tracker authorization
- `tracker_pubkey`: Tracker's public key (hex-encoded, 66 characters)
- `message_signed`: The hex-encoded message that was signed

### Redemption Preparation Request Structure

The `/redemption/prepare` endpoint accepts requests with the following structure:
- `issuer_pubkey`: Public key of the note issuer (hex-encoded, 33 bytes)
- `recipient_pubkey`: Public key of the note recipient (hex-encoded, 33 bytes)
- `amount`: Redemption amount in nanoERG
- `timestamp`: Unix timestamp of the redemption request

### Redemption Preparation Response Structure

The `/redemption/prepare` endpoint returns responses with the following structure:
- `redemption_id`: Unique identifier for the redemption process
- `avl_proof`: AVL+ tree lookup proof for the specific note (hex-encoded bytes)
- `tracker_signature`: 65-byte Schnorr signature from tracker (hex-encoded, 130 characters)
- `tracker_pubkey`: Tracker's public key (hex-encoded, 66 characters)
- `tracker_state_digest`: 33-byte AVL tree root digest (hex-encoded, 66 characters) representing current tracker state
- `block_height`: Current blockchain height at time of proof generation

### Redemption Proof Response Structure

The `/proof/redemption` endpoint returns responses with the following structure:
- `issuer_pubkey`: Public key of the note issuer (hex-encoded, 66 characters)
- `recipient_pubkey`: Public key of the note recipient (hex-encoded, 66 characters)
- `proof_data`: AVL+ tree lookup proof for the specific note (hex-encoded bytes)
- `tracker_state_digest`: 33-byte AVL tree root digest (hex-encoded, 66 characters) representing current tracker state
- `block_height`: Current blockchain height at time of proof generation
- `timestamp`: Unix timestamp of the proof generation

### Real Cryptographic Implementation

The server now implements real cryptographic functionality using the Ergo node's Schnorr signing API instead of mock implementations:

#### Schnorr Signature Generation
- **Remote Signatures**: All signature endpoints now use the Ergo node's `/utils/schnorrSign` API for actual Schnorr signature generation
- **Format**: 65-byte signatures (33 bytes for 'a' component + 32 bytes for 'z' component)
- **Structure**: Properly formatted with compressed public key prefix (0x02 or 0x03) followed by the signature components
- **Security**: Private keys remain secured within the Ergo node, with the tracker only requesting signatures for specific messages
- **Authentication**: Requests to the signing API are authenticated using the tracker API key
- **Implementation**: Tracker signature endpoints (`/tracker/signature` and `/redemption/prepare`) now make HTTP requests to the Ergo node API instead of performing local signing
- **Message Format**: Signing messages follow the format `recipient_pubkey || amount_be_bytes || timestamp_be_bytes` as specified in the Schnorr signature specification

#### AVL+ Tree Proof Generation
- **Real Proofs**: All proof endpoints now generate actual AVL+ tree lookup proofs from the tracker's AVL tree state
- **Format**: Properly formatted proof data that demonstrates existence of key-value pairs in the AVL tree
- **State Commitment**: Tracker state digest properly formatted as 33-byte AVL tree root (1 byte height + 32 bytes hash)
- **Integration**: Proofs are generated by the actual tracker state manager using the AVL tree implementation

#### Tracker State Management
- **Shared State**: Tracker state is maintained in shared state accessible via `state.shared_tracker_state`
- **Real Digests**: Tracker state digests come from actual AVL tree root, not mock implementations
- **Consistency**: All endpoints return consistent tracker state commitments that match the current AVL tree state

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
      - `token_id`: NFT ID from request
      - `amount`: Always 1 for NFTs
    - `registers`: Map of register values
      - `R4`: Owner public key from request
      - `R5`: Tracker NFT ID (if configured) or provided NFT ID
  - `fee`: Transaction fee amount from configuration
  - `change_address`: "default" placeholder (filled by wallet)

## Configuration

The server supports configuration via:

1. Configuration files (config/basis.toml)
2. Environment variables (with BASIS_ prefix)
3. Default fallback values

Key configuration includes:
- Server host/port
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

## Blockchain Integration

The server integrates with the Ergo blockchain through:

1. **Ergo Scanner**: Monitors the blockchain for reserve box events
2. **Tracker Scanner**: Monitors tracker state commitment boxes using the tracker NFT ID to enable cross-verification and state synchronization
3. **Reserve Event Processing**: Handles reserve creation, top-ups, and redemptions
4. **Real-time Updates**: Tracks collateralization ratios and reserve status
5. **Scan Registration**: Automatically registers both reserve and tracker scans with the Ergo node using the `/scan` API

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

## Security Considerations

- CORS headers configured for cross-origin requests
- Input validation for all public keys and amounts
- Signature verification for note creation
- Channel-based communication to ensure thread safety

## Current State Summary

The basis_server crate is a fully functional HTTP API server that:
- Manages IOU notes and redemption processes
- Monitors Ergo blockchain reserve events
- Provides real AVL+ tree proof mechanisms for the Basis protocol
- Generates real Schnorr signatures via Ergo node's signing API for redemption transactions
- Implements proper async/await patterns and error handling
- Supports configuration and event storage
- Includes comprehensive API endpoints for all Basis features
- Provides endpoints for real tracker signature generation (`/tracker/signature`)
- Offers redemption preparation with real proofs and signatures (`/redemption/prepare`)
- Supports redemption-specific proof generation (`/proof/redemption`)
- Integrates with shared tracker state for consistent AVL tree root commitments
- Uses secure remote signing via Ergo node API to protect private keys

This crate serves as the central hub for the Basis Tracker system, connecting the blockchain layer with client applications through a well-defined HTTP interface with real cryptographic operations while maintaining security through remote signing.