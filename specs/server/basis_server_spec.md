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
- `GET /notes/issuer/{pubkey}` - Get all notes issued by a public key
- `GET /notes/recipient/{pubkey}` - Get all notes received by a public key
- `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}` - Get specific note between two parties
- `POST /redeem` - Initiate redemption process
- `POST /redeem/complete` - Complete redemption process
- `GET /proof` - Get proof for a specific note

### Reserve Endpoints

- `GET /reserves` - Get all reserve information
- `GET /reserves/issuer/{pubkey}` - Get reserves for a specific issuer
- `GET /key-status/{pubkey}` - Get status information for a public key

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

### IOU Note Structure

The server handles IOU (I Owe You) notes that represent debt obligations:

- `recipient_pubkey`: Public key of the recipient
- `amount_collected`: Total amount collected
- `amount_redeemed`: Amount already redeemed
- `timestamp`: Creation timestamp
- `signature`: Cryptographic signature

## Configuration

The server supports configuration via:

1. Configuration files (config/basis.toml)
2. Environment variables (with BASIS_ prefix)
3. Default fallback values

Key configuration includes:
- Server host/port
- Ergo node connection details
- Reserve contract P2S address
- Tracker NFT ID (for tracker scanner registration and state commitment monitoring)
- Transaction fees

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
- Provides proof mechanisms for the Basis protocol
- Implements proper async/await patterns and error handling
- Supports configuration and event storage
- Includes comprehensive API endpoints for all Basis features

This crate serves as the central hub for the Basis Tracker system, connecting the blockchain layer with client applications through a well-defined HTTP interface.