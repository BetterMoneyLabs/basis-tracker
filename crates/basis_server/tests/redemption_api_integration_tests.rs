//! Integration tests for redemption API endpoints
//!
//! This module tests the redemption-related HTTP endpoints:
//! - POST /redeem: Initiate redemption
//! - POST /redeem/complete: Complete redemption
//! - GET /proof/redemption: Get redemption proof
//! - POST /redemption/prepare: Prepare redemption data
//! - POST /tracker/signature: Request tracker signature
//!
//! Tests use the direct handler call pattern (Pattern A) with mock AppState,
//! reusing the create_mock_app_state helper from http_api_integration_tests.rs.
//!
//! NOTE: These tests use persistent storage (Fjall/LSM-tree) which creates
//! directories on disk. Due to storage locking, tests should be run with
//! --test-threads=1 to avoid conflicts between parallel test executions.
//! Example: cargo test -p basis_server --test redemption_api_integration_tests -- --test-threads=1

#[cfg(test)]
mod redemption_api_tests {
    use axum::http::StatusCode;
    use basis_server::{
        api::{
            complete_redemption, get_redemption_proof, initiate_redemption, prepare_redemption,
            request_tracker_signature,
        },
        models::{
            CompleteRedemptionRequest, RedeemRequest, RedemptionPreparationRequest,
            TrackerSignatureRequest,
        },
        AppState, TrackerCommand,
    };
    use basis_store::schnorr::generate_keypair;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // ============================================================================
    // Test helper: create mock app state with tracker thread handling redemption commands
    // ============================================================================

    async fn create_mock_app_state() -> AppState {
        let (tx, mut rx) = mpsc::channel(100);
        let event_store = Arc::new(basis_server::store::EventStore::new().await.unwrap());

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
                        new_already_redeemed,
                        response_tx,
                    } => {
                        let result = redemption_manager.complete_redemption(
                            &issuer_pubkey,
                            &recipient_pubkey,
                            redeemed_amount,
                            new_already_redeemed,
                        );
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetNotes { response_tx } => {
                        let result = Ok(Vec::new());
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GenerateProof {
                        issuer_pubkey,
                        recipient_pubkey,
                        response_tx,
                    } => {
                        let mock_proof = basis_store::NoteProof {
                            note: basis_store::IouNote::new([0u8; 33], 0, 0, 0, [0u8; 65]),
                            avl_proof: vec![1, 2, 3, 4],
                            operations: vec![],
                        };
                        let result = Ok(mock_proof);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetTrackerLookupProof {
                        issuer_pubkey: _,
                        recipient_pubkey: _,
                        response_tx,
                    } => {
                        let mock_proof = basis_store::TrackerLookupProof {
                            key: vec![0u8; 64],
                            value: vec![0u8; 8],
                            proof: vec![1, 2, 3, 4],
                        };
                        let _ = response_tx.send(Ok(mock_proof));
                    }
                    TrackerCommand::GetReserveLookupProof {
                        issuer_pubkey: _,
                        recipient_pubkey: _,
                        response_tx,
                    } => {
                        let mock_proof = basis_store::ReserveLookupProof {
                            key: vec![0u8; 64],
                            value: vec![0u8; 8],
                            proof: Some(vec![1, 2, 3, 4]),
                        };
                        let _ = response_tx.send(Ok(mock_proof));
                    }
                    TrackerCommand::GetReserveInsertProof {
                        issuer_pubkey: _,
                        recipient_pubkey: _,
                        timestamp: _,
                        new_already_redeemed: _,
                        response_tx,
                    } => {
                        let _ = response_tx.send(Ok((vec![1, 2, 3, 4], vec![5, 6, 7, 8])));
                    }
                    TrackerCommand::GetNotesByRecipientWithIssuer {
                        recipient_pubkey: _,
                        response_tx,
                    } => {
                        let _ = response_tx.send(Ok(Vec::new()));
                    }
                    TrackerCommand::GetConfirmation {
                        issuer_pubkey,
                        recipient_pubkey,
                        response_tx,
                    } => {
                        let result = Ok(redemption_manager
                            .tracker
                            .get_confirmation(&issuer_pubkey, &recipient_pubkey));
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetAllConfirmations { response_tx } => {
                        let _ = response_tx.send(redemption_manager.tracker.all_confirmations());
                    }
                    TrackerCommand::MarkNotesPending {
                        digest,
                        tx_id,
                        submitted_height,
                        response_tx,
                    } => {
                        let result = redemption_manager.tracker.mark_notes_pending(
                            digest,
                            &tx_id,
                            submitted_height,
                        );
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::ConfirmPendingNotes {
                        box_id,
                        height,
                        response_tx,
                    } => {
                        let result = redemption_manager
                            .tracker
                            .confirm_pending_notes(&box_id, height);
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::RevertPendingNotes { response_tx } => {
                        let result = redemption_manager.tracker.revert_pending_notes();
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::GetReserveStateDigest { response_tx } => {
                        let digest = redemption_manager.tracker.reserve_state_digest();
                        let _ = response_tx.send(digest);
                    }
                    TrackerCommand::ReconcileWithConfirmedDigest {
                        digest,
                        box_id,
                        height,
                        response_tx,
                    } => {
                        let result = redemption_manager
                            .tracker
                            .reconcile_with_confirmed_digest(&digest, &box_id, height);
                        let _ = response_tx.send(result);
                    }
                }
            }
        });

        // Create a minimal config for testing
        let test_config = std::sync::Arc::new(basis_server::config::AppConfig {
            server: basis_server::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3048,
                database_url: Some("sqlite::memory:".to_string()),
            },
            ergo: basis_server::config::ErgoConfig {
                node: basis_store::ergo_scanner::NodeConfig {
                    node_url: "http://localhost:9053".to_string(),
                    ..Default::default()
                },
                basis_reserve_contract_p2s: "test".to_string(),
                tracker_nft_id: Some(
                    "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b".to_string(),
                ),
                tracker_public_key: Some(
                    "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                ),
                tracker_secret_key: None,
            },
            transaction: basis_server::config::TransactionConfig {
                fee: 1000000,
                change_address: None,
            },
            acceptance: basis_server::acceptance::config::AcceptanceConfig::empty(),
        });

        let temp_dir = std::env::temp_dir().join(format!(
            "basis_test_tracker_storage_redemption_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
        let tracker_storage = basis_store::persistence::TrackerStorage::open(&temp_dir)
            .expect("Failed to create tracker storage");

        AppState {
            tx,
            event_store,
            ergo_scanner,
            reserve_tracker,
            config: test_config,
            shared_tracker_state: std::sync::Arc::new(tokio::sync::Mutex::new(
                basis_server::tracker_box_updater::SharedTrackerState::new(),
            )),
            tracker_storage,
            acceptance_predicate: None,
            policy_storage: basis_store::persistence::AcceptancePolicyStorage::open(
                temp_dir.join("policies"),
            )
            .expect("Failed to create policy storage"),
        }
    }

    /// Helper to add a note to the tracker for testing
    async fn add_test_note(
        state: &AppState,
        issuer_pubkey: &str,
        recipient_pubkey: &str,
        amount: u64,
        timestamp: u64,
    ) {
        let (secret, _) = generate_keypair();
        let recipient_bytes = hex::decode(recipient_pubkey).unwrap();
        let recipient_arr: [u8; 33] = recipient_bytes.try_into().unwrap();

        let note = basis_store::IouNote::create_and_sign(recipient_arr, amount, timestamp, &secret)
            .unwrap();

        let issuer_bytes = hex::decode(issuer_pubkey).unwrap();
        let issuer_arr: [u8; 33] = issuer_bytes.try_into().unwrap();

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let cmd = TrackerCommand::AddNote {
            issuer_pubkey: issuer_arr,
            note,
            response_tx,
        };

        state.tx.send(cmd).await.unwrap();
        let _ = response_rx.await.unwrap();
    }

    // ============================================================================
    // POST /redeem tests
    // ============================================================================

    #[tokio::test]
    async fn test_redeem_invalid_hex_pubkey() {
        // Test that invalid hex encoding for recipient pubkey returns 400
        let state = create_mock_app_state().await;

        let request = RedeemRequest {
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "not-valid-hex!!!".to_string(),
            amount: 1000,
            timestamp: 1234567890,
            reserve_box_id: "".to_string(),
            recipient_address: "".to_string(),
            issuer_signature: "01".repeat(65),
            emergency: false,
        };

        let response =
            initiate_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
        assert!(body.data.is_none());
    }

    #[tokio::test]
    async fn test_redeem_invalid_pubkey_length() {
        // Test that wrong-length public key returns 400
        let state = create_mock_app_state().await;

        // 32 bytes instead of 33
        let wrong_length = "0101010101010101010101010101010101010101010101010101010101010101";

        let request = RedeemRequest {
            issuer_pubkey: wrong_length.to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            amount: 1000,
            timestamp: 1234567890,
            reserve_box_id: "".to_string(),
            recipient_address: "".to_string(),
            issuer_signature: "01".repeat(65),
            emergency: false,
        };

        let response =
            initiate_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        // The handler will try to find a reserve and fail, or the tracker thread will fail
        // Either way, it should not succeed
        let body = &response.1;
        assert!(
            !body.success || body.data.is_none(),
            "Expected failure or no data, got: {:?}",
            body
        );
    }

    #[tokio::test]
    async fn test_redeem_note_not_found() {
        // Test redemption for a note that doesn't exist in the tracker
        let state = create_mock_app_state().await;

        let request = RedeemRequest {
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            amount: 1000,
            timestamp: 1234567890,
            reserve_box_id: "".to_string(),
            recipient_address: "".to_string(),
            issuer_signature: "01".repeat(65),
            emergency: false,
        };

        let response =
            initiate_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        // Should fail because no reserve exists for this issuer
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_redeem_emergency_flag_structure() {
        // Test that emergency redemption flag is accepted in request structure
        let state = create_mock_app_state().await;

        let request = RedeemRequest {
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            amount: 1000,
            timestamp: 1234567890,
            reserve_box_id: "".to_string(),
            recipient_address: "".to_string(),
            issuer_signature: "01".repeat(65),
            emergency: true, // Emergency flag set
        };

        // This will fail at the reserve lookup stage (no reserve in DB),
        // but the request structure with emergency=true should be accepted
        let response =
            initiate_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        // Should fail at reserve lookup, not at request parsing
        let body = &response.1;
        if !body.success {
            let default_msg = "unknown".to_string();
            let error_msg = body.error.as_ref().unwrap_or(&default_msg);
            assert!(
                error_msg.contains("reserve")
                    || error_msg.contains("Reserve")
                    || error_msg.contains("No matching reserve"),
                "Expected reserve-related error, got: {}",
                error_msg
            );
        }
    }

    // ============================================================================
    // POST /redeem/complete tests
    // ============================================================================

    #[tokio::test]
    async fn test_complete_redemption_invalid_hex() {
        // Test that invalid hex encoding returns 400
        let state = create_mock_app_state().await;

        let request = CompleteRedemptionRequest {
            redemption_id: "test-redemption-1".to_string(),
            issuer_pubkey: "not-hex!!!".to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            redeemed_amount: 1000,
            new_already_redeemed: None,
        };

        let response =
            complete_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_complete_redemption_wrong_length_pubkey() {
        // Test that wrong-length pubkey returns 400
        let state = create_mock_app_state().await;

        // 32 bytes instead of 33
        let wrong_length = "0101010101010101010101010101010101010101010101010101010101010101";

        let request = CompleteRedemptionRequest {
            redemption_id: "test-redemption-1".to_string(),
            issuer_pubkey: wrong_length.to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            redeemed_amount: 1000,
            new_already_redeemed: None,
        };

        let response =
            complete_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_complete_redemption_note_not_found() {
        // Test completing redemption for a non-existent note
        let state = create_mock_app_state().await;

        let request = CompleteRedemptionRequest {
            redemption_id: "test-redemption-1".to_string(),
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            redeemed_amount: 1000,
            new_already_redeemed: None,
        };

        let response =
            complete_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        // Should fail because note doesn't exist
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    // ============================================================================
    // GET /proof/redemption tests
    // ============================================================================

    #[tokio::test]
    async fn test_get_redemption_proof_invalid_hex() {
        // Test that invalid hex pubkey returns 400
        let state = create_mock_app_state().await;

        let mut params = std::collections::HashMap::new();
        params.insert("issuer_pubkey".to_string(), "not-hex!!!".to_string());
        params.insert(
            "recipient_pubkey".to_string(),
            "020202020202020202020202020202020202020202020202020202020202020202".to_string(),
        );

        let response =
            get_redemption_proof(axum::extract::State(state), axum::extract::Query(params)).await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_get_redemption_proof_wrong_length() {
        // Test that wrong-length pubkey returns 400
        let state = create_mock_app_state().await;

        let mut params = std::collections::HashMap::new();
        // 32 bytes instead of 33
        params.insert(
            "issuer_pubkey".to_string(),
            "0101010101010101010101010101010101010101010101010101010101010101".to_string(),
        );
        params.insert(
            "recipient_pubkey".to_string(),
            "020202020202020202020202020202020202020202020202020202020202020202".to_string(),
        );

        let response =
            get_redemption_proof(axum::extract::State(state), axum::extract::Query(params)).await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    // ============================================================================
    // POST /tracker/signature tests
    // ============================================================================

    #[tokio::test]
    async fn test_request_tracker_signature_invalid_hex() {
        // Test that invalid hex pubkey returns 400
        let state = create_mock_app_state().await;

        let request = TrackerSignatureRequest {
            issuer_pubkey: "not-hex!!!".to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            total_debt: 1000,
            timestamp: 1234567890,
            emergency: false,
        };

        let response =
            request_tracker_signature(axum::extract::State(state), axum::extract::Json(request))
                .await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_request_tracker_signature_wrong_length() {
        // Test that wrong-length pubkey returns 400
        let state = create_mock_app_state().await;

        // 32 bytes instead of 33
        let wrong_length = "0101010101010101010101010101010101010101010101010101010101010101";

        let request = TrackerSignatureRequest {
            issuer_pubkey: wrong_length.to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            total_debt: 1000,
            timestamp: 1234567890,
            emergency: false,
        };

        let response =
            request_tracker_signature(axum::extract::State(state), axum::extract::Json(request))
                .await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_request_tracker_signature_valid_structure() {
        // Test that valid request structure is accepted (will fail at signing stage due to no secret key)
        let state = create_mock_app_state().await;

        let request = TrackerSignatureRequest {
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            total_debt: 1000,
            timestamp: 1234567890,
            emergency: false,
        };

        let response =
            request_tracker_signature(axum::extract::State(state), axum::extract::Json(request))
                .await;

        // Without tracker_secret_key configured, it falls back to Ergo node API which will fail
        // in test environment. The request structure itself should be validated.
        let body = &response.1;
        if !body.success {
            let default_msg = "unknown".to_string();
            let error_msg = body.error.as_ref().unwrap_or(&default_msg);
            assert!(
                error_msg.contains("tracker")
                    || error_msg.contains("Tracker")
                    || error_msg.contains("sign")
                    || error_msg.contains("node"),
                "Expected tracker/signing-related error for valid request structure, got: {}",
                error_msg
            );
        }
    }

    #[tokio::test]
    async fn test_request_tracker_signature_emergency_flag() {
        // Test that emergency flag is accepted in request
        let state = create_mock_app_state().await;

        let request = TrackerSignatureRequest {
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            total_debt: 1000,
            timestamp: 1234567890,
            emergency: true, // Emergency flag
        };

        let response =
            request_tracker_signature(axum::extract::State(state), axum::extract::Json(request))
                .await;

        // The emergency flag should be accepted in the request structure
        let body = &response.1;
        if !body.success {
            let default_msg = "unknown".to_string();
            let error_msg = body.error.as_ref().unwrap_or(&default_msg);
            assert!(
                error_msg.contains("tracker")
                    || error_msg.contains("Tracker")
                    || error_msg.contains("sign")
                    || error_msg.contains("node"),
                "Expected tracker/signing-related error for valid request structure, got: {}",
                error_msg
            );
        }
    }

    // ============================================================================
    // POST /redemption/prepare tests
    // ============================================================================

    #[tokio::test]
    async fn test_prepare_redemption_invalid_hex() {
        // Test that invalid hex pubkey returns 400
        let state = create_mock_app_state().await;

        let request = RedemptionPreparationRequest {
            issuer_pubkey: "not-hex!!!".to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            amount: 1000,
            timestamp: 1234567890,
        };

        let response =
            prepare_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_prepare_redemption_wrong_length_pubkey() {
        // Test that wrong-length pubkey returns error (400 or 500 depending on validation stage)
        let state = create_mock_app_state().await;

        // 32 bytes instead of 33
        let wrong_length = "0101010101010101010101010101010101010101010101010101010101010101";

        let request = RedemptionPreparationRequest {
            issuer_pubkey: wrong_length.to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            amount: 1000,
            timestamp: 1234567890,
        };

        let response =
            prepare_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        // The handler validates hex first (passes for wrong_length since it's valid hex),
        // then later validates length which may return 500 (internal error) or 400
        // depending on where the validation happens. We just assert it doesn't succeed.
        assert_ne!(response.0, StatusCode::OK);
        let body = &response.1;
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_prepare_redemption_valid_structure() {
        // Test that valid request structure is accepted (will fail at Ergo node API call)
        let state = create_mock_app_state().await;

        let request = RedemptionPreparationRequest {
            issuer_pubkey: "010101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
            recipient_pubkey: "020202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
            amount: 1000,
            timestamp: 1234567890,
        };

        let response =
            prepare_redemption(axum::extract::State(state), axum::extract::Json(request)).await;

        // Without Ergo node available, it will fail at the signing stage.
        // The request structure should be validated correctly.
        let body = &response.1;
        if !body.success {
            let default_msg = "unknown".to_string();
            let error_msg = body.error.as_ref().unwrap_or(&default_msg);
            assert!(
                error_msg.contains("tracker")
                    || error_msg.contains("Tracker")
                    || error_msg.contains("sign")
                    || error_msg.contains("node"),
                "Expected tracker/signing-related error for valid request structure, got: {}",
                error_msg
            );
        }
    }

    // ============================================================================
    // Request/response model validation tests
    // ============================================================================

    #[tokio::test]
    async fn test_redeem_request_deserialization() {
        // Test that RedeemRequest deserializes correctly from JSON
        let json = r#"{
            "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
            "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
            "amount": 1000,
            "timestamp": 1234567890,
            "issuer_signature": "01"
        }"#;

        let request: RedeemRequest = serde_json::from_str(json).unwrap();

        assert_eq!(
            request.issuer_pubkey,
            "010101010101010101010101010101010101010101010101010101010101010101"
        );
        assert_eq!(
            request.recipient_pubkey,
            "020202020202020202020202020202020202020202020202020202020202020202"
        );
        assert_eq!(request.amount, 1000);
        assert_eq!(request.timestamp, 1234567890);
        assert_eq!(request.reserve_box_id, ""); // Default
        assert_eq!(request.recipient_address, ""); // Default
        assert_eq!(request.emergency, false); // Default
        assert_eq!(request.issuer_signature, "01");
    }

    #[tokio::test]
    async fn test_complete_redemption_request_deserialization() {
        // Test CompleteRedemptionRequest deserialization from JSON
        let json = r#"{
            "redemption_id": "redemption_test_123",
            "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
            "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
            "redeemed_amount": 500
        }"#;

        let request: CompleteRedemptionRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.redemption_id, "redemption_test_123");
        assert_eq!(
            request.issuer_pubkey,
            "010101010101010101010101010101010101010101010101010101010101010101"
        );
        assert_eq!(
            request.recipient_pubkey,
            "020202020202020202020202020202020202020202020202020202020202020202"
        );
        assert_eq!(request.redeemed_amount, 500);
    }

    #[tokio::test]
    async fn test_tracker_signature_request_deserialization() {
        // Test TrackerSignatureRequest deserialization from JSON
        let json = r#"{
            "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
            "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
            "total_debt": 10000,
            "timestamp": 1234567890,
            "emergency": true
        }"#;

        let request: TrackerSignatureRequest = serde_json::from_str(json).unwrap();

        assert_eq!(
            request.issuer_pubkey,
            "010101010101010101010101010101010101010101010101010101010101010101"
        );
        assert_eq!(
            request.recipient_pubkey,
            "020202020202020202020202020202020202020202020202020202020202020202"
        );
        assert_eq!(request.total_debt, 10000);
        assert_eq!(request.timestamp, 1234567890);
        assert_eq!(request.emergency, true);
    }

    #[tokio::test]
    async fn test_redemption_preparation_request_deserialization() {
        // Test RedemptionPreparationRequest deserialization from JSON
        let json = r#"{
            "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
            "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
            "amount": 1000,
            "timestamp": 1234567890
        }"#;

        let request: RedemptionPreparationRequest = serde_json::from_str(json).unwrap();

        assert_eq!(
            request.issuer_pubkey,
            "010101010101010101010101010101010101010101010101010101010101010101"
        );
        assert_eq!(
            request.recipient_pubkey,
            "020202020202020202020202020202020202020202020202020202020202020202"
        );
        assert_eq!(request.amount, 1000);
        assert_eq!(request.timestamp, 1234567890);
    }

    #[tokio::test]
    async fn test_redeem_request_default_fields() {
        // Test that serde default fields work correctly
        let json = r#"{
            "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
            "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
            "amount": 1000,
            "timestamp": 1234567890,
            "issuer_signature": "01"
        }"#;

        let request: RedeemRequest = serde_json::from_str(json).unwrap();

        // Default values should be applied
        assert_eq!(request.reserve_box_id, "");
        assert_eq!(request.recipient_address, "");
        assert_eq!(request.emergency, false);
        assert_eq!(request.issuer_signature, "01");
    }
}
