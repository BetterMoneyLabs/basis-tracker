# Basis Tracker HTTP API

This document provides instructions for running the Basis Tracker HTTP API server.

## Prerequisites

- Rust and Cargo installed (latest stable version)
- Clone the basis-tracker repository

## Running the HTTP API Server

### Method 1: From the workspace root

```bash
cd /home/kushti/bml/basis-tracker

# Build and run the server
cargo run -p basis_server

# The server will start and display: "DEBUG basis_server: listening on 127.0.0.1:3000"
```

### Method 2: From the server crate directory

```bash
cd /home/kushti/bml/basis-tracker/crates/basis_server

# Build and run the server
cargo run
```

### Method 3: Build and run separately

```bash
cd /home/kushti/bml/basis-tracker

# Build the server
cargo build -p basis_server

# Run the built binary
./target/debug/basis_server
```

## Server Information

- **Host**: 127.0.0.1 (localhost)
- **Port**: 3000
- **Base URL**: http://localhost:3000

## Current Endpoints

### GET /
- **Description**: Basic health check endpoint
- **Response**: "Hello, Basis Tracker API!"
- **Example**:
  ```bash
  curl http://localhost:3000/
  ```

### POST /notes
- **Description**: Create a new IOU note
- **Request Body**:
  ```json
  {
    "recipient_pubkey": [byte array (33 bytes)],
    "amount": 1000,
    "timestamp": 1234567890,
    "signature": [byte array (64 bytes)],
    "issuer_pubkey": [byte array (33 bytes)]
  }
  ```
- **Response**: 
  ```json
  {
    "success": true,
    "data": null,
    "error": null
  }
  ```
- **Example**:
  ```bash
  curl -X POST http://localhost:3000/notes \
    -H "Content-Type: application/json" \
    -d '{
      "recipient_pubkey": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],
      "amount": 1000,
      "timestamp": 1234567890,
      "signature": [3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3],
      "issuer_pubkey": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]
    }'
  ```

### GET /notes/issuer/{pubkey}
- **Description**: Get all notes for a specific issuer
- **Path Parameter**: `pubkey` - Hex-encoded issuer public key (66 characters)
- **Response**: 
  ```json
  {
    "success": true,
    "data": [
      {
        "recipient_pubkey": "hex-encoded public key",
        "amount": 1000,
        "timestamp": 1234567890,
        "signature": "hex-encoded signature"
      }
    ],
    "error": null
  }
  ```
- **Example**:
  ```bash
  curl http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101
  ```

## Environment Variables

- `RUST_LOG`: Set logging level (default: `basis_server=debug,tower_http=debug`)

Example:
```bash
RUST_LOG=info cargo run -p basis_server
```

## Testing the API

Once the server is running, you can test it using curl:

```bash
curl http://localhost:3000/
```

Expected response:
```
Hello, Basis Tracker API!
```

## Stopping the Server

Press `Ctrl+C` in the terminal where the server is running to stop it gracefully.

## Next Steps

This is currently a stub implementation. Future development will add:

- RESTful endpoints for IOU note management
- Authentication and authorization
- Integration with the persistence layer
- WebSocket support for real-time updates
- OpenAPI/Swagger documentation

## Development

To add new endpoints, modify the `crates/basis_server/src/main.rs` file and add new route handlers using Axum's routing system.