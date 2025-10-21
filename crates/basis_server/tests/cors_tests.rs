// Comprehensive CORS tests for basis_server

#[cfg(test)]
mod cors_tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        routing::{get, post},
        Router,
    };
    use basis_server::{api::*, reserve_api::*, store::EventStore, AppState, TrackerCommand};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tower::ServiceExt;
    use tower_http::cors::{Any, CorsLayer};

    // Test helper to create a mock app state with CORS enabled
    async fn create_mock_app_with_cors() -> Router {
        let (tx, mut rx) = mpsc::channel(100);
        let event_store = Arc::new(EventStore::new().await.unwrap());

        // Create a default NodeConfig for the scanner
        let config = basis_store::ergo_scanner::NodeConfig::default();
        let ergo_scanner = Arc::new(tokio::sync::Mutex::new(
            basis_store::ergo_scanner::ServerState::new(
                config,
                "http://localhost:9053".to_string(),
            ),
        ));
        let reserve_tracker = Arc::new(tokio::sync::Mutex::new(basis_store::ReserveTracker::new()));

        // Spawn tracker thread for tests
        tokio::task::spawn_blocking(move || {
            use basis_store::{RedemptionManager, TrackerStateManager};

            tracing::debug!("Test tracker thread started");
            let tracker = TrackerStateManager::new();
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

        let app_state = AppState {
            tx,
            event_store,
            ergo_scanner,
            reserve_tracker,
        };

        // Build the app with CORS enabled (same as main server)
        Router::new()
            // Root route
            .route("/", get(root))
            // Static routes
            .route("/events", get(get_events))
            .route("/events/paginated", get(get_events_paginated))
            .route("/notes", post(create_note))
            .route("/redeem", post(initiate_redemption))
            .route("/redeem/complete", post(complete_redemption))
            .route("/proof", get(get_proof))
            // Most specific parameterized routes first
            .route(
                "/notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}",
                get(get_note_by_issuer_and_recipient),
            )
            // Parameterized routes
            .route("/notes/issuer/{pubkey}", get(get_notes_by_issuer))
            .route("/notes/recipient/{pubkey}", get(get_notes_by_recipient))
            .route("/reserves/issuer/{pubkey}", get(get_reserves_by_issuer))
            .route("/key-status/{pubkey}", get(get_key_status))
            .with_state(app_state)
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
    }

    #[tokio::test]
    async fn test_cors_preflight_allowed() {
        // Test that preflight OPTIONS requests are properly handled
        let app = create_mock_app_with_cors().await;

        // Test preflight for root endpoint
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/")
                    .header("Origin", "http://localhost:3000")
                    .header("Access-Control-Request-Method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );
        assert!(response
            .headers()
            .contains_key("access-control-allow-methods"));
        assert!(response
            .headers()
            .contains_key("access-control-allow-headers"));
    }

    #[tokio::test]
    async fn test_cors_headers_on_all_routes() {
        // Test that CORS headers are present on all API routes
        let app = create_mock_app_with_cors().await;

        let test_routes = [
            ("/", Method::GET),
            ("/events", Method::GET),
            ("/notes", Method::POST),
            ("/redeem", Method::POST),
            ("/proof", Method::GET),
        ];

        for (route, method) in test_routes.iter() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(*route)
                        .header("Origin", "https://example.com")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert!(
                response
                    .headers()
                    .contains_key("access-control-allow-origin"),
                "CORS header missing on route: {}",
                route
            );
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .unwrap(),
                "*",
                "Wrong CORS origin on route: {}",
                route
            );
        }
    }

    #[tokio::test]
    async fn test_cors_all_methods_allowed() {
        // Test that all HTTP methods are allowed via CORS
        let app = create_mock_app_with_cors().await;

        let methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"];

        for method in methods.iter() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("OPTIONS")
                        .uri("/")
                        .header("Origin", "http://test-origin.com")
                        .header("Access-Control-Request-Method", *method)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let allowed_methods = response
                .headers()
                .get("access-control-allow-methods")
                .unwrap()
                .to_str()
                .unwrap();
            // With wildcard (*), all methods are allowed
            assert_eq!(
                allowed_methods, "*",
                "Expected wildcard for allowed methods, got: {}",
                allowed_methods
            );
        }
    }

    #[tokio::test]
    async fn test_cors_all_headers_allowed() {
        // Test that all headers are allowed via CORS
        let app = create_mock_app_with_cors().await;

        let test_headers = [
            "Content-Type",
            "Authorization",
            "X-Requested-With",
            "Accept",
            "Origin",
            "Access-Control-Request-Method",
            "Access-Control-Request-Headers",
        ];

        for header in test_headers.iter() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("OPTIONS")
                        .uri("/")
                        .header("Origin", "http://test-origin.com")
                        .header("Access-Control-Request-Method", "GET")
                        .header("Access-Control-Request-Headers", header.to_string())
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let allowed_headers = response
                .headers()
                .get("access-control-allow-headers")
                .unwrap()
                .to_str()
                .unwrap();
            // With wildcard (*), all headers are allowed
            assert_eq!(
                allowed_headers, "*",
                "Expected wildcard for allowed headers, got: {}",
                allowed_headers
            );
        }
    }

    #[tokio::test]
    async fn test_cors_parameterized_routes() {
        // Test that CORS works on parameterized routes
        let app = create_mock_app_with_cors().await;

        let parameterized_routes = [
            "/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101",
            "/notes/recipient/020202020202020202020202020202020202020202020202020202020202020202",
            "/reserves/issuer/010101010101010101010101010101010101010101010101010101010101010101",
            "/key-status/010101010101010101010101010101010101010101010101010101010101010101",
        ];

        for route in parameterized_routes.iter() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(*route)
                        .header("Origin", "https://different-domain.com")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert!(
                response
                    .headers()
                    .contains_key("access-control-allow-origin"),
                "CORS header missing on parameterized route: {}",
                route
            );
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .unwrap(),
                "*"
            );
        }
    }

    #[tokio::test]
    async fn test_cors_multiple_origins_allowed() {
        // Test that any origin is allowed (wildcard *)
        let app = create_mock_app_with_cors().await;

        let test_origins = [
            "http://localhost:3000",
            "https://example.com",
            "http://127.0.0.1:8080",
            "https://app.basis-tracker.com",
            "http://test.localhost",
        ];

        for origin in test_origins.iter() {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/")
                        .header("Origin", *origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .unwrap(),
                "*",
                "Wrong CORS origin for origin: {}",
                origin
            );
        }
    }

    #[tokio::test]
    async fn test_cors_with_actual_api_calls() {
        // Test CORS with actual API calls that return data
        let app = create_mock_app_with_cors().await;

        // Test GET /events endpoint
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/events")
                    .header("Origin", "https://frontend-app.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );

        // Test GET /notes/issuer/{pubkey} endpoint
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101")
                    .header("Origin", "https://different-domain.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn test_cors_preflight_with_complex_scenarios() {
        // Test complex preflight scenarios
        let app = create_mock_app_with_cors().await;

        // Test preflight with multiple request headers
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/notes")
                    .header("Origin", "https://complex-app.com")
                    .header("Access-Control-Request-Method", "POST")
                    .header(
                        "Access-Control-Request-Headers",
                        "Content-Type, Authorization".to_string(),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );

        let allowed_headers = response
            .headers()
            .get("access-control-allow-headers")
            .unwrap()
            .to_str()
            .unwrap();
        // With wildcard (*), all headers are allowed
        assert_eq!(
            allowed_headers, "*",
            "Expected wildcard for allowed headers, got: {}",
            allowed_headers
        );
    }

    #[tokio::test]
    async fn test_cors_no_origin_header_still_works() {
        // Test that requests without Origin header still work (backwards compatibility)
        let app = create_mock_app_with_cors().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should still work without Origin header
        assert_eq!(response.status(), StatusCode::OK);
        // CORS headers might not be present when no Origin is provided
        // This is expected behavior
    }

    #[tokio::test]
    async fn test_cors_error_responses_still_have_headers() {
        // Test that even error responses have CORS headers
        let app = create_mock_app_with_cors().await;

        // Test with invalid route (404)
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/nonexistent-route")
                    .header("Origin", "https://test-app.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should still have CORS headers even on 404
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );

        // Test with invalid pubkey format (400)
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/notes/issuer/invalid-hex")
                    .header("Origin", "https://test-app.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "*"
        );
    }
}
