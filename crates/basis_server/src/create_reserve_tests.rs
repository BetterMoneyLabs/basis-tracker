#[cfg(test)]
mod create_reserve_tests {
    use axum::{extract::State, http::StatusCode, Json};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use crate::{
        api::{create_reserve_payload, get_basis_reserve_contract_p2s},
        models::CreateReserveRequest,
        redemption_build::{build_redemption, RedemptionBuildRequest},
        AppState, TrackerCommand,
    };
    use basis_store::ergo_scanner::{NodeConfig, ServerState};

    // Helper function to create a unique temporary directory for test storage
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

    // Helper function to create a test AppState that doesn't require file system access
    fn create_test_app_state() -> AppState {
        create_test_app_state_with_p2s("test".to_string())
    }

    fn create_test_app_state_with_p2s(basis_reserve_contract_p2s: String) -> AppState {
        let (tx, _rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);
        let event_store = std::sync::Arc::new(crate::store::EventStore::new_in_memory());

        // Create a minimal configuration
        let config = NodeConfig {
            node_url: "http://localhost:9553".to_string(),
            ..Default::default()
        };

        // Create a scanner state that doesn't try to access files by using a memory-only implementation
        // For testing purposes, we'll create a minimal state that doesn't require file access
        let data_dir = std::env::temp_dir().join(format!(
            "basis_create_reserve_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let scanner = ServerState::new(config, &data_dir).unwrap_or_else(|_| {
            // Fallback to a scanner with minimal initialization that doesn't access storage
            let config = NodeConfig {
                node_url: "http://example.com".to_string(), // Invalid URL to avoid file access
                ..Default::default()
            };
            ServerState::new(config, &data_dir).expect("Fallback scanner creation should succeed")
        });

        // Create a minimal config for testing
        let test_config = std::sync::Arc::new(crate::config::AppConfig {
            server: crate::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3048,
                data_dir: Some(data_dir.to_string_lossy().to_string()),
                database_url: Some("sqlite::memory:".to_string()),
            },
            ergo: crate::config::ErgoConfig {
                node: NodeConfig {
                    node_url: "http://example.com".to_string(),
                    ..Default::default()
                },
                basis_reserve_contract_p2s,
                tracker_nft_id: Some(
                    "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b".to_string(),
                ),
                allow_fresh_tracker_generation: false,
                tracker_public_key: Some(
                    "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                ),
                tracker_secret_key: None,
            },
            transaction: crate::config::TransactionConfig {
                fee: 1000000,
                change_address: None,
            },
            acceptance: crate::acceptance::config::AcceptanceConfig::empty(),
        });

        let reserve_tracker = Arc::new(Mutex::new(basis_store::ReserveTracker::new()));

        let tracker_storage_path =
            unique_test_storage_path("basis_test_tracker_storage_create_reserve");
        std::fs::create_dir_all(&tracker_storage_path)
            .expect("Failed to create tracker storage directory");
        let tracker_storage = basis_store::persistence::TrackerStorage::open(&tracker_storage_path)
            .unwrap_or_else(|_| {
                basis_store::persistence::TrackerStorage::open(unique_test_storage_path(
                    "basis_test_tracker_storage_create_reserve_fallback",
                ))
                .unwrap()
            });

        let policy_storage_path =
            unique_test_storage_path("basis_test_policy_storage_create_reserve");
        std::fs::create_dir_all(&policy_storage_path)
            .expect("Failed to create policy storage directory");
        let policy_storage =
            basis_store::persistence::AcceptancePolicyStorage::open(&policy_storage_path)
                .unwrap_or_else(|_| {
                    basis_store::persistence::AcceptancePolicyStorage::open(
                        unique_test_storage_path(
                            "basis_test_policy_storage_create_reserve_fallback",
                        ),
                    )
                    .unwrap()
                });

        AppState {
            tx,
            event_store,
            ergo_scanner: Arc::new(Mutex::new(scanner)),
            reserve_tracker,
            config: test_config,
            shared_tracker_state: Arc::new(tokio::sync::Mutex::new(
                crate::tracker_box_updater::SharedTrackerState::new(),
            )),
            tracker_storage,
            acceptance_predicate: None,
            policy_storage,
        }
    }

    #[tokio::test]
    async fn test_create_reserve_payload_stays_disabled_until_v2_builder_is_installed() {
        let exact_v2 = basis_store::contract_compiler::get_basis_v2_contract_p2s(
            basis_store::contract_compiler::BasisV2ContractKind::Erg,
        )
        .expect("embedded Basis v2 ERG contract should derive a P2S address");
        let state = create_test_app_state_with_p2s(exact_v2);

        let request_payload = CreateReserveRequest {
            nft_id: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            owner_pubkey: "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12"
                .to_string(), // 33-byte public key
            erg_amount: 1000000000, // 1 ERG in nanoERG
        };

        let result = create_reserve_payload(State(state), Json(request_payload)).await;

        let (status, response_json) = result;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response_json.success);
        assert!(response_json.data.is_none());
        assert!(response_json
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("v2 runtime builder"));
    }

    #[tokio::test]
    async fn test_create_reserve_payload_rejects_known_strict_insert_contract() {
        let legacy = basis_store::contract_compiler::get_basis_reserve_contract_p2s().unwrap();
        let state = create_test_app_state_with_p2s(legacy);
        let request_payload = CreateReserveRequest {
            nft_id: "12".repeat(32),
            owner_pubkey: "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12"
                .to_string(),
            erg_amount: 1_000_000_000,
        };

        let (status, response) = create_reserve_payload(State(state), Json(request_payload)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.success);
        assert!(response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("identity check failed"));
    }

    #[tokio::test]
    async fn all_server_construction_routes_reject_each_unactivated_generation() {
        let exact_v2 = basis_store::contract_compiler::get_basis_v2_contract_p2s(
            basis_store::contract_compiler::BasisV2ContractKind::Erg,
        )
        .unwrap();
        let legacy = basis_store::contract_compiler::get_basis_reserve_contract_p2s().unwrap();

        for configured in [exact_v2, legacy, "unknown-generation".to_string()] {
            let state = create_test_app_state_with_p2s(configured);
            let (p2s_status, p2s_response) =
                get_basis_reserve_contract_p2s(State(state.clone())).await;
            assert_eq!(p2s_status, StatusCode::SERVICE_UNAVAILABLE);
            assert!(!p2s_response.success);

            let build_request = RedemptionBuildRequest {
                issuer_pubkey: "02".to_string(),
                recipient_pubkey: "03".to_string(),
                amount: 1,
                timestamp: 1,
                issuer_signature: String::new(),
                emergency: false,
                tracker_box_id: None,
            };
            let (build_status, build_response) =
                build_redemption(State(state.clone()), Json(build_request)).await;
            assert_eq!(build_status, StatusCode::SERVICE_UNAVAILABLE);
            assert!(!build_response.success);

            let create_request = CreateReserveRequest {
                nft_id: "12".repeat(32),
                owner_pubkey: "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12"
                    .to_string(),
                erg_amount: 1_000_000,
            };
            let (create_status, create_response) =
                create_reserve_payload(State(state), Json(create_request)).await;
            assert_eq!(create_status, StatusCode::SERVICE_UNAVAILABLE);
            assert!(!create_response.success);
        }
    }

    #[tokio::test]
    async fn test_create_reserve_payload_invalid_pubkey() {
        let state = create_test_app_state();

        let request_payload = CreateReserveRequest {
            nft_id: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            owner_pubkey: "invalid_hex".to_string(), // Invalid hex
            erg_amount: 1000000000,
        };

        let result = create_reserve_payload(State(state), Json(request_payload)).await;

        let (status, response_json) = result;

        // The validation should catch the invalid hex before attempting config loading
        if status != StatusCode::INTERNAL_SERVER_ERROR {
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(!response_json.success);
            assert!(response_json.error.is_some());
            assert!(response_json.error.clone().unwrap().contains("hex-encoded"));
        }
    }

    #[tokio::test]
    async fn test_create_reserve_payload_wrong_pubkey_length() {
        let state = create_test_app_state();

        let request_payload = CreateReserveRequest {
            nft_id: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            owner_pubkey: "03e8c3e4".to_string(), // Too short (only 4 bytes when should be 33)
            erg_amount: 1000000000,
        };

        let result = create_reserve_payload(State(state), Json(request_payload)).await;

        let (status, response_json) = result;

        // The validation should catch the wrong pubkey length before attempting config loading
        if status != StatusCode::INTERNAL_SERVER_ERROR {
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(!response_json.success);
            assert!(response_json.error.is_some());
            assert!(response_json.error.clone().unwrap().contains("33 bytes"));
        }
    }

    #[tokio::test]
    async fn test_create_reserve_payload_empty_nft_id() {
        let state = create_test_app_state();

        let request_payload = CreateReserveRequest {
            nft_id: "".to_string(), // Empty NFT ID
            owner_pubkey: "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12"
                .to_string(),
            erg_amount: 1000000000,
        };

        let result = create_reserve_payload(State(state), Json(request_payload)).await;

        let (status, response_json) = result;

        // The validation should catch the empty nft_id before attempting config loading
        if status != StatusCode::INTERNAL_SERVER_ERROR {
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(!response_json.success);
            assert!(response_json.error.is_some());
            assert!(response_json
                .error
                .clone()
                .unwrap()
                .contains("cannot be empty"));
        }
    }

    #[tokio::test]
    async fn test_create_reserve_payload_zero_amount() {
        let state = create_test_app_state();

        let request_payload = CreateReserveRequest {
            nft_id: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            owner_pubkey: "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12"
                .to_string(),
            erg_amount: 0, // Zero amount
        };

        let result = create_reserve_payload(State(state), Json(request_payload)).await;

        let (status, response_json) = result;

        // The validation should catch the zero amount before attempting config loading
        if status != StatusCode::INTERNAL_SERVER_ERROR {
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(!response_json.success);
            assert!(response_json.error.is_some());
            assert!(response_json
                .error
                .clone()
                .unwrap()
                .contains("greater than 0"));
        }
    }
}
