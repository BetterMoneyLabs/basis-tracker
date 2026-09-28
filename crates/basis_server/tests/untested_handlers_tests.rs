//! Coverage for the eight HTTP handlers that no existing test exercised.
//!
//! Verified by grep that neither the handler names nor the route paths appeared in any test
//! file before this one:
//!
//! | Route | Handler |
//! |---|---|
//! | `GET /tracker/proof` | `get_tracker_proof` |
//! | `GET /reserve/proof` | `get_reserve_proof` |
//! | `POST /redemption/submit` | `submit_redemption` |
//! | `GET /config/reserve-token` | `get_reserve_token_config` |
//! | `GET /config/reserve-contract-p2s` | `get_basis_reserve_contract_p2s` |
//! | `GET /tracker/latest-box-id` | `get_latest_tracker_box_id` |
//! | `POST /reserves/create` | `create_reserve_payload` |
//! | `POST /reserves/submit` | `submit_reserve_transaction` |
//!
//! `POST /redemption/submit` is the notable one: it broadcasts a signed transaction and then
//! advances tracker state from the request body at mempool-acceptance time, which the audit
//! rates as a top open finding (Issue #4). These tests pin its *current* behaviour, including
//! the state-advance-on-broadcast semantics, so that a future fix is a visible diff.

mod untested_handlers {
    use axum::http::StatusCode;
    use basis_server::{
        api::{
            create_reserve_payload, get_basis_reserve_contract_p2s, get_latest_tracker_box_id,
            get_reserve_proof, get_reserve_token_config, get_tracker_proof,
            submit_reserve_transaction,
        },
        models::{CreateReserveRequest, ReserveCreationResponse},
        redemption_build::submit_redemption,
        AppState, TrackerCommand,
    };
    use basis_store::{schnorr::generate_keypair, IouNote};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// Fjall keyspace creation can race when several databases open concurrently in one
    /// process, so serialise storage initialisation.
    static STORAGE_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const ISSUER_HEX: &str = "010101010101010101010101010101010101010101010101010101010101010101";
    const RECIPIENT_HEX: &str =
        "020202020202020202020202020202020202020202020202020202020202020202";

    /// Mock state whose tracker thread understands the commands these handlers issue.
    async fn mock_state() -> AppState {
        mock_state_with_config("http://localhost:9053").await
    }

    async fn mock_state_with_config(node_url: &str) -> AppState {
        let (tx, mut rx) = mpsc::channel(100);
        let event_store = Arc::new(basis_server::store::EventStore::new().await.unwrap());

        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();

        let scanner_config = basis_store::ergo_scanner::NodeConfig {
            node_url: node_url.to_string(),
            ..Default::default()
        };
        let ergo_scanner = Arc::new(tokio::sync::Mutex::new(
            basis_store::ergo_scanner::ServerState::new(scanner_config, data_dir).unwrap(),
        ));
        let reserve_tracker = Arc::new(tokio::sync::Mutex::new(basis_store::ReserveTracker::new()));

        tokio::task::spawn_blocking(move || {
            use basis_store::{RedemptionManager, TrackerStateManager};

            let tracker = TrackerStateManager::new_with_temp_storage();
            let mut redemption_manager = RedemptionManager::new(tracker);

            while let Some(cmd) = rx.blocking_recv() {
                match cmd {
                    TrackerCommand::AddNote {
                        issuer_pubkey,
                        note,
                        response_tx,
                    } => {
                        let r = redemption_manager.tracker.add_note(&issuer_pubkey, &note);
                        let _ = response_tx.send(r);
                    }
                    TrackerCommand::GetNoteByIssuerAndRecipient {
                        issuer_pubkey,
                        recipient_pubkey,
                        response_tx,
                    } => {
                        let r = redemption_manager
                            .tracker
                            .lookup_note(&issuer_pubkey, &recipient_pubkey)
                            .map(Some);
                        let _ = response_tx.send(r);
                    }
                    TrackerCommand::CompleteRedemption {
                        issuer_pubkey,
                        recipient_pubkey,
                        redeemed_amount,
                        new_already_redeemed,
                        response_tx,
                    } => {
                        let r = redemption_manager.complete_redemption(
                            &issuer_pubkey,
                            &recipient_pubkey,
                            redeemed_amount,
                            new_already_redeemed,
                        );
                        let _ = response_tx.send(r);
                    }
                    other => {
                        // Commands these tests do not exercise.
                        let _ = other;
                    }
                }
            }
        });

        let config = Arc::new(basis_server::config::AppConfig {
            server: basis_server::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3048,
                data_dir: Some(data_dir.to_string_lossy().to_string()),
                database_url: Some("sqlite::memory:".to_string()),
                tls_cert_path: None,
                tls_key_path: None,
                auth: basis_server::config::AuthConfig::default(),
            },
            ergo: basis_server::config::ErgoConfig {
                node: basis_store::ergo_scanner::NodeConfig {
                    node_url: node_url.to_string(),
                    ..Default::default()
                },
                basis_reserve_contract_p2s: "9testReserveP2SAddress".to_string(),
                basis_token_reserve_contract_p2s: "9testTokenReserveP2S".to_string(),
                tracker_nft_id: Some(
                    "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
                ),
                reserve_token_id: None,
                reserve_token_decimals: 0,
                tracker_public_key: Some(
                    "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                ),
                tracker_secret_key: None,
            },
            transaction: basis_server::config::TransactionConfig {
                fee: 1_000_000,
                change_address: None,
            },
            acceptance: basis_server::acceptance::config::AcceptanceConfig::empty(),
            redemption: basis_server::config::RedemptionConfig {
                enforce_acceptance_policy: true,
            },
            confirmation: basis_server::config::ConfirmationConfig::default(),
        });

        let storage_dir = std::env::temp_dir().join(format!(
            "basis_untested_handlers_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&storage_dir).expect("create temp dir");

        let (tracker_storage, policy_storage) = {
            let _guard = STORAGE_INIT_LOCK.lock().unwrap();
            let ts = basis_store::persistence::TrackerStorage::open(&storage_dir)
                .expect("tracker storage");
            let ps = basis_store::persistence::AcceptancePolicyStorage::open(
                storage_dir.join("policies"),
            )
            .expect("policy storage");
            (ts, ps)
        };

        AppState {
            tx,
            event_store,
            ergo_scanner,
            reserve_tracker,
            config,
            shared_tracker_state: Arc::new(tokio::sync::Mutex::new(
                basis_server::tracker_box_updater::SharedTrackerState::new(),
            )),
            tracker_storage,
            acceptance_predicate: None,
            policy_storage,
        }
    }

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ========================================================================
    // GET /config/reserve-token
    // ========================================================================

    #[tokio::test]
    async fn test_get_reserve_token_config_returns_erg_backed_defaults() {
        let state = mock_state().await;

        let (status, body) = get_reserve_token_config(axum::extract::State(state)).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.success);
        assert!(
            body.data.is_some(),
            "a successful config response must carry a payload"
        );
        let data = body.data.clone().unwrap();
        assert_eq!(
            data.basis_token_reserve_contract_p2s, "9testTokenReserveP2S",
            "the configured token-reserve P2S must be surfaced verbatim"
        );
    }

    // ========================================================================
    // GET /config/reserve-contract-p2s
    // ========================================================================

    #[tokio::test]
    async fn test_get_basis_reserve_contract_p2s_reports_config_load_failure() {
        // This handler re-reads AppConfig from disk instead of using the injected AppState, so
        // its result depends on the deployment environment. In a test environment there is no
        // config/basis.toml (it is gitignored), so what we pin is the documented failure path:
        // a 5xx carrying an error, never a panic and never a silent empty string.
        let state = mock_state().await;

        let (status, body) = get_basis_reserve_contract_p2s(axum::extract::State(state)).await;

        assert!(
            status.is_server_error() || status == StatusCode::OK,
            "expected either a successful load or a clean 5xx, got {} with body {:?}",
            status,
            body
        );
        if status.is_server_error() {
            assert!(!body.success);
            assert!(
                body.error.is_some(),
                "a 5xx must carry an error message, not an empty body"
            );
        }
    }

    // ========================================================================
    // GET /tracker/latest-box-id
    // ========================================================================

    #[tokio::test]
    async fn test_get_latest_tracker_box_id_returns_404_before_first_update() {
        let state = mock_state().await;

        let (status, body) = get_latest_tracker_box_id(axum::extract::State(state)).await;

        // Before the background updater has observed a confirmed tracker box there is nothing
        // to report, and 404 is the correct answer. Returning 200 with an empty or invented id
        // would be worse: clients use this to locate the tracker data-input box.
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "no tracker box observed yet, body: {:?}",
            body
        );
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_get_latest_tracker_box_id_reflects_shared_state() {
        // With a box id present in shared state the handler must report exactly that id.
        let state = mock_state().await;
        {
            let shared = state.shared_tracker_state.lock().await;
            shared.set_tracker_box_id("box_from_updater_123".to_string());
        }

        let (status, body) = get_latest_tracker_box_id(axum::extract::State(state)).await;

        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
        assert!(body.success);
        let data = body
            .data
            .as_ref()
            .expect("latest-box-id response should carry data");
        assert_eq!(
            data.tracker_box_id, "box_from_updater_123",
            "the handler must surface the box id recorded by the updater"
        );
    }

    // ========================================================================
    // GET /tracker/proof
    // ========================================================================

    #[tokio::test]
    async fn test_get_tracker_proof_missing_params_is_bad_request() {
        let state = mock_state().await;

        let (status, body) = get_tracker_proof(
            axum::extract::State(state),
            axum::extract::Query(params(&[])),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_get_tracker_proof_invalid_hex_is_bad_request() {
        let state = mock_state().await;

        let (status, body) = get_tracker_proof(
            axum::extract::State(state),
            axum::extract::Query(params(&[
                ("issuer_pubkey", "not-hex"),
                ("recipient_pubkey", RECIPIENT_HEX),
            ])),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn test_get_tracker_proof_unknown_note_is_not_ok() {
        let state = mock_state().await;

        let (status, body) = get_tracker_proof(
            axum::extract::State(state),
            axum::extract::Query(params(&[
                ("issuer_pubkey", ISSUER_HEX),
                ("recipient_pubkey", RECIPIENT_HEX),
            ])),
        )
        .await;

        assert_ne!(
            status,
            StatusCode::OK,
            "a proof for a note that was never added must not succeed"
        );
        assert!(!body.success);
    }

    // ========================================================================
    // GET /reserve/proof
    // ========================================================================

    #[tokio::test]
    async fn test_get_reserve_proof_missing_params_is_bad_request() {
        let state = mock_state().await;

        let (status, body) = get_reserve_proof(
            axum::extract::State(state),
            axum::extract::Query(params(&[])),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn test_get_reserve_proof_invalid_hex_is_bad_request() {
        let state = mock_state().await;

        let (status, body) = get_reserve_proof(
            axum::extract::State(state),
            axum::extract::Query(params(&[("issuer_pubkey", "zzz")])),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn test_get_reserve_proof_unknown_reserve_is_not_ok() {
        let state = mock_state().await;

        let (status, body) = get_reserve_proof(
            axum::extract::State(state),
            axum::extract::Query(params(&[
                ("issuer_pubkey", ISSUER_HEX),
                ("recipient_pubkey", RECIPIENT_HEX),
                ("amount", "1000"),
                ("timestamp", "1700000000000"),
            ])),
        )
        .await;

        assert_ne!(
            status,
            StatusCode::OK,
            "a reserve proof for an unknown reserve must not succeed"
        );
        assert!(!body.success);
    }

    // ========================================================================
    // POST /reserves/create
    // ========================================================================

    #[tokio::test]
    async fn test_create_reserve_payload_rejects_invalid_owner_hex() {
        let state = mock_state().await;

        let (status, body) = create_reserve_payload(
            axum::extract::State(state),
            axum::extract::Json(CreateReserveRequest {
                owner_pubkey: "not-hex".to_string(),
                nft_id: "abc".to_string(),
                erg_amount: 1_000_000,
                token_amount: 0,
                token_id: String::new(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    #[tokio::test]
    async fn test_create_reserve_payload_rejects_wrong_length_owner_key() {
        let state = mock_state().await;

        // 32 bytes, not 33.
        let short = "01".repeat(32);
        let (status, body) = create_reserve_payload(
            axum::extract::State(state),
            axum::extract::Json(CreateReserveRequest {
                owner_pubkey: short,
                nft_id: "abc".to_string(),
                erg_amount: 1_000_000,
                token_amount: 0,
                token_id: String::new(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert!(!body.success);
    }

    // ========================================================================
    // POST /reserves/submit
    // ========================================================================

    #[tokio::test]
    async fn test_submit_reserve_transaction_without_node_is_service_unavailable() {
        // No node configured: the handler must refuse cleanly rather than panic or hang.
        let state = mock_state_with_config("").await;

        let (status, body) = submit_reserve_transaction(
            axum::extract::State(state),
            axum::extract::Json(ReserveCreationResponse {
                requests: vec![],
                fee: 1_000_000,
                change_address: "9test".to_string(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn test_submit_reserve_transaction_unreachable_node_is_bad_gateway() {
        // Port 1 is reserved and refuses connections, so this exercises the upstream-failure
        // branch without needing a live node.
        let state = mock_state_with_config("http://127.0.0.1:1").await;

        let (status, body) = submit_reserve_transaction(
            axum::extract::State(state),
            axum::extract::Json(ReserveCreationResponse {
                requests: vec![],
                fee: 1_000_000,
                change_address: "9test".to_string(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_GATEWAY, "body: {:?}", body);
        assert!(!body.success);
        assert!(body.error.is_some());
    }

    // ========================================================================
    // POST /redemption/submit
    //
    // This handler broadcasts a client-supplied signed transaction and then advances tracker
    // state from the request body. specs/PRODUCTION_READINESS_AUDIT.md Issue #4 rates this the
    // top open finding: no confirmation-depth gate, the transaction's contents are never
    // inspected, and redeemed_amount / new_already_redeemed are trusted verbatim. The tests
    // below pin that behaviour so a future fix shows up as a failing test.
    // ========================================================================

    /// Node stub that accepts any broadcast and returns a fixed tx id.
    async fn spawn_accepting_node() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock node");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 8192];
                    let _ = socket.read(&mut buf).await;
                    let body = format!("{{\"id\":\"{}\"}}", "ab".repeat(32));
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{}", addr)
    }

    fn submit_request() -> basis_server::redemption_build::RedemptionSubmitRequest {
        basis_server::redemption_build::RedemptionSubmitRequest {
            issuer_pubkey: ISSUER_HEX.to_string(),
            recipient_pubkey: RECIPIENT_HEX.to_string(),
            signed_tx: serde_json::json!({ "id": "unsigned-placeholder" }),
            redeemed_amount: 1_000,
            new_already_redeemed: 1_000,
        }
    }

    #[tokio::test]
    async fn test_submit_redemption_rejects_invalid_issuer_pubkey() {
        let state = mock_state().await;
        let mut req = submit_request();
        req.issuer_pubkey = "not-hex".to_string();

        let (status, body) =
            submit_redemption(axum::extract::State(state), axum::extract::Json(req)).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn test_submit_redemption_rejects_wrong_length_pubkey() {
        let state = mock_state().await;
        let mut req = submit_request();
        req.recipient_pubkey = "02".repeat(32); // 32 bytes, not 33

        let (status, body) =
            submit_redemption(axum::extract::State(state), axum::extract::Json(req)).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn test_submit_redemption_failed_broadcast_returns_500() {
        let state = mock_state_with_config("http://127.0.0.1:1").await;

        let (status, body) = submit_redemption(
            axum::extract::State(state),
            axum::extract::Json(submit_request()),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "body: {:?}",
            body
        );
        assert!(!body.success);
        let msg = body.error.clone().unwrap_or_default();
        assert!(
            msg.contains("broadcast failed"),
            "a failed broadcast must be reported as such, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_submit_redemption_advances_state_on_broadcast_acceptance() {
        // Characterisation test for audit Issue #4.
        //
        // The node stub accepts anything, and the handler still credits `redeemed_amount`
        // from the request body: no confirmation-depth check, and no inspection of what the
        // transaction actually does. Pinning this means the recommended fix surfaces as a
        // failing test rather than silent behavioural drift.
        let node_url = spawn_accepting_node().await;
        let state = mock_state_with_config(&node_url).await;

        let (secret, issuer) = generate_keypair();
        let (_, recipient) = generate_keypair();
        let note = IouNote::create_and_sign(recipient, 10_000, 1_700_000_000_000, &secret).unwrap();
        state
            .tx
            .send(TrackerCommand::AddNote {
                issuer_pubkey: issuer,
                note,
                response_tx: tokio::sync::oneshot::channel().0,
            })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut req = submit_request();
        req.issuer_pubkey = hex::encode(issuer);
        req.recipient_pubkey = hex::encode(recipient);

        let (status, body) = submit_redemption(
            axum::extract::State(state.clone()),
            axum::extract::Json(req),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
        assert!(body.success, "error: {:?}", body.error);

        let (tx, rx) = tokio::sync::oneshot::channel();
        state
            .tx
            .send(TrackerCommand::GetNoteByIssuerAndRecipient {
                issuer_pubkey: issuer,
                recipient_pubkey: recipient,
                response_tx: tx,
            })
            .await
            .unwrap();
        let note = rx
            .await
            .expect("tracker should answer the note lookup")
            .expect("note lookup should succeed")
            .expect("note should exist");

        assert_eq!(
            note.amount_redeemed, 1_000,
            "state was advanced from the request body on broadcast acceptance alone"
        );
    }
}
