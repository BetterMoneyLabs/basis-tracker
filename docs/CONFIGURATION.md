# Basis Tracker Configuration

## Overview

This document describes the configuration options for the Basis Tracker server and CLI.

## Configuration File

The main configuration file is `config/basis.toml`. The server will look for this file in the following locations:

1. Current working directory: `config/basis.toml`
2. Environment variables with `BASIS_` prefix
3. Default values

## Configuration Sections

### Server Configuration

```toml
[server]
host = "0.0.0.0"        # Host address to bind to
port = 3048             # Port to listen on
data_dir = "data"       # Base directory for all on-disk storage (databases, indices, scanner metadata)
database_url = "sqlite:data/basis.db"  # Legacy field, kept for compatibility (currently unused)
# tls_cert_path = "server.crt"  # PEM-encoded TLS certificate chain (enable for production auth)
# tls_key_path = "server.key"   # PEM-encoded TLS private key
```

`data_dir` controls where the server writes all persistent state. It defaults to a `data/` directory relative to the working directory from which the server is launched. You can override it with the `BASIS_SERVER_DATA_DIR` environment variable.

### Server Authentication & TLS

The tracker server supports optional TLS and three authentication modes under
`[server.auth]`:

| Mode | Description |
|---|---|
| `none` | Anonymous requests (default, local development only) |
| `api_key` | Shared secret via `Authorization: Bearer <key>` or `X-API-Key: <key>` |
| `signature` | Per-request secp256k1 Schnorr signatures |

```toml
[server.auth]
mode = "signature"
# api_key = "change-me"
# [[server.auth.authorized_clients]]
# pubkey = "020202020202020202020202020202020202020202020202020202020202020202"
# role = "admin"
allowed_origins = []
signature_timestamp_tolerance_ms = 30000
```

When auth is enabled, configure TLS (`tls_cert_path` / `tls_key_path`) so
credentials and signatures are not sent in plaintext. Clients authenticate with
the same `server_auth_*` fields in `~/.basis/cli.toml`; the MCP server can also
read them from environment variables. See `specs/server/authentication_authorization.md`
for the full signature scheme and RBAC rules.

### Ergo Blockchain Configuration

```toml
[ergo]
# Basis reserve contract P2S address
basis_reserve_contract_p2s = "3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT"

# Starting block height for scanning (legacy)
start_height = 0

# Tracker NFT ID (hex-encoded) - REQUIRED for reserve creation and redemption
# This NFT identifies the tracker server and must be set in reserve contract R6 register
# Example: tracker_nft_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
tracker_nft_id = ""

# Tracker public key - can be either:
# 1. Hex-encoded compressed public key (33 bytes = 66 hex chars): "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7"
# 2. Ergo P2PK address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33"
tracker_public_key = ""

[ergo.node]
url = "http://159.89.116.15:11088"   # Ergo node URL
api_key = "hello"                    # API key for authenticated nodes
timeout_secs = 30                    # Request timeout in seconds
```

### Redemption Configuration

```toml
[redemption]
# Enforce acceptance-policy compliance on redemption requests (default: true)
enforce_acceptance_policy = true
```

When `enforce_acceptance_policy` is enabled, the tracker checks every redemption
request (`POST /redeem` and `POST /redemption/build`) against the acceptance
policies of the reserve issuer's **other** debt holders before signing:

- The tracker simulates the post-redemption collateralization ratio
  (`(collateral - amount) / (total_debt - amount)`) and rejects the request with
  HTTP 400 `failed_policy_violation` if any other holder's policy would be newly
  violated.
- If **all** other holders' policies are already violated (distressed,
  undercollateralized reserve), a FIFO fallback applies: only the issuer's
  **oldest outstanding note** (minimum timestamp) may be redeemed; other requests
  are rejected with HTTP 400 `failed_not_oldest_note` (the message includes the
  oldest note's timestamp).

Set it to `false` to disable these checks (e.g. for testing). Emergency
redemptions via the contract path bypass the tracker and are never
policy-checked. See `specs/redemption_acceptance_policy.md` for the full design.

### Confirmation Configuration

```toml
[confirmation]
# Minimum on-chain depth (including the inclusion block) before a tracker box
# update is treated as confirmed and pending notes become redeemable
min_depth = 2
```

Presence of the tracker update transaction in a block alone is not treated as
confirmation; the tracker box updater waits until the transaction has
`min_depth` blocks before promoting pending notes to `Confirmed` (making them
redeemable). Box/height fetch failures are retried on the next update cycle.
Note this is only a depth gate — reorg handling beyond it is out of scope
(see `specs/pr12_triage.md`).

### Node HTTP bounds

All outbound Ergo node requests share one bounded client
(`crates/basis_store/src/http.rs`): 3 s connect timeout, 15 s total request
timeout, and a 2 MiB response body cap. These values are currently compile-time
constants, not config keys.

## Tracker NFT Configuration

### What is the Tracker NFT?

The Tracker NFT is a critical component of the Basis system that:

1. **Identifies your tracker server** on the blockchain
2. **Links reserves to your tracker** via the R6 register
3. **Prevents unauthorized redemptions** by verifying tracker identity
4. **Enables multi-tracker support** (future feature)

### How to Set Up Tracker NFT

1. **Create an NFT** on the Ergo blockchain
   - Use any NFT creation tool or wallet
   - The NFT should be unique to your tracker instance

2. **Configure the NFT ID** in `config/basis.toml`
   ```toml
   tracker_nft_id = "your_nft_token_id_here"
   ```

3. **Use the NFT in reserve creation**
   - When creating reserves, the NFT ID must be set in the R6 register
   - This links the reserve to your specific tracker

### Reserve Contract Registers

When creating a reserve contract box, you must set these registers:

- **R4**: Issuer's public key (GroupElement)
- **R5**: AVL tree tracking cumulative redeemed amounts (initially empty)
  - Stores: `hash(ownerKey || receiverKey) -> cumulativeRedeemedAmount`
  - Updated on each redemption to prevent double-spending
- **R6**: Tracker NFT ID (from your configuration)

## Environment Variables

All configuration options can also be set via environment variables with the `BASIS_` prefix:

```bash
export BASIS_SERVER_HOST="0.0.0.0"
export BASIS_SERVER_PORT=3048
export BASIS_ERGO_BASIS_RESERVE_CONTRACT_P2S="your_reserve_contract_p2s"
export BASIS_ERGO_TRACKER_NFT_ID="your_tracker_nft_id"
export BASIS_ERGO_NODE_URL="http://your-node:9053"
```

## Tracker Public Key Configuration

### What is the Tracker Public Key?

The Tracker Public Key is used by the tracker server to:

1. **Sign and submit transactions** that update the tracker box state commitments on-chain
2. **Identify your tracker server instance** in tracker box R4 register
3. **Enable automated tracker box updates** every 10 minutes

### How to Configure Tracker Public Key

1. **Prepare a compressed secp256k1 public key** (33 bytes)
   - Can be provided as hex string: `02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7`
   - Can be provided as Ergo P2PK address: `9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33`

2. **Configure the public key** in `config/basis.toml`
   ```toml
   tracker_public_key = "your_public_key_or_p2pk_address_here"
   ```

3. **The tracker will use this key** to sign transactions updating the tracker box R4 register

### Format Options

The tracker public key supports two formats:
- **Hex format**: 66 hexadecimal characters representing 33 bytes (e.g., `02abcd...`)
- **P2PK address**: Base58 encoded Ergo address starting with '9' (mainnet) or '3' (testnet)

## Default Configuration

If no configuration file is found, the server uses these defaults:

```toml
[server]
host = "0.0.0.0"
port = 3048
data_dir = "data"
database_url = "sqlite:data/basis.db"
# tls_cert_path = "server.crt"
# tls_key_path = "server.key"

[server.auth]
mode = "none"
api_key = ""
allowed_origins = []
signature_timestamp_tolerance_ms = 30000

[ergo]
basis_reserve_contract_p2s = ""
start_height = 0
tracker_nft_id = ""

[ergo.node]
url = "http://159.89.116.15:11088"
api_key = "hello"
timeout_secs = 30

[redemption]
enforce_acceptance_policy = true

[confirmation]
min_depth = 2
```

## Verification

To verify your configuration is working:

1. **Start the server**: `cargo run -p basis_server`
2. **Check server logs** for configuration loading messages
3. **Test with CLI**: `basis-cli status`
4. **Verify tracker NFT**: Check that reserves can be created and redeemed

## Troubleshooting

### Common Issues

1. **"Tracker NFT not configured"**
   - Ensure `tracker_nft_id` is set in your configuration
   - The NFT must exist on the blockchain

2. **"Invalid tracker NFT"**
   - Verify the NFT ID is correctly hex-encoded
   - Check that the NFT exists and is owned by you

3. **"Reserve creation failed"**
   - Ensure R6 register contains the correct tracker NFT ID
   - Verify the reserve contract template is correct

### Logging

Enable debug logging to see configuration details:

```bash
RUST_LOG=debug cargo run -p basis_server
```
