//! Integration tests for acceptance predicate API endpoints
//!
//! This module tests:
//! - check_acceptance: Evaluates if a note would be accepted
//! - upload_policy: Uploads a signed acceptance policy for a recipient
//! - get_policy_by_recipient: Retrieves a stored policy for a recipient

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

// ============================================================================
// check_acceptance endpoint tests
// ============================================================================

#[tokio::test]
async fn test_check_acceptance_without_policy() {
    let app = create_test_app(None).await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "total_debt": 1000000000
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Without policy, should use default (reject)
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["acceptable"], false);
}

#[tokio::test]
async fn test_check_acceptance_whitelist() {
    let config = basis_server::acceptance::config::AcceptanceConfig {
        default: basis_server::acceptance::config::DefaultPolicy::Reject,
        root: Some("trusted".to_string()),
        predicates: vec![
            basis_server::acceptance::config::PredicateConfig::Whitelist {
                name: "trusted".to_string(),
                holders: vec![
                    "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2"
                        .to_string(),
                ],
                max_debt: None,
            },
        ],
    };

    let predicate = basis_server::acceptance::builder::build_predicate_tree(config)
        .unwrap()
        .map(|p| {
            std::sync::Arc::from(p) as std::sync::Arc<dyn basis_server::acceptance::NotePredicate>
        });

    let app = create_test_app(predicate).await;

    // Test whitelisted pubkey
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "total_debt": 1000000000
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    if status != StatusCode::OK {
        panic!("Expected OK but got {:?}: {:?}", status, json);
    }

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["acceptable"], true);

    // Test non-whitelisted pubkey
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "03ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "total_debt": 1000000000
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    if status != StatusCode::OK {
        panic!("Expected OK but got {:?}: {:?}", status, json);
    }

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["acceptable"], false);
}

#[tokio::test]
async fn test_check_acceptance_invalid_pubkey() {
    let app = create_test_app(None).await;

    // Test invalid hex
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "issuer_pubkey": "not-hex!!!",
                "total_debt": 100
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test wrong length
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "issuer_pubkey": "deadbeef",
                "total_debt": 100
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_check_acceptance_with_max_debt() {
    let config = basis_server::acceptance::config::AcceptanceConfig {
        default: basis_server::acceptance::config::DefaultPolicy::Reject,
        root: Some("trusted".to_string()),
        predicates: vec![
            basis_server::acceptance::config::PredicateConfig::Whitelist {
                name: "trusted".to_string(),
                holders: vec![
                    "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2"
                        .to_string(),
                ],
                max_debt: Some(500),
            },
        ],
    };

    let predicate = basis_server::acceptance::builder::build_predicate_tree(config)
        .unwrap()
        .map(|p| {
            std::sync::Arc::from(p) as std::sync::Arc<dyn basis_server::acceptance::NotePredicate>
        });

    let app = create_test_app(predicate).await;

    // Under limit
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "total_debt": 400
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["acceptable"], true);

    // Over limit
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "total_debt": 600
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["acceptable"], false);
}

// ============================================================================
// upload_policy endpoint tests
// ============================================================================

#[tokio::test]
async fn test_upload_policy_invalid_hex_pubkey() {
    let app = create_test_app_with_policy_routes(None).await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "recipient_pubkey": "not-hex!!!",
            "policy_json": "{}",
            "signature": "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("hex-encoded"));
}

#[tokio::test]
async fn test_upload_policy_wrong_length_pubkey() {
    let app = create_test_app_with_policy_routes(None).await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "recipient_pubkey": "deadbeef",
            "policy_json": "{}",
            "signature": "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("33 bytes"));
}

#[tokio::test]
async fn test_upload_policy_invalid_signature_hex() {
    let app = create_test_app_with_policy_routes(None).await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "recipient_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "policy_json": "{}",
            "signature": "not-hex!!!"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("hex-encoded"));
}

#[tokio::test]
async fn test_upload_policy_wrong_signature_length() {
    let app = create_test_app_with_policy_routes(None).await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "recipient_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "policy_json": "{}",
            "signature": "aabbccdd"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("65 bytes"));
}

#[tokio::test]
async fn test_upload_policy_invalid_json() {
    let app = create_test_app_with_policy_routes(None).await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "recipient_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "policy_json": "not valid json",
            "signature": "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Invalid policy JSON"));
}

#[tokio::test]
async fn test_upload_policy_invalid_signature() {
    let app = create_test_app_with_policy_routes(None).await;

    // Valid policy JSON but all-zero signature (will fail verification)
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "recipient_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "policy_json": "{\"predicates\":[]}",
            "signature": "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Invalid signature"));
}

#[tokio::test]
async fn test_upload_and_retrieve_policy_roundtrip() {
    use secp256k1::{Secp256k1, SecretKey};

    let app = create_test_app_with_policy_routes(None).await;

    // Generate a valid secp256k1 keypair for signing
    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let recipient_pubkey = hex::encode(public_key.serialize());

    // Create a policy JSON with the correct format (requires 'type' field)
    let policy_json = r#"{"default":"reject","root":"require_full_collateral","predicates":[{"name":"require_full_collateral","type":"collateralization","min_ratio":1.0}]}"#;

    // Sign the policy JSON using the core Schnorr implementation
    let policy_bytes = policy_json.as_bytes();
    let signature = sign_policy_with_key(&policy_bytes, &secret_key);
    let signature_hex = hex::encode(&signature);

    // Upload the policy
    let upload_request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "recipient_pubkey": recipient_pubkey,
                "policy_json": policy_json,
                "signature": signature_hex
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(upload_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]["policy_hash"].is_string());
    assert!(json["data"]["uploaded_at"].is_number());

    // Retrieve the policy
    let get_request = Request::builder()
        .method(Method::GET)
        .uri(format!("/acceptance/policy/{}", recipient_pubkey))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["recipient_pubkey"], recipient_pubkey);
    assert_eq!(json["data"]["policy_json"], policy_json);
    assert_eq!(json["data"]["signature"], signature_hex);
    assert!(json["data"]["uploaded_at"].is_number());
}

#[tokio::test]
async fn test_get_policy_not_found() {
    let app = create_test_app_with_policy_routes(None).await;

    let request = Request::builder()
        .method(Method::GET)
        .uri(
            "/acceptance/policy/02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
        )
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("No policy found"));
}

#[tokio::test]
async fn test_get_policy_invalid_pubkey() {
    let app = create_test_app_with_policy_routes(None).await;

    // Invalid hex
    let request = Request::builder()
        .method(Method::GET)
        .uri("/acceptance/policy/not-hex")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Wrong length
    let request = Request::builder()
        .method(Method::GET)
        .uri("/acceptance/policy/deadbeef")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Per-recipient policy integration with check_acceptance
// ============================================================================

#[tokio::test]
async fn test_check_acceptance_uses_per_recipient_policy() {
    use secp256k1::{Secp256k1, SecretKey};

    let app = create_test_app_with_all_routes(None).await;

    // Generate recipient keypair
    let secp = Secp256k1::new();
    let recipient_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let recipient_pubkey = secp256k1::PublicKey::from_secret_key(&secp, &recipient_secret);
    let recipient_pubkey_hex = hex::encode(recipient_pubkey.serialize());

    // Create a whitelist policy that only accepts a specific issuer
    let issuer_pubkey = "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
    let policy_json = format!(
        r#"{{"default":"reject","root":"trusted","predicates":[{{"name":"trusted","type":"whitelist","holders":["{}"],"max_debt":null}}]}}"#,
        issuer_pubkey
    );

    // Sign the policy
    let signature = sign_policy_with_key(policy_json.as_bytes(), &recipient_secret);
    let signature_hex = hex::encode(&signature);

    // Upload the policy
    let upload_request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/policy")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "recipient_pubkey": recipient_pubkey_hex,
                "policy_json": policy_json,
                "signature": signature_hex
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(upload_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Test check_acceptance with the whitelisted issuer and recipient
    let check_request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "issuer_pubkey": issuer_pubkey,
                "total_debt": 1000000000,
                "recipient_pubkey": recipient_pubkey_hex
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(check_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["acceptable"], true);

    // Test with a non-whitelisted issuer - should be rejected by per-recipient policy
    let check_request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "03ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "total_debt": 1000000000,
            "recipient_pubkey": recipient_pubkey_hex
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(check_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["acceptable"], false);
    assert!(json["data"]["reason"]
        .as_str()
        .unwrap()
        .contains("per-recipient policy"));
}

#[tokio::test]
async fn test_check_acceptance_fallback_to_global_policy() {
    // Create a global policy that rejects everything
    let config = basis_server::acceptance::config::AcceptanceConfig {
        default: basis_server::acceptance::config::DefaultPolicy::Reject,
        root: None,
        predicates: vec![],
    };

    let predicate = basis_server::acceptance::builder::build_predicate_tree(config)
        .unwrap()
        .map(|p| {
            std::sync::Arc::from(p) as std::sync::Arc<dyn basis_server::acceptance::NotePredicate>
        });

    let app = create_test_app_with_all_routes(predicate).await;

    // Check acceptance without a per-recipient policy - should use global policy
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
            "total_debt": 1000000000,
            "recipient_pubkey": "02b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["acceptable"], false);
    let reason = json["data"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("global policy") || reason.contains("No acceptance policy configured"),
        "Expected reason to contain 'global policy' or 'No acceptance policy configured', got: {}",
        reason
    );
}

// ============================================================================
// Helper functions
// ============================================================================

/// Generate a unique temporary directory path for test storage.
///
/// Fjall/Loro storage locks its directory, so concurrent tests (including tests
/// in different binaries) must use distinct paths.
fn unique_test_storage_path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// Global lock to serialize Fjall storage initialization across concurrent tests.
///
/// Fjall's keyspace creation can race when multiple databases are opened
/// concurrently in the same process, leading to intermittent "No such file or
/// directory" errors. Holding this lock while creating test storage avoids that.
static STORAGE_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Helper to create a test app with optional acceptance predicate
async fn create_test_app(
    acceptance_predicate: Option<std::sync::Arc<dyn basis_server::acceptance::NotePredicate>>,
) -> axum::Router {
    use basis_server::*;
    use basis_store::ergo_scanner::NodeConfig;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let (tx, _rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);
    let event_store = Arc::new(store::EventStore::new_in_memory());

    let config = Arc::new(config::AppConfig {
        server: config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3048,
            database_url: Some("sqlite::memory:".to_string()),
        },
        ergo: config::ErgoConfig {
            node: NodeConfig {
                node_url: "http://example.com".to_string(),
                ..Default::default()
            },
            basis_reserve_contract_p2s: "test".to_string(),
            tracker_nft_id: Some("test".to_string()),
            tracker_public_key: None,
            tracker_secret_key: None,
        },
        transaction: config::TransactionConfig {
            fee: 1000000,
            change_address: None,
        },
        acceptance: acceptance::config::AcceptanceConfig::empty(),
    });

    let scanner = basis_store::ergo_scanner::ServerState::new(NodeConfig {
        node_url: "http://example.com".to_string(),
        ..Default::default()
    })
    .unwrap();

    let tracker_storage_path = unique_test_storage_path("basis_test_tracker_storage_acceptance");
    std::fs::create_dir_all(&tracker_storage_path)
        .expect("Failed to create tracker storage directory");
    let policy_storage_path = unique_test_storage_path("basis_test_policy_storage_acceptance");
    std::fs::create_dir_all(&policy_storage_path)
        .expect("Failed to create policy storage directory");

    let (tracker_storage, policy_storage) = {
        let _guard = STORAGE_INIT_LOCK.lock().unwrap();
        let tracker_storage =
            basis_store::persistence::TrackerStorage::open(&tracker_storage_path).unwrap();
        let policy_storage =
            basis_store::persistence::AcceptancePolicyStorage::open(&policy_storage_path).unwrap();
        (tracker_storage, policy_storage)
    };

    let app_state = AppState {
        tx,
        event_store,
        ergo_scanner: Arc::new(Mutex::new(scanner)),
        reserve_tracker: Arc::new(Mutex::new(basis_store::ReserveTracker::new())),
        config,
        shared_tracker_state: Arc::new(tokio::sync::Mutex::new(
            tracker_box_updater::SharedTrackerState::new(),
        )),
        tracker_storage,
        acceptance_predicate,
        policy_storage,
    };

    axum::Router::new()
        .route(
            "/acceptance/check",
            axum::routing::post(api::check_acceptance),
        )
        .with_state(app_state)
}

/// Helper to create a test app with policy upload/retrieval routes
async fn create_test_app_with_policy_routes(
    acceptance_predicate: Option<std::sync::Arc<dyn basis_server::acceptance::NotePredicate>>,
) -> axum::Router {
    use basis_server::*;
    use basis_store::ergo_scanner::NodeConfig;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let (tx, _rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);
    let event_store = Arc::new(store::EventStore::new_in_memory());

    let config = Arc::new(config::AppConfig {
        server: config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3048,
            database_url: Some("sqlite::memory:".to_string()),
        },
        ergo: config::ErgoConfig {
            node: NodeConfig {
                node_url: "http://example.com".to_string(),
                ..Default::default()
            },
            basis_reserve_contract_p2s: "test".to_string(),
            tracker_nft_id: Some("test".to_string()),
            tracker_public_key: None,
            tracker_secret_key: None,
        },
        transaction: config::TransactionConfig {
            fee: 1000000,
            change_address: None,
        },
        acceptance: acceptance::config::AcceptanceConfig::empty(),
    });

    let scanner = basis_store::ergo_scanner::ServerState::new(NodeConfig {
        node_url: "http://example.com".to_string(),
        ..Default::default()
    })
    .unwrap();

    let tracker_storage_path =
        unique_test_storage_path("basis_test_tracker_storage_acceptance_policy");
    std::fs::create_dir_all(&tracker_storage_path)
        .expect("Failed to create tracker storage directory");
    let policy_storage_path =
        unique_test_storage_path("basis_test_policy_storage_acceptance_policy");
    std::fs::create_dir_all(&policy_storage_path)
        .expect("Failed to create policy storage directory");

    let (tracker_storage, policy_storage) = {
        let _guard = STORAGE_INIT_LOCK.lock().unwrap();
        let tracker_storage =
            basis_store::persistence::TrackerStorage::open(&tracker_storage_path).unwrap();
        let policy_storage =
            basis_store::persistence::AcceptancePolicyStorage::open(&policy_storage_path).unwrap();
        (tracker_storage, policy_storage)
    };

    let app_state = AppState {
        tx,
        event_store,
        ergo_scanner: Arc::new(Mutex::new(scanner)),
        reserve_tracker: Arc::new(Mutex::new(basis_store::ReserveTracker::new())),
        config,
        shared_tracker_state: Arc::new(tokio::sync::Mutex::new(
            tracker_box_updater::SharedTrackerState::new(),
        )),
        tracker_storage,
        acceptance_predicate,
        policy_storage,
    };

    axum::Router::new()
        .route(
            "/acceptance/policy",
            axum::routing::post(api::upload_policy),
        )
        .route(
            "/acceptance/policy/{pubkey}",
            axum::routing::get(api::get_policy_by_recipient),
        )
        .with_state(app_state)
}

/// Helper to create a test app with all acceptance routes (check + policy)
async fn create_test_app_with_all_routes(
    acceptance_predicate: Option<std::sync::Arc<dyn basis_server::acceptance::NotePredicate>>,
) -> axum::Router {
    use basis_server::*;
    use basis_store::ergo_scanner::NodeConfig;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let (tx, _rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);
    let event_store = Arc::new(store::EventStore::new_in_memory());

    let config = Arc::new(config::AppConfig {
        server: config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3048,
            database_url: Some("sqlite::memory:".to_string()),
        },
        ergo: config::ErgoConfig {
            node: NodeConfig {
                node_url: "http://example.com".to_string(),
                ..Default::default()
            },
            basis_reserve_contract_p2s: "test".to_string(),
            tracker_nft_id: Some("test".to_string()),
            tracker_public_key: None,
            tracker_secret_key: None,
        },
        transaction: config::TransactionConfig {
            fee: 1000000,
            change_address: None,
        },
        acceptance: acceptance::config::AcceptanceConfig::empty(),
    });

    let scanner = basis_store::ergo_scanner::ServerState::new(NodeConfig {
        node_url: "http://example.com".to_string(),
        ..Default::default()
    })
    .unwrap();

    let tracker_storage_path =
        unique_test_storage_path("basis_test_tracker_storage_acceptance_all");
    std::fs::create_dir_all(&tracker_storage_path)
        .expect("Failed to create tracker storage directory");
    let policy_storage_path = unique_test_storage_path("basis_test_policy_storage_acceptance_all");
    std::fs::create_dir_all(&policy_storage_path)
        .expect("Failed to create policy storage directory");

    let (tracker_storage, policy_storage) = {
        let _guard = STORAGE_INIT_LOCK.lock().unwrap();
        let tracker_storage =
            basis_store::persistence::TrackerStorage::open(&tracker_storage_path).unwrap();
        let policy_storage =
            basis_store::persistence::AcceptancePolicyStorage::open(&policy_storage_path).unwrap();
        (tracker_storage, policy_storage)
    };

    let app_state = AppState {
        tx,
        event_store,
        ergo_scanner: Arc::new(Mutex::new(scanner)),
        reserve_tracker: Arc::new(Mutex::new(basis_store::ReserveTracker::new())),
        config,
        shared_tracker_state: Arc::new(tokio::sync::Mutex::new(
            tracker_box_updater::SharedTrackerState::new(),
        )),
        tracker_storage,
        acceptance_predicate,
        policy_storage,
    };

    axum::Router::new()
        .route(
            "/acceptance/check",
            axum::routing::post(api::check_acceptance),
        )
        .route(
            "/acceptance/policy",
            axum::routing::post(api::upload_policy),
        )
        .route(
            "/acceptance/policy/{pubkey}",
            axum::routing::get(api::get_policy_by_recipient),
        )
        .with_state(app_state)
}

/// Sign a policy JSON using a secp256k1 secret key with Schnorr signatures
fn sign_policy_with_key(policy_bytes: &[u8], secret_key: &secp256k1::SecretKey) -> [u8; 65] {
    use basis_core::impls::SchnorrVerifier;
    use basis_core::traits::SignatureVerifier;

    let verifier = SchnorrVerifier;
    let secret_key_bytes = secret_key.secret_bytes();

    // Generate a dummy public key for signing (we just need a valid signature)
    let secp = secp256k1::Secp256k1::new();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, secret_key);
    let pubkey_bytes = public_key.serialize();

    let signature = verifier
        .sign_message(policy_bytes, &secret_key_bytes, &pubkey_bytes)
        .expect("Failed to sign policy");

    // Verify the signature locally before returning
    verifier
        .verify_signature(&signature, policy_bytes, &pubkey_bytes)
        .expect("Local signature verification failed");

    signature
}
