# Basis Tracker Server Authentication & Authorization

## Overview

The Basis Tracker server supports multiple authentication modes and role-based access control (RBAC). Authentication protects tracker API endpoints from unauthorized clients; authorization further restricts what each authenticated client is allowed to do.

This document describes the supported modes, the signature scheme, RBAC rules, and how clients (CLI and MCP) supply credentials.

## Authentication Modes

The server is configured under `[server.auth]` with `mode` set to one of:

| Mode | Description | Use Case |
|---|---|---|
| `none` | Anonymous access. | Local development only. |
| `api_key` | Shared secret sent in `Authorization: Bearer <key>` or `X-API-Key: <key>`. | Server-to-server or MCP clients. |
| `signature` | Per-client secp256k1 Schnorr request signatures. | High-security production deployments. |

When auth is enabled, the server also logs a warning if TLS is not configured, because credentials and signatures would travel over plaintext.

### Configuration Example

```toml
[server]
host = "0.0.0.0"
port = 3048
# TLS is strongly recommended whenever auth is enabled.
tls_cert_path = "server.crt"
tls_key_path = "server.key"

[server.auth]
mode = "signature"
# Shared secret used only when mode = "api_key".
# api_key = "change-me"

# Authorized public keys used when mode = "signature".
[[server.auth.authorized_clients]]
pubkey = "020202020202020202020202020202020202020202020202020202020202020202"
role = "admin"

[[server.auth.authorized_clients]]
pubkey = "030303030303030303030303030303030303030303030303030303030303030303"
role = "write"

# CORS origins allowed when auth is enabled. Empty = any origin (not recommended for browsers).
allowed_origins = ["https://tracker.example.com"]

# Request signature timestamp tolerance in milliseconds (signature mode only).
signature_timestamp_tolerance_ms = 30000
```

## Signature Mode Details

In signature mode each request must carry:

| Header | Value |
|---|---|
| `X-Signature-Pubkey` | Hex-encoded 33-byte compressed secp256k1 public key (66 characters). |
| `X-Signature` | Hex-encoded 65-byte Schnorr signature (130 characters). |
| `X-Signature-Timestamp` | Unix timestamp in milliseconds. |
| `X-Signature-Nonce` | Random nonce (recommended; required for strong replay protection). |

### Canonical Message

The signed message is:

```text
<METHOD>\n<PATH>\n<QUERY>\n<TIMESTAMP>\n<NONCE>\n<BODY_HASH>
```

where:

- `<METHOD>` is the uppercase HTTP method (`GET`, `POST`, ...).
- `<PATH>` is the URI path including a leading slash (e.g. `/notes`).
- `<QUERY>` is the raw query string, or empty if none.
- `<TIMESTAMP>` is the value of `X-Signature-Timestamp`.
- `<NONCE>` is the value of `X-Signature-Nonce`.
- `<BODY_HASH>` is the lowercase hex SHA-256 of the raw request body (empty for `GET`).

### Signature Format

The 65-byte signature follows the chaincash-rs / Basis Schnorr format:

- 33 bytes: compressed point `a`
- 32 bytes: scalar `z`

Verification uses `basis_offchain::schnorr::schnorr_verify` and the challenge `e = H(a || message || pubkey)`.

### Replay Protection

The server keeps an in-memory cache of seen `(pubkey_lowercase, nonce)` pairs. If a client omits a nonce, the server falls back to `(pubkey_lowercase, timestamp)`, which allows at most one request per timestamp tick. Entries are pruned after twice the configured timestamp tolerance.

### Timestamp Tolerance

Signatures whose timestamp is older than `signature_timestamp_tolerance_ms` or more than that far in the future are rejected. The default is 30 seconds.

## Role-Based Access Control

Every authenticated request receives an effective role. Routes require the following roles:

| Route Pattern | Method | Required Role |
|---|---|---|
| `/` | `GET` | public (Read context inserted) |
| `GET` endpoints | `GET` | `Read` |
| `/notes` | `POST` | `Write` |
| `/redeem` | `POST` | `Write` |
| `/redeem/complete` | `POST` | `Write` |
| `/redemption/prepare` | `POST` | `Write` |
| `/redemption/build` | `POST` | `Write` |
| `/redemption/submit` | `POST` | `Write` |
| `/tracker/signature` | `POST` | `Write` |
| `/notes/state` | `POST` | `Read` |
| `/acceptance/check` | `POST` | `Read` |
| `/acceptance/policy` | `POST` | `Admin` |
| `/reserves/create` | `POST` | `Admin` |
| `/reserves/submit` | `POST` | `Admin` |
| unknown routes/verbs | any | `Admin` |

Role hierarchy: `Admin` > `Write` > `Read`. A higher role can access endpoints requiring a lower role.

### Role Assignment

- `anonymous` / `none` mode: every request gets `Admin`.
- `api_key` mode: every request gets `Admin`.
- `signature` mode: role is taken from the matching `authorized_clients` entry.

## Client Configuration

### `basis_cli`

The CLI reads auth settings from `~/.basis/cli.toml`:

```toml
server_url = "https://tracker.example.com:3048"
server_auth_mode = "signature"
server_auth_pubkey = "020202..."
server_auth_secret_key = "<64 hex chars>"
```

For API-key mode:

```toml
server_auth_mode = "api_key"
server_api_key = "change-me"
```

`TrackerAuth::from_config()` builds the active auth scheme, and `TrackerClient::apply_auth()` attaches the correct headers to every request.

### `basis_mcp`

The MCP server reads credentials from environment variables first, then falls back to `~/.basis/cli.toml`:

| Variable | Purpose |
|---|---|
| `BASIS_TRACKER_AUTH_MODE` | `none`, `api_key`, or `signature` |
| `BASIS_TRACKER_API_KEY` | Shared secret for API-key mode |
| `BASIS_TRACKER_AUTH_PUBKEY` | Hex public key for signature mode |
| `BASIS_TRACKER_AUTH_SECRET_KEY` | Hex secret key for signature mode |

If an environment variable is absent or empty, the corresponding `server_*` field from `CliConfig` is used. This lets operators inject secrets via env vars while still allowing local development with `cli.toml`.

### `basis_app` (TUI wallet)

The TUI wallet uses the same `~/.basis/cli.toml` auth settings as `basis_cli`. It builds `TrackerAuth::from_config(...)` and creates an authenticated `TrackerClient`. No additional UI configuration is required; existing `server_auth_mode`, `server_api_key`, `server_auth_pubkey`, and `server_auth_secret_key` values are honored automatically.

## CORS Considerations

When auth is enabled, the server restricts CORS origins if `allowed_origins` is non-empty. If `allowed_origins` is empty while auth is enabled, the server allows any origin but logs a warning, because browser-based clients should use an explicit allow-list.

## Security Checklist

- Never run `mode = "none"` or `api_key` over plaintext in production; configure TLS.
- Keep `signature_timestamp_tolerance_ms` small (default 30 s) to limit replay windows.
- Always provide a unique `X-Signature-Nonce` per request in signature mode.
- Store secret keys and API keys in environment variables or a secrets manager, never in committed config files.
- Use `Read` roles for monitoring-only clients, `Write` for wallet/agent operations, and `Admin` for reserve management.

## References

- Implementation: `crates/basis_server/src/auth_middleware.rs`, `crates/basis_server/src/authorization.rs`
- Config: `crates/basis_server/src/config.rs`, `config/basis.toml.example`
- Client: `crates/basis_cli/src/api.rs`, `crates/basis_cli/src/config.rs`
- MCP: `crates/basis_mcp/src/server.rs`
- TUI wallet: `crates/basis_app/src/app.rs`
- OpenAPI: `openapi.yaml`, `docs/OPENAPI.md`
