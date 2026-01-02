// HTTP API integration tests for basis_server endpoints

#[cfg(test)]
mod http_api_tests {
    use axum::http::StatusCode;
    use basis_server::{
        api::{get_notes_by_issuer, get_notes_by_recipient},
        config,
        store::EventStore,
        AppState, TrackerCommand,
    };
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tower::util::ServiceExt;

    // Test helper to create a mock app state
    async fn create_mock_app_state() -> AppState {
        let (tx, mut rx) = mpsc::channel(100);
        let event_store = Arc::new(EventStore::new().await.unwrap());

        // Create a default NodeConfig for the scanner
        let config = basis_store::ergo_scanner::NodeConfig {
            node_url: "http://localhost:9053".to_string(),
            ..Default::default()
        };
        let ergo_scanner = Arc::new(tokio::sync::Mutex::new(
            basis_store::ergo_scanner::ServerState::new(config).unwrap(),
        ));
        let reserve_tracker = Arc::new(tokio::sync::Mutex::new(basis_store::ReserveTracker::new()));

        // Spawn tracker thread for tests
        tokio::task::spawn_blocking(move || {
            use basis_store::{RedemptionManager, TrackerStateManager};

            tracing::debug!("Test tracker thread started");
            let tracker = TrackerStateManager::new_with_temp_storage();
            let mut redemption_manager = RedemptionManager::new(tracker);

            while let Some(cmd) = rx.blocking_recv() {
                tracing::debug!("Test tracker thread received command: {:?}", cmd);
                match cmd {
                    TrackerCommand::AddNote {
                        issuer_pubkey,
                        note,
                        response_tx,
                    } => {
                        let result = redemption_manager.tracker.add_note(&issuer_pubkey, &note);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetNotesByIssuer {
                        issuer_pubkey,
                        response_tx,
                    } => {
                        let result = redemption_manager.tracker.get_issuer_notes(&issuer_pubkey);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetNotesByRecipient {
                        recipient_pubkey,
                        response_tx,
                    } => {
                        let result = redemption_manager
                            .tracker
                            .get_recipient_notes(&recipient_pubkey);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetNoteByIssuerAndRecipient {
                        issuer_pubkey,
                        recipient_pubkey,
                        response_tx,
                    } => {
                        let result = redemption_manager
                            .tracker
                            .lookup_note(&issuer_pubkey, &recipient_pubkey)
                            .map(Some);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::InitiateRedemption {
                        request,
                        response_tx,
                    } => {
                        let result = redemption_manager.initiate_redemption(&request);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::CompleteRedemption {
                        issuer_pubkey,
                        recipient_pubkey,
                        redeemed_amount,
                        response_tx,
                    } => {
                        let result = redemption_manager.complete_redemption(
                            &issuer_pubkey,
                            &recipient_pubkey,
                            redeemed_amount,
                        );
                        let _ = response_tx.send(result);
                    }
                }
            }
        });

        // Create a minimal config for testing
        let test_config = std::sync::Arc::new(config::AppConfig {
            server: config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3048,
                database_url: Some("sqlite::memory:".to_string()),
            },
            ergo: config::ErgoConfig {
                node: basis_store::ergo_scanner::NodeConfig {
                    node_url: "http://localhost:9053".to_string(),
                    ..Default::default()
                },
                basis_reserve_contract_p2s: "test".to_string(),
                tracker_nft_id: Some("69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b".to_string()),
                tracker_public_key: Some("9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string()),
            },
            transaction: config::TransactionConfig {
                fee: 1000000,
            },
        });

        AppState {
            tx,
            event_store,
            ergo_scanner,
            reserve_tracker,
            config: test_config,
        }
    }

    #[tokio::test]
    async fn test_get_notes_by_issuer_empty_list() {
        // Test that when no notes exist for an issuer, we get an empty list (not 404)
        let state = create_mock_app_state().await;

        // Create a valid public key (33 bytes hex encoded)
        let valid_pubkey = "010101010101010101010101010101010101010101010101010101010101010101";

        let response = get_notes_by_issuer(
            axum::extract::State(state),
            axum::extract::Path(valid_pubkey.to_string()),
        )
        .await;

        // Should return 200 OK with empty array
        assert_eq!(response.0, StatusCode::OK);

        let response_body = &response.1;
        assert!(response_body.success);
        assert!(response_body.data.is_some());

        let notes = response_body.data.as_ref().unwrap();
        assert!(
            notes.is_empty(),
            "Expected empty notes list, got: {:?}",
            notes
        );
    }

    #[tokio::test]
    async fn test_get_notes_by_issuer_invalid_hex() {
        // Test that invalid hex encoding returns 400 Bad Request
        let state = create_mock_app_state().await;

        let invalid_hex = "not_a_valid_hex_string";

        let response = get_notes_by_issuer(
            axum::extract::State(state),
            axum::extract::Path(invalid_hex.to_string()),
        )
        .await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);

        let response_body = &response.1;
        assert!(!response_body.success);
        assert!(response_body.error.is_some());
        assert!(response_body.data.is_none());
    }

    #[tokio::test]
    async fn test_get_notes_by_issuer_wrong_length() {
        // Test that wrong byte length returns 400 Bad Request
        let state = create_mock_app_state().await;

        // 32 bytes instead of 33
        let wrong_length_pubkey =
            "0101010101010101010101010101010101010101010101010101010101010101";

        let response = get_notes_by_issuer(
            axum::extract::State(state),
            axum::extract::Path(wrong_length_pubkey.to_string()),
        )
        .await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);

        let response_body = &response.1;
        assert!(!response_body.success);
        assert!(response_body.error.is_some());
        assert!(response_body.data.is_none());
    }

    #[tokio::test]
    async fn test_get_notes_by_recipient_empty_list() {
        // Test that when no notes exist for a recipient, we get an empty list (not 404)
        let state = create_mock_app_state().await;

        // Create a valid public key (33 bytes hex encoded)
        let valid_pubkey = "020202020202020202020202020202020202020202020202020202020202020202";

        let response = get_notes_by_recipient(
            axum::extract::State(state),
            axum::extract::Path(valid_pubkey.to_string()),
        )
        .await;

        // Should return 200 OK with empty array
        assert_eq!(response.0, StatusCode::OK);

        let response_body = &response.1;
        assert!(response_body.success);
        assert!(response_body.data.is_some());

        let notes = response_body.data.as_ref().unwrap();
        assert!(
            notes.is_empty(),
            "Expected empty notes list, got: {:?}",
            notes
        );
    }

    #[tokio::test]
    async fn test_cors_headers_present() {
        // Test that CORS headers are properly set on responses
        use axum::{routing::get, Router};
        use tower_http::cors::{Any, CorsLayer};

        // Create a test app with CORS enabled (same as main server)
        let app = Router::new()
            .route("/", get(|| async { "Hello, Basis Tracker API!" }))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );

        // Test with a preflight OPTIONS request
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("OPTIONS")
                    .uri("/")
                    .header("Origin", "http://example.com")
                    .header("Access-Control-Request-Method", "GET")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check that CORS headers are present
        assert!(response
            .headers()
            .contains_key("access-control-allow-origin"));
        assert!(response
            .headers()
            .contains_key("access-control-allow-methods"));
        assert!(response
            .headers()
            .contains_key("access-control-allow-headers"));

        // Test with a regular GET request
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/")
                    .header("Origin", "http://example.com")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check that CORS headers are present on regular responses too
        assert!(response
            .headers()
            .contains_key("access-control-allow-origin"));
    }
}
