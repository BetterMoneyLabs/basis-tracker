//! Authentication middleware for the Basis Tracker server.
//!
//! Supports three modes:
//! - `None`: anonymous requests (backward-compatible local dev).
//! - `ApiKey`: shared secret via `Authorization: Bearer <key>` or `X-API-Key: <key>`.
//! - `Signature`: per-client secp256k1 Schnorr request signatures.
//!
//! In signature mode the client must provide:
//! - `X-Signature-Pubkey`: hex-encoded 33-byte compressed public key (66 chars).
//! - `X-Signature`: hex-encoded 65-byte Schnorr signature (130 chars).
//! - `X-Signature-Timestamp`: Unix timestamp in milliseconds.
//! - `X-Signature-Nonce`: unique nonce (recommended; required for strong replay protection).
//!
//! The signed message is:
//! ```text
//! <METHOD>\n<PATH>\n<QUERY>\n<TIMESTAMP>\n<NONCE>\n<BODY_HASH>
//! ```
//! where `BODY_HASH` is the lowercase hex SHA-256 of the raw request body.

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

use crate::config::{AuthConfig, AuthMode, ClientRole};

/// State shared by the auth middleware.
#[derive(Clone)]
pub struct AuthState {
    pub config: AuthConfig,
    /// Replay cache: (pubkey_hex, nonce) -> first seen time.
    pub replay_cache: Arc<Mutex<HashMap<(String, String), Instant>>>,
}

impl AuthState {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            replay_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// TTL for replay-cache entries. Using twice the timestamp tolerance gives a
    /// small safety margin while bounding memory growth.
    fn replay_ttl(&self) -> Duration {
        Duration::from_millis(self.config.signature_timestamp_tolerance_ms * 2)
    }
}

/// Information about an authenticated request, inserted into request extensions.
#[derive(Clone, Debug)]
pub struct AuthContext {
    /// Authenticated public key, if any.
    pub pubkey: Option<String>,
    /// Effective role of the caller.
    pub role: ClientRole,
    /// Active authentication mode.
    pub auth_mode: AuthMode,
}

/// Main entry point used by `axum::middleware::from_fn_with_state`.
pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    // The health check is always public so load balancers can probe the server.
    if is_health_check(&request) {
        let mut request = request;
        request.extensions_mut().insert(AuthContext {
            pubkey: None,
            role: ClientRole::Read,
            auth_mode: state.config.mode,
        });
        return next.run(request).await;
    }

    match state.config.mode {
        AuthMode::None => {
            let mut request = request;
            request.extensions_mut().insert(AuthContext {
                pubkey: None,
                role: ClientRole::Admin,
                auth_mode: AuthMode::None,
            });
            next.run(request).await
        }
        AuthMode::ApiKey => match verify_api_key(&state.config, &request) {
            Ok(ctx) => {
                let mut request = request;
                request.extensions_mut().insert(ctx);
                next.run(request).await
            }
            Err(e) => unauthorized(e),
        },
        AuthMode::Signature => match verify_signature(&state, request).await {
            Ok((ctx, request)) => {
                let mut request = request;
                request.extensions_mut().insert(ctx);
                next.run(request).await
            }
            Err(e) => unauthorized(e),
        },
    }
}

fn is_health_check(request: &Request) -> bool {
    request.uri().path() == "/" && request.method() == axum::http::Method::GET
}

fn unauthorized(message: impl AsRef<str>) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        format!(
            r#"{{"success":false,"data":null,"error":"{}"}}"#,
            message.as_ref().replace('"', "\\\"")
        ),
    )
        .into_response()
}

fn verify_api_key(config: &AuthConfig, request: &Request) -> Result<AuthContext, String> {
    let provided = extract_bearer_token(request)
        .or_else(|| extract_header(request, "X-API-Key"))
        .ok_or("Missing API key: provide Authorization: Bearer <key> or X-API-Key: <key>")?;

    let expected = config
        .api_key
        .as_ref()
        .filter(|k| !k.is_empty())
        .ok_or("Server is not configured with an API key")?;

    if subtle::constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(AuthContext {
            pubkey: None,
            role: ClientRole::Admin,
            auth_mode: AuthMode::ApiKey,
        })
    } else {
        Err("Invalid API key".to_string())
    }
}

fn extract_bearer_token(request: &Request) -> Option<String> {
    let value = request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(|s| s.trim().to_string())
}

fn extract_header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
}

async fn verify_signature(
    state: &AuthState,
    request: Request,
) -> Result<(AuthContext, Request), String> {
    let pubkey_hex = extract_header(&request, "X-Signature-Pubkey")
        .ok_or("Missing X-Signature-Pubkey header")?;
    let signature_hex =
        extract_header(&request, "X-Signature").ok_or("Missing X-Signature header")?;
    let timestamp_str = extract_header(&request, "X-Signature-Timestamp")
        .ok_or("Missing X-Signature-Timestamp header")?;
    let nonce = extract_header(&request, "X-Signature-Nonce").unwrap_or_default();

    // Validate pubkey format.
    if hex::decode(&pubkey_hex)
        .map(|b| b.len() != 33)
        .unwrap_or(true)
    {
        return Err("Invalid X-Signature-Pubkey: expected 66 hex characters".to_string());
    }

    // Locate the authorized client entry.
    let client = state
        .config
        .authorized_clients
        .iter()
        .find(|c| c.pubkey.eq_ignore_ascii_case(&pubkey_hex))
        .ok_or("Public key is not authorized")?;

    // Timestamp tolerance check.
    let timestamp_ms: u64 = timestamp_str
        .parse()
        .map_err(|_| "Invalid X-Signature-Timestamp: expected integer milliseconds")?;
    let now_ms = current_timestamp_ms();
    let tolerance = state.config.signature_timestamp_tolerance_ms;
    if now_ms.saturating_sub(timestamp_ms) > tolerance {
        return Err("Request signature timestamp is too old".to_string());
    }
    if timestamp_ms > now_ms.saturating_add(tolerance) {
        return Err("Request signature timestamp is in the future".to_string());
    }

    // Replay protection: reject reused (pubkey, nonce) pairs within the TTL.
    // If the client does not send a nonce we fall back to (pubkey, timestamp),
    // which allows at most one request per pubkey per timestamp tick.
    let cache_key = if nonce.is_empty() {
        (pubkey_hex.to_lowercase(), timestamp_str)
    } else {
        (pubkey_hex.to_lowercase(), nonce.clone())
    };
    {
        let mut cache = state.replay_cache.lock().await;
        prune_cache(&mut cache, state.replay_ttl());
        if cache.contains_key(&cache_key) {
            return Err("Replayed signature nonce/timestamp".to_string());
        }
        cache.insert(cache_key, Instant::now());
    }

    // Build canonical message and verify signature.
    let (parts, body) = request.into_parts();
    let body_bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|e| format!("Failed to read request body: {}", e))?;
    let body_hash = hex::encode(Sha256::digest(&body_bytes));

    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or("").to_string();
    let method = parts.method.as_str().to_uppercase();

    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, path, query, timestamp_ms, nonce, body_hash
    );

    let signature_bytes =
        hex::decode(&signature_hex).map_err(|_| "Invalid X-Signature: expected hex string")?;
    if signature_bytes.len() != 65 {
        return Err(format!(
            "Invalid signature length: expected 130 hex characters, got {}",
            signature_hex.len()
        ));
    }
    let mut signature_array = [0u8; 65];
    signature_array.copy_from_slice(&signature_bytes);

    let pubkey_bytes = hex::decode(&pubkey_hex).map_err(|_| "Invalid public key hex")?;
    let mut pubkey_array = [0u8; 33];
    pubkey_array.copy_from_slice(&pubkey_bytes);

    basis_offchain::schnorr::schnorr_verify(&signature_array, canonical.as_bytes(), &pubkey_array)
        .map_err(|_| "Invalid request signature")?;

    let request = Request::from_parts(parts, Body::from(body_bytes));
    Ok((
        AuthContext {
            pubkey: Some(pubkey_hex),
            role: client.role,
            auth_mode: AuthMode::Signature,
        },
        request,
    ))
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn prune_cache(cache: &mut HashMap<(String, String), Instant>, ttl: Duration) {
    let now = Instant::now();
    cache.retain(|_, instant| now.duration_since(*instant) < ttl);
}

// `subtle` is not a direct dependency, so implement a small constant-time comparison.
mod subtle {
    pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut acc = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            acc |= x ^ y;
        }
        acc == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn constant_time_eq_works() {
        assert!(subtle::constant_time_eq(b"same", b"same"));
        assert!(!subtle::constant_time_eq(b"same", b"different"));
        assert!(!subtle::constant_time_eq(b"a", b"aa"));
    }

    #[test]
    fn extract_bearer_token_parses_header() {
        let request = Request::builder()
            .header("Authorization", "Bearer secret-key-123")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&request).unwrap(), "secret-key-123");
    }

    #[test]
    fn api_key_missing_header_fails() {
        let request = Request::builder().body(Body::empty()).unwrap();
        let config = AuthConfig {
            mode: AuthMode::ApiKey,
            api_key: Some("secret".to_string()),
            ..Default::default()
        };
        assert!(verify_api_key(&config, &request).is_err());
    }
}
