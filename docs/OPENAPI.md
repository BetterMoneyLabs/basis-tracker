# Basis Tracker OpenAPI Documentation

This directory contains the OpenAPI 3.0 specification for the Basis Tracker HTTP API.

## Files

- `openapi.yaml` - Complete OpenAPI specification in YAML format
- `openapi.json` - Basic OpenAPI specification in JSON format (currently minimal)

## API Overview

The Basis Tracker API provides RESTful endpoints for managing decentralized debt issuance and tracking. The API supports:

- **IOU Note Management**: Create and retrieve debt notes between issuers and recipients
- **Reserve Tracking**: Monitor collateral reserves for debt issuance
- **Redemption Operations**: Build, prepare, submit, and complete note redemptions
- **Proof Generation**: Generate AVL proofs and tracker signatures for redemption transactions
- **Acceptance Policies**: Check and upload per-recipient note acceptance policies
- **Event Monitoring**: Track system events including note updates and reserve changes
- **Tracker State**: Inspect on-chain tracker box state and pending update transactions
- **Configuration**: Expose reserve contract and token configuration to clients

## Base URL

```
http://localhost:3048
```

## Response Format

All endpoints return a standardized response envelope:

```json
{
  "success": boolean,
  "data": object | array | null,
  "error": string | null
}
```

## Endpoints

### Health

- `GET /` - Health check. Returns `"Hello, Basis Tracker API!"`.

### Notes

- `POST /notes` - Create a new IOU note
- `GET /notes` - Get all IOU notes with age and confirmation state
- `GET /notes/issuer/{pubkey}` - Get all notes issued by a public key
- `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}` - Get a specific note
- `GET /notes/recipient/{pubkey}` - Get all notes where the public key is the recipient
- `POST /notes/state` - Get confirmation state for a single note

### Reserves

- `GET /reserves` - Get all reserves
- `GET /reserves/{box_id}` - Get a specific reserve by box ID
- `GET /reserves/issuer/{pubkey}` - Get reserves for an issuer
- `POST /reserves/create` - Build a reserve creation payment request
- `POST /reserves/submit` - Submit a reserve creation payment request to the configured Ergo node

### Events

- `GET /events` - Get the 50 most recent tracker events
- `GET /events/paginated` - Get paginated tracker events

### Status

- `GET /key-status/{pubkey}` - Get comprehensive key status information (debt, collateral, ratio)

### Redemption

- `POST /redeem` - Initiate redemption of an IOU note
- `POST /redeem/complete` - Mark a redemption as completed locally
- `POST /redemption/prepare` - Prepare a redemption (legacy tracker-signature + proof endpoint)
- `POST /redemption/build` - Build an unsigned redemption transaction and sign fee inputs
- `POST /redemption/submit` - Broadcast a fully-signed redemption transaction

### Proofs

- `GET /proof/redemption` - Generate a single AVL proof for a note
- `GET /tracker/proof` - Get tracker lookup proof (context extension variable #8)
- `GET /reserve/proof` - Get reserve lookup/insert proofs (context extension variables #5 and #7)

### Tracker

- `POST /tracker/signature` - Request a tracker Schnorr signature for redemption
- `GET /tracker/state` - Get local/confirmed/pending tracker state digests
- `GET /tracker/pending-tx` - Get the in-flight tracker update transaction
- `GET /tracker/latest-box-id` - Get the latest on-chain tracker box ID

### Acceptance

- `POST /acceptance/check` - Check whether a note would be accepted by the server's policy
- `POST /acceptance/policy` - Upload a signed per-recipient acceptance policy
- `GET /acceptance/policy/{pubkey}` - Get a recipient's uploaded acceptance policy

### Config

- `GET /config/reserve-contract-p2s` - Get the configured Basis reserve contract P2S address
- `GET /config/reserve-token` - Get reserve token configuration (token-backed reserves)

## Data Formats

### Public Keys and Signatures

All public keys and signatures are hex-encoded strings:

- **Public Keys**: 33 bytes (66 hex characters)
- **Signatures**: 65 bytes (130 hex characters) - Schnorr format

### Amounts and Timestamps

- **Amounts**: unsigned 64-bit integers, typically in nanoERG or raw token units
- **Note timestamps**: Unix timestamp in milliseconds for redemption-related calls; seconds for note creation

### IOU Note Response

```json
{
  "issuer_pubkey": "010101...",
  "recipient_pubkey": "020202...",
  "amount_collected": 1000,
  "amount_redeemed": 0,
  "timestamp": 1234567890,
  "signature": "030303...",
  "confirmation": {
    "status": "local_only",
    "confirmed_total_debt": null,
    "pending_total_debt": null,
    "confirmed_box_id": null,
    "confirmed_height": null,
    "pending_tx_id": null,
    "redeemable": true,
    "redeemable_amount": 1000
  }
}
```

### Error Handling

- **400 Bad Request**: Invalid input parameters
- **404 Not Found**: Resource not found
- **500 Internal Server Error**: Server-side error
- **502 Bad Gateway**: Ergo node returned an error
- **503 Service Unavailable**: Required external service (e.g., Ergo node) not configured

## Usage Examples

### Create a Note

```bash
curl -X POST http://localhost:3048/notes \
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
curl http://localhost:3048/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101
```

### Get Note State

```bash
curl -X POST http://localhost:3048/notes/state \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202"
  }'
```

### Get Events

```bash
# Get recent events
curl http://localhost:3048/events

# Get paginated events
curl "http://localhost:3048/events/paginated?page=0&page_size=10"
```

### Get Key Status

```bash
curl http://localhost:3048/key-status/010101010101010101010101010101010101010101010101010101010101010101
```

### Check Acceptance

```bash
curl -X POST http://localhost:3048/acceptance/check \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "total_debt": 1000000000,
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202"
  }'
```

### Initiate Redemption

```bash
curl -X POST http://localhost:3048/redeem \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
    "amount": 500000000,
    "timestamp": 1234567890000,
    "issuer_signature": "030303..."
  }'
```

### Build Redemption Transaction

```bash
curl -X POST http://localhost:3048/redemption/build \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
    "amount": 500000000,
    "timestamp": 1234567890000,
    "issuer_signature": "030303..."
  }'
```

### Get Tracker Proof

```bash
curl "http://localhost:3048/tracker/proof?issuer_pubkey=010101010101010101010101010101010101010101010101010101010101010101&recipient_pubkey=020202020202020202020202020202020202020202020202020202020202020202"
```

### Get Reserve Proof

```bash
curl "http://localhost:3048/reserve/proof?issuer_pubkey=010101010101010101010101010101010101010101010101010101010101010101&recipient_pubkey=020202020202020202020202020202020202020202020202020202020202020202&amount=500000000"
```

### Get Tracker Signature

```bash
curl -X POST http://localhost:3048/tracker/signature \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
    "total_debt": 1000000000,
    "timestamp": 1234567890000
  }'
```

### Create Reserve Payload

```bash
curl -X POST http://localhost:3048/reserves/create \
  -H "Content-Type: application/json" \
  -d '{
    "nft_id": "a1b2c3d4e5f67788990011223344556677889900112233445566778899001122",
    "owner_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "erg_amount": 2500000000
  }'
```

### Submit Reserve Transaction

```bash
curl -X POST http://localhost:3048/reserves/submit \
  -H "Content-Type: application/json" \
  -d '@reserve_creation_response.json'
```

### Get Tracker State

```bash
curl http://localhost:3048/tracker/state
```

## Signing Message Format

The Schnorr signatures used in notes and redemptions sign the following 48-byte message:

```
key || totalDebt || timestamp
```

where:

- `key = blake2b256(ownerKeyBytes || receiverKeyBytes)` (32 bytes)
- `totalDebt` is the cumulative debt amount (8 bytes, big-endian)
- `timestamp` is the note payment timestamp (8 bytes, big-endian)

The same message format is used for both normal and emergency redemption. In an emergency redemption, the tracker signature is optional.

## Context Extension Variables

Redemption transactions use context extension variables to pass data to the Basis contract:

| Variable | Purpose | Source |
|----------|---------|--------|
| **#0** | Action byte (0x00 = redemption) | Constant |
| **#1** | Receiver's public key (33 bytes) | From request |
| **#2** | Reserve owner's Schnorr signature (65 bytes) | From issuer wallet |
| **#3** | Total debt amount (8 bytes) | From tracker AVL proof |
| **#4** | Note payment timestamp (8 bytes) | From request |
| **#5** | Reserve insert proof | From `/reserve/proof` endpoint |
| **#6** | Tracker's Schnorr signature (65 bytes) | From `/tracker/signature` endpoint |
| **#7** | Reserve lookup proof (optional) | From `/reserve/proof` endpoint |
| **#8** | Tracker lookup proof | From `/tracker/proof` endpoint |

**Note:** Context variable #7 (reserve lookup proof) is omitted for first redemptions when `already_redeemed = 0`.

## AVL Proof Endpoints

The API provides three proof endpoints for redemption transactions:

1. **`GET /tracker/proof`** - Tracker lookup proof (context var #8)
   - Proves `totalDebt` exists in the tracker's AVL tree
   - Returns: `key`, `value`, `proof`, `total_debt`, `tracker_state_digest`

2. **`GET /reserve/proof`** - Reserve proofs (context var #5 and #7)
   - Provides both insert proof (#5) and lookup proof (#7)
   - Returns: `key`, `value`, `proof` (lookup), `insert_proof`, `already_redeemed`, `is_first_redemption`, `new_reserve_state_digest`

3. **`GET /proof/redemption`** - Legacy single proof endpoint
   - Returns a single hex-encoded AVL proof for a note

## Tools

You can use the OpenAPI specification with various tools:

### Swagger UI

```bash
npm install -g swagger-ui
swagger-ui openapi.yaml
```

### Redoc

```bash
npm install -g redoc-cli
redoc-cli serve openapi.yaml
```

### OpenAPI Generator

```bash
openapi-generator generate -i openapi.yaml -g typescript-axios -o ./client
```

## Development

When adding new endpoints or modifying existing ones, update both the Rust implementation and `openapi.yaml`. This file is maintained manually; regenerate client libraries after changes.

## Related Documentation

- `README.md` - Project overview
- `specs/server/` - Detailed server specifications
- `specs/ergo/` - Ergo blockchain integration specs
