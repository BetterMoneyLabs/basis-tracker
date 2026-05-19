//! Integration tests for acceptance predicate API endpoint

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

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
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
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
                    "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2".to_string()
                ],
                max_debt: None,
            },
        ],
    };
    
    let predicate = basis_server::acceptance::builder::build_predicate_tree(config)
        .unwrap()
        .map(|p| std::sync::Arc::from(p) as std::sync::Arc<dyn basis_server::acceptance::NotePredicate>);
    
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
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
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
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
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
        .body(Body::from(json!({
            "issuer_pubkey": "not-hex!!!",
            "total_debt": 100
        }).to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    
    // Test wrong length
    let request = Request::builder()
        .method(Method::POST)
        .uri("/acceptance/check")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "issuer_pubkey": "deadbeef",
            "total_debt": 100
        }).to_string()))
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
                    "02a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2".to_string()
                ],
                max_debt: Some(500),
            },
        ],
    };
    
    let predicate = basis_server::acceptance::builder::build_predicate_tree(config)
        .unwrap()
        .map(|p| std::sync::Arc::from(p) as std::sync::Arc<dyn basis_server::acceptance::NotePredicate>);
    
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["acceptable"], false);
}

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
    }).unwrap();
    
    let app_state = AppState {
        tx,
        event_store,
        ergo_scanner: Arc::new(Mutex::new(scanner)),
        reserve_tracker: Arc::new(Mutex::new(basis_store::ReserveTracker::new())),
        config,
        shared_tracker_state: Arc::new(tokio::sync::Mutex::new(tracker_box_updater::SharedTrackerState::new())),
        tracker_storage: basis_store::persistence::TrackerStorage::open("test_tracker").unwrap(),
        acceptance_predicate,
    };
    
    axum::Router::new()
        .route("/acceptance/check", axum::routing::post(api::check_acceptance))
        .with_state(app_state)
}
