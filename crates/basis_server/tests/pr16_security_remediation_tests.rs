//! Handler-level tests for the PR #16 security-review remediations that need an `AppState`:
//!
//! * H2 — fail-closed reserve selection: `/tracker/signature`, `/redemption/prepare` and
//!   `/redemption/build` answer 409 when the issuer has more than one tracked reserve.
//! * M1 — `/redemption/build` is rate limited per (issuer, recipient) pair (429).

mod pr16_security_remediation {
    use axum::http::StatusCode;
    use basis_server::{
        api::{prepare_redemption, request_tracker_signature},
        models::{RedemptionPreparationRequest, TrackerSignatureRequest},
        redemption_build::{build_redemption, RedemptionBuildRequest},
        AppState, TrackerCommand,
    };
    use basis_store::{schnorr::generate_keypair, IouNote};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// Fjall keyspace creation can race when several databases open concurrently in one
    /// process, so serialise storage initialisation.
    static STORAGE_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Mock state whose tracker thread understands the commands these handlers issue.
    async fn mock_state(max_builds_per_pair_per_hour: u32) -> AppState {
        let (tx, mut rx) = mpsc::channel(100);
        let event_store = Arc::new(basis_server::store::EventStore::new().await.unwrap());

        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();

        let scanner_config = basis_store::ergo_scanner::NodeConfig {
            node_url: "http://localhost:9053".to_string(),
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
                        denomination,
                        response_tx,
                    } => {
                        let r = redemption_manager.tracker.get_note_optional(
                            &issuer_pubkey,
                            &recipient_pubkey,
                            &denomination,
                        );
                        let _ = response_tx.send(r);
                    }
                    TrackerCommand::GetNotesByIssuer {
                        issuer_pubkey,
                        response_tx,
                    } => {
                        let r = redemption_manager.tracker.get_issuer_notes(&issuer_pubkey);
                        let _ = response_tx.send(r);
                    }
                    TrackerCommand::CompleteRedemption {
                        issuer_pubkey,
                        recipient_pubkey,
                        redeemed_amount,
                        new_already_redeemed,
                        denomination,
                        response_tx,
                    } => {
                        let r = redemption_manager.complete_redemption(
                            &issuer_pubkey,
                            &recipient_pubkey,
                            &basis_store::DenomKey::Erg,
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
                reserve_creation_max_erg_nano: 10_000_000_000,
                reserve_creation_max_token_amount: 1_000_000_000,
                redemption_build_max_per_pair_per_hour: max_builds_per_pair_per_hour,
            },
            ergo: basis_server::config::ErgoConfig {
                node: basis_store::ergo_scanner::NodeConfig {
                    node_url: "http://localhost:9053".to_string(),
                    ..Default::default()
                },
                basis_reserve_contract_p2s: "9testReserveP2SAddress".to_string(),
                basis_token_reserve_contract_p2s: "9testTokenReserveP2S".to_string(),
                tracker_nft_id: Some(
                    "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
                ),
                reserve_tokens: Default::default(),
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
            "basis_pr16_tests_{}_{}",
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
            pending_redemptions: Arc::new(
                basis_server::redemption_build::PendingRedemptionRegistry::new(),
            ),
            redemption_build_rate: Arc::new(basis_server::redemption_build::BuildRateLimiter::new()),
        }
    }

    /// Add a tracked reserve owned by `owner_hex` to the in-memory reserve tracker.
    async fn add_reserve(state: &AppState, owner_hex: &str, box_id: &str) {
        let tracker = state.reserve_tracker.lock().await;
        tracker
            .update_reserve(basis_store::ExtendedReserveInfo {
                base_info: basis_store::ReserveInfo {
                    collateral_amount: 10_000_000_000,
                    last_updated_height: 0,
                    contract_address: "test".to_string(),
                    tracker_nft_id: "test".to_string(),
                    refund_initiation_height: 0,
                    reserve_token_id: String::new(),
                },
                total_debt: 0,
                box_id: box_id.to_string(),
                owner_pubkey: owner_hex.to_string(),
                last_updated_timestamp: 0,
            })
            .unwrap();
    }

    /// Store a note for (issuer, recipient) with cumulative debt 10_000 and a fixed timestamp.
    async fn add_note(state: &AppState) -> ([u8; 33], [u8; 33]) {
        let (secret, issuer) = generate_keypair();
        let (_, recipient) = generate_keypair();
        let note = IouNote::create_and_sign(
            recipient,
            10_000,
            1_700_000_000_000,
            &secret,
            &[0x42; 32],
            basis_store::DenomKey::Erg,
        )
        .unwrap();
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
        (issuer, recipient)
    }

    const NOTE_TIMESTAMP: u64 = 1_700_000_000_000;
    const NOTE_TOTAL_DEBT: u64 = 10_000;

    // ------------------------------------------------------------------
    // H2: fail-closed reserve selection
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn tracker_signature_conflicts_when_issuer_has_two_reserves() {
        let state = mock_state(60).await;
        let (issuer, recipient) = add_note(&state).await;
        let issuer_hex = hex::encode(issuer);
        add_reserve(&state, &issuer_hex, "box_a").await;
        add_reserve(&state, &issuer_hex, "box_b").await;

        let (status, body) = request_tracker_signature(
            axum::extract::State(state),
            axum::extract::Json(TrackerSignatureRequest {
                issuer_pubkey: issuer_hex,
                recipient_pubkey: hex::encode(recipient),
                total_debt: NOTE_TOTAL_DEBT,
                timestamp: NOTE_TIMESTAMP,
                emergency: false,

                denomination: String::new(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "body: {:?}", body);
        assert!(!body.success);
        assert!(body.error.clone().unwrap().contains("tracked reserves"));
    }

    #[tokio::test]
    async fn tracker_signature_single_reserve_passes_fail_closed_check() {
        let state = mock_state(60).await;
        let (issuer, recipient) = add_note(&state).await;
        let issuer_hex = hex::encode(issuer);
        add_reserve(&state, &issuer_hex, "box_a").await;

        let (status, _body) = request_tracker_signature(
            axum::extract::State(state),
            axum::extract::Json(TrackerSignatureRequest {
                issuer_pubkey: issuer_hex,
                recipient_pubkey: hex::encode(recipient),
                total_debt: NOTE_TOTAL_DEBT,
                timestamp: NOTE_TIMESTAMP,
                emergency: false,

                denomination: String::new(),
            }),
        )
        .await;

        // The request must get PAST the fail-closed reserve check (it fails later, at the
        // signing step, because no tracker secret key or reachable node is configured here).
        assert_ne!(
            status,
            StatusCode::CONFLICT,
            "a single reserve must not trip the fail-closed check"
        );
    }

    #[tokio::test]
    async fn prepare_redemption_conflicts_when_issuer_has_two_reserves() {
        let state = mock_state(60).await;
        let (issuer, recipient) = add_note(&state).await;
        let issuer_hex = hex::encode(issuer);
        add_reserve(&state, &issuer_hex, "box_a").await;
        add_reserve(&state, &issuer_hex, "box_b").await;

        let (status, body) = prepare_redemption(
            axum::extract::State(state),
            axum::extract::Json(RedemptionPreparationRequest {
                issuer_pubkey: issuer_hex,
                recipient_pubkey: hex::encode(recipient),
                amount: 1_000,
                timestamp: NOTE_TIMESTAMP,
                emergency: false,

                denomination: String::new(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn prepare_redemption_single_reserve_passes_fail_closed_check() {
        let state = mock_state(60).await;
        let (issuer, recipient) = add_note(&state).await;
        let issuer_hex = hex::encode(issuer);
        add_reserve(&state, &issuer_hex, "box_a").await;

        let (status, _body) = prepare_redemption(
            axum::extract::State(state),
            axum::extract::Json(RedemptionPreparationRequest {
                issuer_pubkey: issuer_hex,
                recipient_pubkey: hex::encode(recipient),
                amount: 1_000,
                timestamp: NOTE_TIMESTAMP,
                emergency: false,

                denomination: String::new(),
            }),
        )
        .await;

        assert_ne!(
            status,
            StatusCode::CONFLICT,
            "a single reserve must not trip the fail-closed check"
        );
    }

    #[tokio::test]
    async fn build_redemption_conflicts_when_issuer_has_two_reserves() {
        let state = mock_state(60).await;
        let (issuer, recipient) = add_note(&state).await;
        let issuer_hex = hex::encode(issuer);
        add_reserve(&state, &issuer_hex, "box_a").await;
        add_reserve(&state, &issuer_hex, "box_b").await;

        let (status, body) = build_redemption(
            axum::extract::State(state),
            axum::extract::Json(build_request(&issuer_hex, &hex::encode(recipient))),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "body: {:?}", body);
        assert!(!body.success);
    }

    #[tokio::test]
    async fn build_redemption_single_reserve_passes_fail_closed_check() {
        let state = mock_state(60).await;
        let (issuer, recipient) = add_note(&state).await;
        let issuer_hex = hex::encode(issuer);
        add_reserve(&state, &issuer_hex, "box_a").await;

        let (status, _body) = build_redemption(
            axum::extract::State(state),
            axum::extract::Json(build_request(&issuer_hex, &hex::encode(recipient))),
        )
        .await;

        assert_ne!(
            status,
            StatusCode::CONFLICT,
            "a single reserve must not trip the fail-closed check"
        );
    }

    // ------------------------------------------------------------------
    // M1: per-pair rate limit on /redemption/build
    // ------------------------------------------------------------------

    fn build_request(issuer_hex: &str, recipient_hex: &str) -> RedemptionBuildRequest {
        RedemptionBuildRequest {
            issuer_pubkey: issuer_hex.to_string(),
            recipient_pubkey: recipient_hex.to_string(),
            amount: 1_000,
            timestamp: NOTE_TIMESTAMP,
            issuer_signature: "00".repeat(65),
            emergency: false,
            tracker_box_id: None,
            change_address: None,
            denomination: String::new(),
        }
    }

    #[tokio::test]
    async fn build_redemption_rate_limits_per_pair() {
        let state = mock_state(2).await;
        let (_, issuer) = generate_keypair();
        let (_, recipient) = generate_keypair();
        let (_, other_recipient) = generate_keypair();
        let issuer_hex = hex::encode(issuer);
        let recipient_hex = hex::encode(recipient);

        // No note exists for the pair, so builds that pass the rate limiter fail with 400
        // ("note not found"); the limit+1-th build must instead fail with 429.
        for attempt in 0..2 {
            let (status, body) = build_redemption(
                axum::extract::State(state.clone()),
                axum::extract::Json(build_request(&issuer_hex, &recipient_hex)),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "attempt {attempt} should pass the rate limiter, body: {:?}",
                body
            );
        }

        let (status, body) = build_redemption(
            axum::extract::State(state.clone()),
            axum::extract::Json(build_request(&issuer_hex, &recipient_hex)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "limit+1 builds must be rate limited, body: {:?}",
            body
        );

        // A different (issuer, recipient) pair is unaffected.
        let (status, _) = build_redemption(
            axum::extract::State(state),
            axum::extract::Json(build_request(&issuer_hex, &hex::encode(other_recipient))),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
