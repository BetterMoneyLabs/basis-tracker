# Basis Tracker OpenAPI Documentation

This directory contains the OpenAPI 3.0 specification for the Basis Tracker HTTP API.

## Files

- `openapi.yaml` - Complete OpenAPI specification in YAML format
- `openapi.json` - Basic OpenAPI specification in JSON format (currently minimal)

## API Overview

The Basis Tracker API provides RESTful endpoints for managing decentralized debt issuance and tracking. The API supports:

- **IOU Note Management**: Create and retrieve debt notes between issuers and recipients
- **Reserve Tracking**: Monitor collateral reserves for debt issuance
- **Event Monitoring**: Track system events including note updates and reserve changes
- **Health Checks**: Basic API status verification

## Endpoints

### Health Check
- `GET /` - Returns "Hello, Basis Tracker API!"

### Notes Management
- `POST /notes` - Create a new IOU note
- `GET /notes/issuer/{pubkey}` - Get all notes for an issuer
- `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}` - Get specific note

### Reserve Management
- `GET /reserves/issuer/{pubkey}` - Get reserves for an issuer

### Event Monitoring
- `GET /events` - Get recent tracker events (50 most recent)
- `GET /events/paginated` - Get paginated tracker events

### Status and Monitoring
- `GET /key-status/{pubkey}` - Get comprehensive key status information

### Redemption Operations
- `POST /redeem` - Initiate redemption of an IOU note

### Proof Generation
- `GET /proof` - Generate proof for a specific note

## Data Formats

### Public Keys and Signatures
All public keys and signatures are hex-encoded strings:
- **Public Keys**: 33 bytes (66 hex characters)
- **Signatures**: 65 bytes (130 hex characters) - Schnorr format

### API Response Format
All endpoints return a standardized response format:
```json
{
  "success": boolean,
  "data": object | array | null,
  "error": string | null
}
```

### Error Handling
- **400 Bad Request**: Invalid input parameters
- **404 Not Found**: Resource not found
- **500 Internal Server Error**: Server-side error

## Usage Examples

### Create a Note
```bash
curl -X POST http://localhost:3000/notes \
  -H "Content-Type: application/json" \
  -d '{
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
    "amount": 1000,
    "timestamp": 1234567890,
    "signature": "0303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303",
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101"
  }'
```

### Get Notes by Issuer
```bash
curl http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101
```

### Get Events
```bash
# Get recent events
curl http://localhost:3000/events

# Get paginated events
curl "http://localhost:3000/events/paginated?page=0&page_size=10"
```

### Get Key Status
```bash
curl http://localhost:3000/key-status/010101010101010101010101010101010101010101010101010101010101010101
```

### Initiate Redemption
```bash
curl -X POST http://localhost:3000/redeem \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
    "amount": 500000000,
    "timestamp": 1234567890
  }'
```

### Get Proof
```bash
curl "http://localhost:3000/proof?issuer_pubkey=010101010101010101010101010101010101010101010101010101010101010101&recipient_pubkey=020202020202020202020202020202020202020202020202020202020202020202"
```

## Validation

The OpenAPI specification includes validation rules:
- Public keys must match pattern: `^[0-9a-fA-F]{66}$`
- Signatures must match pattern: `^[0-9a-fA-F]{130}$`
- Amounts must be positive integers
- Timestamps must be valid Unix timestamps

## Tools

You can use the OpenAPI specification with various tools:

### Swagger UI
```bash
# Install swagger-ui
npm install -g swagger-ui

# Serve the specification
swagger-ui openapi.yaml
```

### Redoc
```bash
# Install redoc
npm install -g redoc-cli

# Generate documentation
redoc-cli serve openapi.yaml
```

### OpenAPI Generator
```bash
# Generate client libraries
openapi-generator generate -i openapi.yaml -g typescript-axios -o ./client
```

## Development

The OpenAPI specification is automatically generated from the Rust codebase. When adding new endpoints or modifying existing ones, update both the Rust implementation and the OpenAPI specification.

## Related Documentation

- [HTTP_API.md](../HTTP_API.md) - Detailed API usage instructions
- [CRUSH.md](../CRUSH.md) - Development guidelines and commands
- [README.md](../README.md) - Project overview