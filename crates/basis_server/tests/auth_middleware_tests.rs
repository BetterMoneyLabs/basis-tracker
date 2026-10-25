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
    let timestamp = current_timestamp_ms();
    let request1 = sign_request_full(
        Request::builder().uri("/notes").method("GET"),
        Vec::new(),
        &pubkey_hex,
        &secret_key,
        timestamp,
        "test-nonce",
    );

    // First request succeeds.
    let response = app.clone().oneshot(request1).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same (timestamp, nonce) pair replayed is rejected.
    let request2 = sign_request_full(
        Request::builder().uri("/notes").method("GET"),
        Vec::new(),
        &pubkey_hex,
        &secret_key,
        timestamp,
        "test-nonce",
    );
    let response = app.oneshot(request2).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
    let timestamp = current_timestamp_ms();
    sign_request_full(
        builder,
        body_bytes,
        pubkey_hex,
        secret_key,
        timestamp,
        "test-nonce",
    )
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn sign_request_full(
    builder: axum::http::request::Builder,
    body_bytes: Vec<u8>,
    pubkey_hex: &str,
    secret_key: &secp256k1::SecretKey,
    timestamp: u64,
    nonce: &str,
) -> Request<Body> {
    let body_hash = hex::encode(sha2::Sha256::digest(&body_bytes));

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

fn write_mode_config(pubkey_hex: &str) -> AuthConfig {
    let mut config = AuthConfig::default();
    config.mode = AuthMode::Signature;
    config.authorized_clients.push(AuthorizedClient {
        pubkey: pubkey_hex.to_string(),
        role: ClientRole::Write,
    });
    config
}

#[tokio::test]
async fn signature_mode_rejects_missing_nonce() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let app = test_app(write_mode_config(&pubkey_hex));

    let mut request = sign_request(
        Request::builder().uri("/notes").method("GET"),
        None,
        &pubkey_hex,
        &secret_key,
    );
    request.headers_mut().remove("X-Signature-Nonce");

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bad_signature_does_not_burn_nonce() {
    // A request with an invalid signature must not insert the nonce into the
    // replay cache: the legitimate retry with the same nonce must still pass.
    let (pubkey_hex, secret_key) = generate_keypair();
    let app = test_app(write_mode_config(&pubkey_hex));

    let timestamp = current_timestamp_ms();
    let nonce = "burn-test-nonce";

    // Forged request: valid headers but a corrupted signature.
    let mut forged = sign_request_full(
        Request::builder().uri("/notes").method("GET"),
        Vec::new(),
        &pubkey_hex,
        &secret_key,
        timestamp,
        nonce,
    );
    {
        let headers = forged.headers_mut();
        let sig = headers.get("X-Signature").unwrap().to_str().unwrap();
        let mut bytes = hex::decode(sig).unwrap();
        bytes[50] ^= 0x01;
        headers.insert("X-Signature", hex::encode(bytes).parse().unwrap());
    }
    let response = app.clone().oneshot(forged).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Legitimate request with the same nonce succeeds.
    let genuine = sign_request_full(
        Request::builder().uri("/notes").method("GET"),
        Vec::new(),
        &pubkey_hex,
        &secret_key,
        timestamp,
        nonce,
    );
    let response = app.oneshot(genuine).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn signature_mode_rejects_oversized_body() {
    let (pubkey_hex, secret_key) = generate_keypair();
    let app = test_app(write_mode_config(&pubkey_hex));

    // One byte over the limit the auth middleware shares with the router's DefaultBodyLimit.
    let body = vec![b'a'; basis_server::auth_middleware::MAX_BODY_BYTES + 1];
    let request = sign_request_full(
        Request::builder().uri("/notes").method("POST"),
        body,
        &pubkey_hex,
        &secret_key,
        current_timestamp_ms(),
        "big-body-nonce",
    );

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn api_key_without_admin_key_keeps_admin_role() {
    // Backward compatibility: when admin_api_key is not configured, the shared
    // api_key keeps its full-Admin meaning.
    let mut config = AuthConfig::default();
    config.mode = AuthMode::ApiKey;
    config.api_key = Some("super-secret".to_string());

    let app = test_app(config);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/acceptance/policy")
                .method("POST")
                .header("Authorization", "Bearer super-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"role=Admin");
}

#[tokio::test]
async fn api_key_with_admin_key_set_grants_only_write() {
    // With a separate admin_api_key configured, the shared api_key is Write-only:
    // readable routes pass, operator-only (Admin) routes are rejected with 403.
    let mut config = AuthConfig::default();
    config.mode = AuthMode::ApiKey;
    config.api_key = Some("shared-key".to_string());
    config.admin_api_key = Some("operator-key".to_string());

    let app = test_app(config);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/notes")
                .header("Authorization", "Bearer shared-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"role=Write");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/acceptance/policy")
                .method("POST")
                .header("Authorization", "Bearer shared-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_api_key_grants_admin_role() {
    let mut config = AuthConfig::default();
    config.mode = AuthMode::ApiKey;
    config.api_key = Some("shared-key".to_string());
    config.admin_api_key = Some("operator-key".to_string());

    let app = test_app(config);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/acceptance/policy")
                .method("POST")
                .header("Authorization", "Bearer operator-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"role=Admin");
}

#[tokio::test]
async fn signature_mode_rejects_zero_padded_timestamp_replay() {
    // Regression test: the signature covers the PARSED timestamp, so replaying
    // a captured request with a zero-padded X-Signature-Timestamp string
    // ("01745000000000") yields the same canonical message and a still-valid
    // signature. Replay protection must key the cache on the parsed value so
    // the variant is rejected instead of replayed.
    let (pubkey_hex, secret_key) = generate_keypair();
    let app = test_app(write_mode_config(&pubkey_hex));

    let timestamp = current_timestamp_ms();
    let nonce = "padded-replay-nonce";
    let request1 = sign_request_full(
        Request::builder().uri("/notes").method("GET"),
        Vec::new(),
        &pubkey_hex,
        &secret_key,
        timestamp,
        nonce,
    );

    // First request succeeds.
    let response = app.clone().oneshot(request1).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Replay with the EXACT same headers and signature, except the timestamp
    // string is zero-padded. The signature still verifies (it covers the
    // parsed value), so only the replay cache can stop it.
    let mut request2 = sign_request_full(
        Request::builder().uri("/notes").method("GET"),
        Vec::new(),
        &pubkey_hex,
        &secret_key,
        timestamp,
        nonce,
    );
    request2.headers_mut().insert(
        "X-Signature-Timestamp",
        format!("0{}", timestamp).parse().unwrap(),
    );

    let response = app.oneshot(request2).await.unwrap();
    assert!(
        response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN,
        "zero-padded replay must be rejected, got {}",
        response.status()
    );
}
