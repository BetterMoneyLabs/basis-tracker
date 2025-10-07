// HTTP API integration tests for basis_server endpoints

#[cfg(test)]
mod http_api_tests {
    use axum::{
        http::StatusCode,
    };
    use crate::{
        api::{get_notes_by_issuer, get_notes_by_recipient},
        models::{ApiResponse, SerializableIouNote},
        AppState,
    };
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // Test helper to create a mock app state
    fn create_mock_app_state() -> AppState {
        let (tx, _rx) = mpsc::channel(100);
        let event_store = Arc::new(crate::store::EventStore::new());
        
        // Create a default NodeConfig for the scanner
        let config = basis_store::ergo_scanner::NodeConfig::default();
        let ergo_scanner = Arc::new(tokio::sync::Mutex::new(
            basis_store::ergo_scanner::ServerState::new(config, "http://localhost:9053".to_string())
        ));
        let reserve_tracker = Arc::new(tokio::sync::Mutex::new(
            basis_store::ReserveTracker::new()
        ));
        
        AppState {
            tx,
            event_store,
            ergo_scanner,
            reserve_tracker,
        }
    }

    #[tokio::test]
    async fn test_get_notes_by_issuer_empty_list() {
        // Test that when no notes exist for an issuer, we get an empty list (not 404)
        let state = create_mock_app_state();
        
        // Create a valid public key (33 bytes hex encoded)
        let valid_pubkey = "010101010101010101010101010101010101010101010101010101010101010101";
        
        let response = get_notes_by_issuer(
            axum::extract::State(state),
            axum::extract::Path(valid_pubkey.to_string()),
        ).await;
        
        // Should return 200 OK with empty array
        assert_eq!(response.0, StatusCode::OK);
        
        let response_body = response.1;
        assert!(response_body.success);
        assert!(response_body.data.is_some());
        
        let notes = response_body.data.unwrap();
        assert!(notes.is_empty(), "Expected empty notes list, got: {:?}", notes);
    }

    #[tokio::test]
    async fn test_get_notes_by_issuer_invalid_hex() {
        // Test that invalid hex encoding returns 400 Bad Request
        let state = create_mock_app_state();
        
        let invalid_hex = "not_a_valid_hex_string";
        
        let response = get_notes_by_issuer(
            axum::extract::State(state),
            axum::extract::Path(invalid_hex.to_string()),
        ).await;
        
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        
        let response_body = response.1;
        assert!(!response_body.success);
        assert!(response_body.error.is_some());
        assert!(response_body.data.is_none());
    }

    #[tokio::test]
    async fn test_get_notes_by_issuer_wrong_length() {
        // Test that wrong byte length returns 400 Bad Request
        let state = create_mock_app_state();
        
        // 32 bytes instead of 33
        let wrong_length_pubkey = "0101010101010101010101010101010101010101010101010101010101010101";
        
        let response = get_notes_by_issuer(
            axum::extract::State(state),
            axum::extract::Path(wrong_length_pubkey.to_string()),
        ).await;
        
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        
        let response_body = response.1;
        assert!(!response_body.success);
        assert!(response_body.error.is_some());
        assert!(response_body.data.is_none());
    }

    #[tokio::test]
    async fn test_get_notes_by_recipient_empty_list() {
        // Test that when no notes exist for a recipient, we get an empty list (not 404)
        let state = create_mock_app_state();
        
        // Create a valid public key (33 bytes hex encoded)
        let valid_pubkey = "020202020202020202020202020202020202020202020202020202020202020202";
        
        let response = get_notes_by_recipient(
            axum::extract::State(state),
            axum::extract::Path(valid_pubkey.to_string()),
        ).await;
        
        // Should return 200 OK with empty array
        assert_eq!(response.0, StatusCode::OK);
        
        let response_body = response.1;
        assert!(response_body.success);
        assert!(response_body.data.is_some());
        
        let notes = response_body.data.unwrap();
        assert!(notes.is_empty(), "Expected empty notes list, got: {:?}", notes);
    }
}