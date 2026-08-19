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
