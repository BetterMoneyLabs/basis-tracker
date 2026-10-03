//! Integration tests for the authentication and authorization middleware.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Extension, Router,
};
use basis_server::{
    auth_middleware::{auth_middleware, AuthContext, AuthState},
    authorization::authorization_middleware,
    config::{AuthConfig, AuthMode, AuthorizedClient, ClientRole},
};
use sha2::Digest;
use std::sync::Arc;
use tower::ServiceExt;

async fn protected_handler(Extension(ctx): Extension<AuthContext>) -> String {
    format!("role={:?}", ctx.role)
}

fn test_app(auth_config: AuthConfig) -> Router {
    let auth_state = Arc::new(AuthState::new(auth_config));
    Router::new()
        .route("/", get(protected_handler))
        .route("/notes", get(protected_handler).post(protected_handler))
        .route("/acceptance/policy", post(protected_handler))
        .layer(axum::middleware::from_fn(authorization_middleware))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
}

#[tokio::test]
async fn anonymous_mode_allows_all_requests() {
    let app = test_app(AuthConfig::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn api_key_mode_accepts_valid_key() {
    let mut config = AuthConfig::default();
    config.mode = AuthMode::ApiKey;
    config.api_key = Some("super-secret".to_string());

    let app = test_app(config);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/notes")
                .header("Authorization", "Bearer super-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn api_key_mode_rejects_missing_key() {
    let mut config = AuthConfig::default();
    config.mode = AuthMode::ApiKey;
    config.api_key = Some("super-secret".to_string());

    let app = test_app(config);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_key_mode_rejects_wrong_key() {
    let mut config = AuthConfig::default();
    config.mode = AuthMode::ApiKey;
    config.api_key = Some("super-secret".to_string());

    let app = test_app(config);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/notes")
                .header("Authorization", "Bearer wrong-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signature_mode_accepts_valid_signature() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.clone(),
        role: ClientRole::Write,
    });

    let app = test_app(config);
    let request = sign_request(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn read_role_cannot_post_admin_route() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.clone(),
        role: ClientRole::Read,
    });

    let app = test_app(config);
    let request = sign_request(
        Request::builder().uri("/acceptance/policy").method("POST"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn signature_mode_rejects_unauthorized_pubkey() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    // No authorized clients.

    let app = test_app(config);
    let request = sign_request(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signature_mode_rejects_bad_signature() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.clone(),
        role: ClientRole::Write,
    });

    let app = test_app(config);
    let mut request = sign_request(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    // Corrupt the signature.
    {
        let headers = request.headers_mut();
        let sig = headers.get("X-Signature").unwrap().to_str().unwrap();
        let mut bytes = hex::decode(sig).unwrap();
        bytes[50] ^= 0x01;
        headers.insert("X-Signature", hex::encode(bytes).parse().unwrap());
    }

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signature_mode_rejects_replayed_nonce() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.clone(),
        role: ClientRole::Write,
    });

    let app = test_app(config);
    let request1 = sign_request(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );

    // First request succeeds.
    let response = app.clone().oneshot(request1).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same nonce replayed is rejected.
    let request2 = sign_request(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    let response = app.oneshot(request2).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// SECURITY (PR #14 S4): a nonce must only be consumed by a request whose signature actually
/// verified. The replay cache used to be written before verification, so anybody could burn a
/// client's nonce with one forged request and lock the legitimate client out.
#[tokio::test]
async fn forged_request_does_not_consume_a_nonce() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.clone(),
        role: ClientRole::Write,
    });

    let app = test_app(config);

    // A request with a valid timestamp/nonce but a garbage signature.
    let forged = Request::builder()
        .uri("/notes")
        .method("GET")
        .header("X-Signature-Pubkey", &pubkey_hex)
        .header("X-Signature", hex::encode([0u8; 65]))
        .header("X-Signature-Timestamp", current_timestamp_ms().to_string())
        .header("X-Signature-Nonce", "shared-nonce")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(forged).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // The genuine client must still be able to use that nonce.
    let genuine = sign_request_with_nonce(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
        "shared-nonce",
    );
    let response = app.oneshot(genuine).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a forged request must not burn the client's nonce"
    );
}

/// SECURITY (PR #14 S1): the replay cache is keyed on the PARSED timestamp, so a captured request
/// cannot be replayed by adding leading zeros to `X-Signature-Timestamp` (`0<ts>`). Previously the
/// cache used the raw header string while the signature covered the parsed value, so the padded
/// variant produced a fresh cache key and the same signature verified again.
#[tokio::test]
async fn zero_padded_timestamp_cannot_bypass_replay_protection() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.clone(),
        role: ClientRole::Write,
    });

    let app = test_app(config);

    // No nonce, so the cache falls back to the timestamp.
    let first = sign_request_without_nonce(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    let timestamp_str = first
        .headers()
        .get("X-Signature-Timestamp")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let response = app.clone().oneshot(first).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same signature, same parsed timestamp, different header string.
    let mut replay = sign_request_without_nonce(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    replay.headers_mut().insert(
        "X-Signature-Timestamp",
        format!("0{timestamp_str}").parse().unwrap(),
    );
    let response = app.oneshot(replay).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a zero-padded timestamp must not create a fresh replay-cache key"
    );
}

/// SECURITY (PR #14 S1): the documented auth mode name is `api_key`. It used to deserialize as
/// `apikey`, and the server then fell back to defaults -- i.e. anonymous Admin -- so an operator
/// who followed the documentation silently ran an open server.
#[test]
fn auth_mode_deserializes_the_documented_api_key_spelling() {
    for spelling in ["api_key", "apikey", "API_KEY"] {
        let parsed: AuthConfig = toml::from_str(&format!("mode = \"{spelling}\""))
            .unwrap_or_else(|e| panic!("auth mode {spelling:?} must deserialize: {e}"));
        assert_eq!(parsed.mode, AuthMode::ApiKey, "spelling {spelling:?}");
    }

    // The other two modes keep working.
    let none: AuthConfig = toml::from_str("mode = \"none\"").unwrap();
    assert_eq!(none.mode, AuthMode::None);
    let sig: AuthConfig = toml::from_str("mode = \"signature\"").unwrap();
    assert_eq!(sig.mode, AuthMode::Signature);

    // An unknown mode must be an error, not a silent downgrade to `none`.
    assert!(
        toml::from_str::<AuthConfig>("mode = \"ap1key\"").is_err(),
        "an unrecognised auth mode must fail to parse rather than default to none"
    );
}

fn generate_keypair() -> (String, secp256k1::SecretKey) {
    let secp = secp256k1::Secp256k1::new();
    let secret = secp256k1::SecretKey::new(&mut secp256k1::rand::thread_rng());
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret).serialize();
    (hex::encode(pubkey), secret)
}

fn sign_request(
    builder: axum::http::request::Builder,
    body: Option<serde_json::Value>,
    pubkey_hex: &str,
    secret_key: &secp256k1::SecretKey,
) -> Request<Body> {
    let body_bytes = body
        .as_ref()
        .map(|v| serde_json::to_vec(v).unwrap())
        .unwrap_or_default();
    let body_hash = hex::encode(sha2::Sha256::digest(&body_bytes));
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let nonce = "test-nonce".to_string();

    let method = builder.method_ref().cloned().unwrap_or_default();
    let uri = builder.uri_ref().cloned().unwrap_or_default();
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str().to_uppercase(),
        uri.path(),
        uri.query().unwrap_or(""),
        timestamp,
        nonce,
        body_hash
    );

    let pubkey_bytes = hex::decode(pubkey_hex).unwrap();
    let mut pubkey_array = [0u8; 33];
    pubkey_array.copy_from_slice(&pubkey_bytes);
    let signature =
        basis_offchain::schnorr::schnorr_sign(canonical.as_bytes(), secret_key, &pubkey_array)
            .unwrap();

    builder
        .header("X-Signature-Pubkey", pubkey_hex)
        .header("X-Signature", hex::encode(signature))
        .header("X-Signature-Timestamp", timestamp.to_string())
        .header("X-Signature-Nonce", nonce)
        .body(Body::from(body_bytes))
        .unwrap()
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Core signing helper: builds a request signed over the canonical string, with a caller-chosen
/// nonce (`None` omits the `X-Signature-Nonce` header entirely, which makes the middleware fall back
/// to keying its replay cache on the timestamp).
fn signed_request_with_nonce(
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
    pubkey_hex: &str,
    secret_key: &secp256k1::SecretKey,
    nonce: Option<&str>,
) -> Request<Body> {
    let body_bytes = body
        .as_ref()
        .map(|v| serde_json::to_vec(v).unwrap())
        .unwrap_or_default();
    let body_hash = hex::encode(sha2::Sha256::digest(&body_bytes));
    let timestamp = current_timestamp_ms();
    let nonce_value = nonce.unwrap_or("");

    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_uppercase(),
        path,
        "",
        timestamp,
        nonce_value,
        body_hash
    );

    let mut pubkey_array = [0u8; 33];
    pubkey_array.copy_from_slice(&hex::decode(pubkey_hex).unwrap());
    let signature =
        basis_offchain::schnorr::schnorr_sign(canonical.as_bytes(), secret_key, &pubkey_array)
            .unwrap();

    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("X-Signature-Pubkey", pubkey_hex)
        .header("X-Signature", hex::encode(signature))
        .header("X-Signature-Timestamp", timestamp.to_string());
    if let Some(n) = nonce {
        builder = builder.header("X-Signature-Nonce", n);
    }
    builder.body(Body::from(body_bytes)).unwrap()
}

fn sign_request_with_nonce(
    builder: axum::http::request::Builder,
    body: Option<serde_json::Value>,
    pubkey_hex: &str,
    secret_key: &secp256k1::SecretKey,
    nonce: &str,
) -> Request<Body> {
    let method = builder.method_ref().cloned().unwrap_or_default();
    let uri = builder.uri_ref().cloned().unwrap_or_default();
    signed_request_with_nonce(
        method.as_str(),
        uri.path(),
        body,
        pubkey_hex,
        secret_key,
        Some(nonce),
    )
}

fn sign_request_without_nonce(
    builder: axum::http::request::Builder,
    body: Option<serde_json::Value>,
    pubkey_hex: &str,
    secret_key: &secp256k1::SecretKey,
) -> Request<Body> {
    let method = builder.method_ref().cloned().unwrap_or_default();
    let uri = builder.uri_ref().cloned().unwrap_or_default();
    signed_request_with_nonce(
        method.as_str(),
        uri.path(),
        body,
        pubkey_hex,
        secret_key,
        None,
    )
}
