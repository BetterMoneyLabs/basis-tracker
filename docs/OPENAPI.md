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

When TLS is configured, use `https://localhost:3048` instead.

## Authentication

The tracker server supports three authentication modes, configured under
`[server.auth]` in `config/basis.toml`:

- **`none`** (default): anonymous requests; suitable for local development only.
  Every request is granted `Admin` access, so the server refuses to start in
  this mode unless `server.host` is a loopback address (`127.0.0.1`, `::1`,
  or `localhost`).
- **`api_key`**: clients send a shared secret in the `Authorization: Bearer <key>`
  or `X-API-Key: <key>` header. API-key clients are granted full (`Admin`) access.
- **`signature`**: clients authenticate with per-request secp256k1 Schnorr
  signatures. The server must list the client's public key and role under
  `server.auth.authorized_clients`.

### Signature mode

For each request the client provides:

| Header | Value |
|--------|-------|
| `X-Signature-Pubkey` | 66-character compressed secp256k1 public key |
| `X-Signature` | 130-character Schnorr signature |
| `X-Signature-Timestamp` | Unix timestamp in milliseconds |
| `X-Signature-Nonce` | Unique nonce (required; requests without one are rejected) |

The signature covers the canonical string:

```text
<METHOD>\n<PATH>\n<QUERY>\n<TIMESTAMP>\n<NONCE>\n<BODY_HASH>
```

where `BODY_HASH` is the lowercase hex SHA-256 of the raw request body. The
`GET /` health check is always public.

### Authorization

When authentication is enabled, routes require the following roles:

| Role | Access |
|------|--------|
| `Read` | All `GET` endpoints plus read-only `POST` endpoints (`/notes/state`, `/acceptance/check`) |
| `Write` | `Read` access plus note creation, redemption, and `/tracker/signature` |
| `Admin` | `Write` access plus `/reserves/create` and `/acceptance/policy` |

### Authentication

`server.auth.mode` accepts `"none"`, `"api_key"` or `"signature"`. **The server refuses to start with
`mode = "none"` on a non-loopback bind address**, because in that mode every request is granted
`Admin`. Set an authenticated mode, bind to `127.0.0.1`, or set
`server.auth.allow_anonymous_non_loopback = true` to accept the exposure deliberately.

When auth is enabled and `allowed_origins` is empty, **no CORS layer is installed** — browsers cannot
call the tracker cross-origin. There is no wildcard origin. Add `allowed_origins` only for web clients
you trust.

All three official clients support authentication:
`basis_cli` and `basis_app` (TUI) read `server_auth_mode` and credentials from
`~/.basis/cli.toml`; `basis_mcp` reads the same settings from environment
variables first, falling back to `~/.basis/cli.toml`.

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

- `POST /notes` - Create a new IOU note (requires `tracker_nft_id`, which must match this tracker's configured NFT id — see "Signing Message Format" below; optional `denomination` field, `"erg"` or `"token:<64-hex>"`, defaulting to the pair's existing note's denomination when unambiguous, else ERG)
- `POST /notes/exchange` - Exchange debt between two denominations of one (issuer, recipient) pair (off-chain novation / triage; both parties sign both edges' new states — the recipient's signatures are the creditor consent required for a debt decrease)
- `GET /notes` - Get all IOU notes with age and confirmation state
- `GET /notes/issuer/{pubkey}` - Get all notes issued by a public key
- `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}` - Get a specific note (optional `?denomination=` query parameter)
- `GET /notes/recipient/{pubkey}` - Get all notes where the public key is the recipient
- `POST /notes/state` - Get confirmation state for a single note (optional `denomination` field)

### Reserves

- `GET /reserves` - Get all reserves
- `GET /reserves/{box_id}` - Get a specific reserve by box ID
- `GET /reserves/issuer/{pubkey}` - Get reserves for an issuer
- `POST /reserves/create` - Build a reserve creation payment request

`POST /reserves/create` only *builds* the payment request. To create the
reserve on-chain, submit the returned `requests` array to your own Ergo
node's wallet directly:

```bash
curl -X POST http://your-ergo-node:9053/wallet/payment/send \
  -H "Content-Type: application/json" \
  -H "api_key: <your-node-api-key>" \
  -d '<the requests array from the /reserves/create response>'
```

The tracker no longer exposes an endpoint that forwards payments to its
configured Ergo node wallet.

### Events

- `GET /events` - Get the 50 most recent tracker events
- `GET /events/paginated` - Get paginated tracker events

### Status

- `GET /key-status/{pubkey}` - Get key status as a per-denomination breakdown (`denominations[]`, each with debt/collateral/ratio in that denomination's units; debt and collateral are never summed across denominations)

### Redemption

- `POST /redeem` - Initiate redemption of an IOU note
- `POST /redeem/complete` - Mark a redemption as completed locally (amounts are validated against the stored note state; a mismatch returns 400)
- `POST /redemption/prepare` - Prepare a redemption (legacy tracker-signature + proof endpoint; the note must exist and the stored note's cumulative debt and timestamp are signed — the request's `timestamp` field is ignored)
- `POST /redemption/build` - Build an unsigned redemption transaction and sign fee inputs (the tracker proof is generated against the last on-chain-confirmed tracker state; returns 400 if the note is not yet in the confirmed state)
- `POST /redemption/submit` - Broadcast a fully-signed redemption transaction (note key and amounts are re-validated against stored state before broadcasting; a mismatch returns 400)

### Proofs

- `GET /proof/redemption` - Generate a single AVL proof for a note
- `GET /tracker/proof` - Get tracker lookup proof (context extension variable #8)
- `GET /reserve/proof` - Get reserve lookup/insert proofs (context extension variables #5 and #7)

### Tracker

- `POST /tracker/signature` - Request a tracker Schnorr signature for redemption (signs the stored note's `total_debt`/`timestamp`; the request fields of the same names are legacy and ignored; returns 404 if no note is stored for the pair)
- `GET /tracker/state` - Get local/confirmed/pending tracker state digests
- `GET /tracker/pending-tx` - Get the in-flight tracker update transaction
- `GET /tracker/latest-box-id` - Get the latest on-chain tracker box ID

### Acceptance

- `POST /acceptance/check` - Check whether a note would be accepted by the server's policy (collateralization is evaluated per denomination: against the issuer's tracked reserve of the note's currency)
- `POST /acceptance/policy` - Upload a signed per-recipient acceptance policy
- `GET /acceptance/policy/{pubkey}` - Get a recipient's uploaded acceptance policy

### Config

- `GET /config/reserve-contract-p2s` - Get the configured Basis reserve contract P2S address
- `GET /config/reserve-token` - Get reserve token configuration (token-backed reserves)
- `GET /config/reserve-collateral` - Get the reserve-collateral configuration: ERG and token reserve contract addresses plus the served-token table (symbol, decimals, per-token policy)

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
  "tracker_nft_id": "a1b2c3d4...",
  "denomination": "erg",
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
    "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
    "tracker_nft_id": "a1b2c3d4e5f67788990011223344556677889900112233445566778899001122"
  }'
```

The `tracker_nft_id` field (32 bytes, hex) is required. It is part of the signed message (appended
after `key || totalDebt || timestamp`), binding the note to one tracker. The tracker validates it
against its configured `ergo.tracker_nft_id`: a malformed hex string or wrong length returns
`400`, and an id that does not match this tracker's NFT returns `409`.

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

The request's optional `change_address` field is deprecated and ignored:
change is always paid to the tracker's own wallet (derived from the selected
fee input, falling back to the configured change address).

The tracker AVL proof in the response is generated against the last
on-chain-confirmed tracker state digest (not the live local tree), so it
verifies against the digest stored in the tracker box. The request fails with
400 if no tracker state has been confirmed on-chain yet or the note is not part
of the confirmed state.

The request's `timestamp` field is legacy and ignored: the stored note's
payment timestamp is used for the tracker signature, context var #4, and the
reserve tree value.

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

The tracker always signs the stored note's cumulative `total_debt` and payment
`timestamp`; the request fields of the same names are legacy and ignored. The
acceptance policy is enforced for non-emergency requests before signing — it is
evaluated against the issuer's tracked reserve of the note's denomination (409
when that reserve is missing or not unique; a reserve of another currency is
never consulted) — and the endpoint returns 404 when no note is stored for the
issuer/recipient pair.

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

The tracker only **builds** the payload — there is no `/reserves/submit` endpoint. The
reserve owner broadcasts it themselves by posting the returned `requests` array to their
own Ergo node's `/wallet/payment/send`, so the tracker's wallet is never involved (PR #16
B4). The owner's wallet must hold the reserve NFT and enough ERG for the value plus fee.
`nft_id` must not be the tracker's own NFT, and amounts are bounded by the
`server.reserve_creation_max_erg_nano` / `reserve_creation_max_token_amount` config caps.

### Get Tracker State

```bash
curl http://localhost:3048/tracker/state
```

## Signing Message Format

The Schnorr signatures used in notes and redemptions sign the following 80-byte message:

```
debtKey || totalDebt || timestamp || trackerNftId
```

where:

- `debtKey = blake2b256(ownerKeyBytes || receiverKeyBytes)` for ERG, or `blake2b256(ownerKeyBytes || receiverKeyBytes || tokenId)` for token denominations (32 bytes)
- `totalDebt` is the cumulative debt amount (8 bytes, big-endian)
- `timestamp` is the note payment timestamp (8 bytes, big-endian)
- `trackerNftId` is the tracker's NFT id (32 bytes), binding the signature to one specific tracker/contract — on-chain the contract reads it from the tracker box's `R6` register and appends it, so a signature made for one tracker cannot be replayed on another

The same message format is used for both normal and emergency redemption. In an emergency redemption, the tracker signature is optional.

## Context Extension Variables

Redemption transactions use context extension variables to pass data to the Basis contract:

| Variable | Purpose | Source |
|----------|---------|--------|
| **#0** | Action byte (0x00 = redemption) | Constant |
| **#1** | Receiver's public key (33 bytes) | From request |
| **#2** | Reserve owner's Schnorr signature (65 bytes) | From issuer wallet |
| **#3** | Total debt amount (8 bytes) | From tracker AVL proof |
| **#4** | Note payment timestamp (8 bytes) | From the stored note (server-authoritative) |
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
