use axum::{
    routing::{get, post},
    Router,
};
use basis_server::{
    api::*, build_redemption, reserve_api::*, store::EventStore, submit_redemption, AppConfig,
    AppState, ErgoConfig, EventType, PublicationLease, ServerConfig, SharedTrackerState,
    TrackerBoxUpdateConfig, TrackerBoxUpdater, TrackerCommand, TrackerEvent, TransactionConfig,
};
use basis_store::{
    ergo_scanner::{start_scanner, NodeConfig, ReserveEvent, ServerState},
    tracker_scanner::{create_tracker_server_state, TrackerNodeConfig},
    ReserveTracker,
};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn reject_while_publication_is_fenced(command: TrackerCommand) {
    use basis_store::NoteError;

    match command {
        TrackerCommand::AddNote { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetNotesByIssuer { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetProjectedIssuerGrossDebt { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetNotesByRecipient { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetNotesByRecipientWithIssuer { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetNoteByIssuerAndRecipient { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetNotes { response_tx } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GenerateProof { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetTrackerLookupProof { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetReserveLookupProof { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetReserveInsertProof { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetReserveStateDigest { response_tx } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetValidatedState { response_tx } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetConfirmation { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::GetAllConfirmations { response_tx } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::BeginPublication { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationInProgress));
        }
        TrackerCommand::RecordPublicationAttempt { response_tx, .. }
        | TrackerCommand::ConfirmPublication { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationLeaseMismatch));
        }
        TrackerCommand::AbortPublication { response_tx, .. } => {
            let _ = response_tx.send(Err(NoteError::PublicationLeaseMismatch));
        }
    }
}

#[tokio::main]
async fn main() {
    tracing::info!("Starting basis server...");
    // Load configuration
    tracing::info!("Loading configuration...");
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!("Failed to load configuration: {}", e);
            tracing::info!("Using default configuration...");
            AppConfig::load().unwrap_or_else(|_| {
                // Fallback to hardcoded defaults if config loading fails completely
                AppConfig {
                    server: ServerConfig {
                        host: "0.0.0.0".to_string(),
                        port: 3048,
                        data_dir: Some("data".to_string()),
                        database_url: Some("sqlite:data/basis.db".to_string()),
                    },
                    ergo: ErgoConfig {
                        node: NodeConfig {
                            start_height: None,
                            reserve_contract_p2s: None,
                            node_url: "http://127.0.0.1:9053".to_string(),
                            scan_name: Some("Basis Reserve Scanner".to_string()),
                            api_key: None,
                        },
                        basis_reserve_contract_p2s: "3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT".to_string(),
                        tracker_nft_id: None,
                        allow_fresh_tracker_generation: false,
                        tracker_public_key: None,
                        tracker_secret_key: None,
                    },
                    transaction: TransactionConfig {
                        fee: 1000000, // 0.001 ERG
                        change_address: None, // Will be derived from tracker public key
                    },
                    acceptance: basis_server::acceptance::config::AcceptanceConfig::empty(),
                }
            })
        }
    };

    // Validate that tracker NFT ID is exactly the token id bound to persistent state.
    let tracker_nft_bytes: [u8; 32] = match config
        .tracker_nft_bytes()
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
    {
        Some(bytes) => bytes,
        None => {
            tracing::error!("Tracker NFT ID must be exactly 32 bytes of hex");
            std::process::exit(1);
        }
    };

    tracing::info!("Configuration loaded successfully");

    // Resolve the configured data directory once.
    let data_dir = config.server.data_dir();
    tracing::info!("Using data directory: {:?}", data_dir);

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| {
                "basis_server=debug,basis_store=debug,tower_http=debug,axum=debug".into()
            }),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize real Ergo scanner with blockchain monitoring
    tracing::info!("Initializing Ergo scanner with blockchain monitoring...");

    // Create scanner configuration with actual reserve contract P2S
    let mut scanner_config = config.ergo.node.clone();
    scanner_config.reserve_contract_p2s = Some(config.ergo.basis_reserve_contract_p2s.clone());

    // Create real scanner state with configured node URL and contract template
    let ergo_scanner = match ServerState::new(scanner_config, &data_dir) {
        Ok(scanner) => scanner,
        Err(e) => {
            tracing::warn!("Failed to create Ergo scanner: {}", e);
            tracing::info!("Continuing without blockchain scanner...");
            // Create a minimal scanner that won't actually scan
            let minimal_config = NodeConfig {
                node_url: "http://127.0.0.1:9053".to_string(), // Dummy URL that won't be used
                ..Default::default()
            };
            ServerState::new(minimal_config, &data_dir)
                .unwrap_or_else(|_| panic!("Failed to create minimal scanner"))
        }
    };

    // Start the scanner background task
    if let Err(e) = start_scanner(ergo_scanner.clone()).await {
        tracing::warn!("Failed to start background scanner: {}", e);
        tracing::info!("Continuing without background scanner...");
    } else {
        tracing::info!("Ergo scanner started successfully");
    }

    // Get tracker public key from config early, needed for shared state
    let tracker_pubkey = if let Some(tracker_pubkey_bytes) = match config.tracker_public_key_bytes()
    {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Invalid tracker public key format: {}. Please set 'ergo.tracker_public_key' as either a hex-encoded public key or a P2PK address in your configuration file.", e);
            std::process::exit(1);
        }
    } {
        tracing::info!("Using tracker public key from configuration");
        tracker_pubkey_bytes
    } else {
        tracing::error!("No tracker public key found in configuration. Please set 'ergo.tracker_public_key' as either a hex-encoded public key or a P2PK address in your configuration file.");
        std::process::exit(1);
    };

    // Create shared tracker state for the updater (before scanner so scanner can set box ID)
    tracing::info!("Initializing shared tracker state...");
    let shared_tracker_state_for_updater = SharedTrackerState::new_with_tracker_key(tracker_pubkey);

    // Initialize tracker scanner for monitoring tracker state commitment boxes
    tracing::debug!(
        "Tracker NFT ID from config: {:?}",
        config.ergo.tracker_nft_id
    );
    let _tracker_scanner_initialized = if config.ergo.tracker_nft_id.is_some()
        && config
            .ergo
            .tracker_nft_id
            .as_ref()
            .map_or(false, |id| !id.is_empty())
    {
        tracing::info!("Initializing tracker scanner with tracker NFT ID...");
        let tracker_scanner_config = TrackerNodeConfig {
            start_height: config.ergo.node.start_height,
            tracker_nft_id: config.ergo.tracker_nft_id.clone(),
            node_url: config.ergo.node.node_url.clone(),
            scan_name: Some("Basis Tracker Scanner".to_string()),
            api_key: config.ergo.node.api_key.clone(),
        };

        // Create tracker scanner state with persistent storage paths (similar to reserve scanner)
        let metadata_storage_path = data_dir.join("tracker_scanner_metadata");
        let tracker_storage_path = data_dir.join("tracker_boxes");

        // Ensure data directory exists
        std::fs::create_dir_all(&metadata_storage_path.parent().unwrap_or(&data_dir))
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to create data directory: {}", e);
            });

        match basis_store::persistence::ScannerMetadataStorage::open(metadata_storage_path.clone())
        {
            Ok(metadata_storage) => {
                match basis_store::persistence::TrackerStorage::open(tracker_storage_path.clone()) {
                    Ok(tracker_storage) => {
                        let tracker_scanner = create_tracker_server_state(
                            tracker_scanner_config,
                            metadata_storage,
                            tracker_storage,
                        );

                        // Process tracker boxes directly, no scan registration required
                        match tracker_scanner.process_tracker_boxes().await {
                            Ok(tracker_boxes) => {
                                tracing::info!("Processed {} tracker boxes", tracker_boxes.len());
                                if let Err(e) =
                                    tracker_scanner.update_tracker_state(&tracker_boxes).await
                                {
                                    tracing::error!("Failed to update tracker state: {}", e);
                                }

                                // Set the latest tracker box ID in shared state for the updater
                                if let Some(latest_box) =
                                    tracker_boxes.iter().max_by_key(|b| b.last_verified_height)
                                {
                                    tracing::info!(
                                        "Setting latest tracker box ID in shared state: {}",
                                        latest_box.box_id
                                    );
                                    shared_tracker_state_for_updater
                                        .set_tracker_box_id(latest_box.box_id.clone());
                                }

                                tracing::info!(
                                    "Tracker scanner initialization completed successfully"
                                );
                                true
                            }
                            Err(e) => {
                                tracing::warn!("Failed to process tracker boxes: {:?}", e);
                                tracing::info!("Continuing without tracker scanner...");
                                false
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create tracker storage for tracker scanner: {:?}",
                            e
                        );
                        tracing::info!("Continuing without tracker scanner...");
                        false
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create metadata storage for tracker scanner: {:?}",
                    e
                );
                tracing::info!("Continuing without tracker scanner...");
                false
            }
        }
    } else {
        tracing::info!("Tracker NFT ID not configured, skipping tracker scanner initialization");
        tracing::info!(
            "To enable tracker scanner, configure 'ergo.tracker_nft_id' in your configuration"
        );
        false
    };

    // Initialize reserve tracker
    tracing::info!("Initializing reserve tracker...");
    let _reserve_tracker = ReserveTracker::new();
    tracing::info!("Reserve tracker initialized successfully");

    // Create channel for communicating with tracker thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);

    let generation = basis_store::TrackerGenerationConfig {
        tracker_nft_id: tracker_nft_bytes,
        fresh_generation: if config.ergo.allow_fresh_tracker_generation {
            basis_store::FreshGenerationApproval::Approve
        } else {
            basis_store::FreshGenerationApproval::Deny
        },
    };

    // Open and validate state in its owning blocking thread, then wait for the
    // startup result before exposing any API or publisher task.
    let shared_state_for_tracker = shared_tracker_state_for_updater.clone(); // Also pass shared state for updater
    let data_dir_for_tracker_thread = data_dir.clone();
    let (init_tx, init_rx) = tokio::sync::oneshot::channel();

    // Spawn tracker thread (using tokio::task::spawn_blocking for CPU-bound work)
    tokio::task::spawn_blocking(move || {
        use basis_store::RedemptionManager;

        tracing::debug!("Tracker thread started");
        let tracker = match basis_store::TrackerStateManager::try_new_with_publication_health(
            &data_dir_for_tracker_thread,
            generation,
            shared_state_for_tracker.publication_health(),
        ) {
            Ok(tracker) => tracker,
            Err(error) => {
                shared_state_for_tracker.quarantine_publication();
                let _ = init_tx.send(Err(format!("{error:?}")));
                return;
            }
        };
        let initial_root = match tracker.validated_state() {
            Ok(state) => state.avl_root_digest,
            Err(error) => {
                shared_state_for_tracker.quarantine_publication();
                let _ = init_tx.send(Err(format!("{error:?}")));
                return;
            }
        };
        let initial_pending = match tracker.pending_publication() {
            Ok(pending) => pending,
            Err(error) => {
                shared_state_for_tracker.quarantine_publication();
                let _ = init_tx.send(Err(format!("{error:?}")));
                return;
            }
        };
        let _ = init_tx.send(Ok((initial_root, initial_pending.clone())));
        tracing::info!(
            "Tracker thread initialized with AVL root digest: {}",
            hex::encode(&initial_root)
        );

        let mut redemption_manager = RedemptionManager::new(tracker);
        let mut active_publication: Option<PublicationLease> =
            initial_pending.as_ref().map(|pending| PublicationLease {
                id: 0,
                digest: pending.digest(),
            });
        let mut next_publication_id = 1u64;

        while let Some(cmd) = rx.blocking_recv() {
            tracing::debug!("Tracker thread received command: {:?}", cmd);

            if let Some(active_lease) = active_publication {
                match cmd {
                    TrackerCommand::RecordPublicationAttempt {
                        lease,
                        tx_id,
                        submitted_height,
                        response_tx,
                    } if lease == active_lease => {
                        let result =
                            redemption_manager
                                .tracker
                                .validated_state()
                                .and_then(|state| {
                                    if state.avl_root_digest != lease.digest {
                                        return Err(
                                            basis_store::NoteError::PublicationLeaseMismatch,
                                        );
                                    }
                                    redemption_manager.tracker.mark_notes_pending(
                                        lease.digest,
                                        &tx_id,
                                        submitted_height,
                                    )
                                });
                        if result.is_err() {
                            shared_state_for_tracker.quarantine_publication();
                        }
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::ConfirmPublication {
                        tx_id,
                        box_id,
                        height,
                        response_tx,
                    } => {
                        let result = redemption_manager
                            .tracker
                            .confirm_pending_publication(&tx_id, &box_id, height);
                        if result.is_ok() {
                            active_publication = None;
                        } else {
                            shared_state_for_tracker.quarantine_publication();
                        }
                        let _ = response_tx.send(result);
                    }
                    TrackerCommand::AbortPublication { lease, response_tx }
                        if lease == active_lease =>
                    {
                        let result =
                            redemption_manager
                                .tracker
                                .pending_publication()
                                .and_then(|pending| {
                                    if pending.is_some() {
                                        Err(basis_store::NoteError::PublicationInProgress)
                                    } else {
                                        Ok(())
                                    }
                                });
                        if result.is_ok() {
                            active_publication = None;
                        }
                        let _ = response_tx.send(result);
                    }
                    other => reject_while_publication_is_fenced(other),
                }
                continue;
            }

            match cmd {
                TrackerCommand::AddNote {
                    issuer_pubkey,
                    note,
                    response_tx,
                } => {
                    // Get mutable access to the tracker for adding a note
                    let result = redemption_manager.tracker.add_note(&issuer_pubkey, &note);

                    // Update shared state for tracker box updater if successful
                    if result.is_ok() {
                        // Update the shared AVL root digest to match the current tracker state
                        // Note: In a real implementation, we'd send this back to the async context to store
                        // For now, we'll handle event storage in the async handler
                    }

                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNotesByIssuer {
                    issuer_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager.tracker.get_issuer_notes(&issuer_pubkey);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetProjectedIssuerGrossDebt {
                    issuer_pubkey,
                    candidate_recipient,
                    candidate_total_debt,
                    response_tx,
                } => {
                    let result = redemption_manager.tracker.projected_issuer_gross_debt(
                        &issuer_pubkey,
                        candidate_recipient.as_ref(),
                        candidate_total_debt,
                    );
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
                TrackerCommand::GetNotesByRecipientWithIssuer {
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager
                        .tracker
                        .get_recipient_notes_with_issuer(&recipient_pubkey);
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
                TrackerCommand::GetNotes { response_tx } => {
                    let result = redemption_manager.tracker.get_all_notes_with_issuer();
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GenerateProof {
                    issuer_pubkey,
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager
                        .tracker
                        .generate_proof(&issuer_pubkey, &recipient_pubkey)
                        .and_then(|proof| {
                            redemption_manager
                                .tracker
                                .validated_state()
                                .map(|state| (proof, state))
                        });
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetTrackerLookupProof {
                    issuer_pubkey,
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager
                        .tracker
                        .generate_tracker_lookup_proof(&issuer_pubkey, &recipient_pubkey)
                        .and_then(|proof| {
                            redemption_manager
                                .tracker
                                .validated_state()
                                .map(|state| (proof, state))
                        });
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetReserveLookupProof {
                    issuer_pubkey,
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager
                        .tracker
                        .generate_reserve_lookup_proof(&issuer_pubkey, &recipient_pubkey)
                        .and_then(|proof| {
                            redemption_manager
                                .tracker
                                .reserve_state_digest()
                                .map(|root| (proof, root))
                        });
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetReserveInsertProof {
                    issuer_pubkey,
                    recipient_pubkey,
                    timestamp,
                    new_already_redeemed,
                    response_tx,
                } => {
                    let result = redemption_manager
                        .tracker
                        .generate_reserve_insert_proof(
                            &issuer_pubkey,
                            &recipient_pubkey,
                            timestamp,
                            new_already_redeemed,
                        )
                        .and_then(|(proof, updated_root)| {
                            redemption_manager
                                .tracker
                                .reserve_state_digest()
                                .map(|current_root| (proof, updated_root, current_root))
                        });
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetReserveStateDigest { response_tx } => {
                    let digest = redemption_manager.tracker.reserve_state_digest();
                    let _ = response_tx.send(digest);
                }
                TrackerCommand::GetValidatedState { response_tx } => {
                    let _ = response_tx.send(redemption_manager.tracker.validated_state());
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
                    let result = redemption_manager
                        .tracker
                        .validated_state()
                        .map(|_| redemption_manager.tracker.all_confirmations());
                    let _ = response_tx.send(result);
                }
                TrackerCommand::BeginPublication {
                    tracker_nft_id,
                    observed_root,
                    box_id,
                    height,
                    response_tx,
                } => {
                    let result = redemption_manager
                        .tracker
                        .validate_observed_generation(&tracker_nft_id, observed_root)
                        .and_then(|_| {
                            redemption_manager.tracker.reconcile_with_confirmed_digest(
                                &observed_root,
                                &box_id,
                                height,
                            )?;
                            redemption_manager.tracker.validated_state()
                        })
                        .and_then(|state| {
                            next_publication_id
                                .checked_add(1)
                                .ok_or(basis_store::NoteError::PublicationLeaseMismatch)?;
                            Ok(PublicationLease {
                                id: next_publication_id,
                                digest: state.avl_root_digest,
                            })
                        });

                    match result {
                        Ok(lease) => {
                            if response_tx.send(Ok(lease)).is_ok() {
                                active_publication = Some(lease);
                                next_publication_id += 1;
                            }
                        }
                        Err(error) => {
                            shared_state_for_tracker.quarantine_publication();
                            let _ = response_tx.send(Err(error));
                        }
                    }
                }
                TrackerCommand::RecordPublicationAttempt { response_tx, .. }
                | TrackerCommand::ConfirmPublication { response_tx, .. } => {
                    let _ = response_tx.send(Err(basis_store::NoteError::PublicationLeaseMismatch));
                }
                TrackerCommand::AbortPublication { response_tx, .. } => {
                    let _ = response_tx.send(Err(basis_store::NoteError::PublicationLeaseMismatch));
                }
            }
        }
    });

    match init_rx.await {
        Ok(Ok((_root, pending))) => {
            if let Some(pending) = pending {
                shared_tracker_state_for_updater.set_pending(
                    pending.digest(),
                    pending.tx_id().to_string(),
                    pending.submitted_height(),
                );
            }
        }
        Ok(Err(error)) => {
            shared_tracker_state_for_updater.quarantine_publication();
            tracing::error!(error, "Tracker state initialization failed closed");
            std::process::exit(1);
        }
        Err(_) => {
            shared_tracker_state_for_updater.quarantine_publication();
            tracing::error!("Tracker state thread ended during initialization");
            std::process::exit(1);
        }
    }

    // Create tracker box updater
    tracing::info!("Initializing tracker box updater...");

    // Check if node configuration is provided, abort if not
    if config.ergo.node.node_url.is_empty() {
        tracing::error!("No Ergo node URL provided in configuration. Tracker box updater requires node connection.");
        std::process::exit(1);
    }

    // Initialize tracker NFT ID in shared state if configured
    if let Some(ref tracker_nft_id) = config.ergo.tracker_nft_id {
        shared_tracker_state_for_updater.set_tracker_nft_id(tracker_nft_id.clone());
        tracing::info!("Tracker NFT ID initialized: {}", tracker_nft_id);
    } else {
        tracing::warn!(
            "No tracker NFT ID configured. Tracker box updater will be disabled until configured."
        );
    }

    // Use mainnet network prefix for address encoding
    let _network_prefix = ergo_lib::ergotree_ir::chain::address::NetworkPrefix::Mainnet;

    let tracker_box_config = TrackerBoxUpdateConfig {
        node_url: config.ergo.node.node_url.clone(),
        api_key: config.ergo.node.api_key.clone(),
        update_interval_seconds: 600, // 10 minutes
        fee: config.transaction.fee,
        change_address: config.get_change_address().ok(),
        tracker_secret_key: config.tracker_secret_key_bytes(),
    };
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Clone the channel for the tracker updater
    let _updater_shutdown_rx = shutdown_tx.subscribe();

    // Start the tracker box updater in the background
    let updater_config = tracker_box_config.clone();
    let shared_state_clone = shared_tracker_state_for_updater.clone();
    let updater_cmd_tx = tx.clone();
    let updater_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        if let Err(e) = TrackerBoxUpdater::start(
            updater_config,
            shared_state_clone,
            updater_shutdown_rx,
            Some(updater_cmd_tx),
        )
        .await
        {
            tracing::error!("Tracker box updater failed: {}", e);
        }
    });
    tracing::info!("Tracker box updater started successfully");

    let event_store = match EventStore::new().await {
        Ok(store) => std::sync::Arc::new(store),
        Err(e) => {
            tracing::error!("Failed to initialize event store: {:?}", e);
            std::process::exit(1);
        }
    };

    // Add demo events
    let demo_events = vec![
        TrackerEvent {
            id: 0,
            event_type: EventType::NoteUpdated,
            timestamp: 1234567890,
            issuer_pubkey: Some(
                "010101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            recipient_pubkey: Some(
                "020202020202020202020202020202020202020202020202020202020202020202".to_string(),
            ),
            amount: Some(1000),
            reserve_box_id: None,
            collateral_amount: None,
            redeemed_amount: None,
            height: None,
        },
        TrackerEvent {
            id: 0,
            event_type: EventType::NoteUpdated,
            timestamp: 1234567891,
            issuer_pubkey: Some(
                "010101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            recipient_pubkey: Some(
                "030303030303030303030303030303030303030303030303030303030303030303".to_string(),
            ),
            amount: Some(2000),
            reserve_box_id: None,
            collateral_amount: None,
            redeemed_amount: None,
            height: None,
        },
        TrackerEvent {
            id: 0,
            event_type: EventType::ReserveCreated,
            timestamp: 1234567892,
            issuer_pubkey: Some(
                "010101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: Some("box1234567890abcdef".to_string()),
            collateral_amount: Some(1000000000),
            redeemed_amount: None,
            height: Some(1000),
        },
        TrackerEvent {
            id: 0,
            event_type: EventType::ReserveToppedUp,
            timestamp: 1234567893,
            issuer_pubkey: Some(
                "010101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: Some("box1234567890abcdef".to_string()),
            collateral_amount: Some(500000000),
            redeemed_amount: None,
            height: Some(1001),
        },
        TrackerEvent {
            id: 0,
            event_type: EventType::ReserveRedeemed,
            timestamp: 1234567894,
            issuer_pubkey: Some(
                "010101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: Some("box1234567890abcdef".to_string()),
            collateral_amount: None,
            redeemed_amount: Some(250000000),
            height: Some(1002),
        },
        TrackerEvent {
            id: 0,
            event_type: EventType::Commitment,
            timestamp: 1234567895,
            issuer_pubkey: None,
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: None,
            collateral_amount: None,
            redeemed_amount: None,
            height: Some(1003),
        },
        TrackerEvent {
            id: 0,
            event_type: EventType::CollateralAlert { ratio: 0.8 },
            timestamp: 1234567896,
            issuer_pubkey: Some(
                "010101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: None,
            collateral_amount: None,
            redeemed_amount: None,
            height: None,
        },
    ];

    for event in demo_events {
        if let Err(e) = event_store.add_event(event).await {
            tracing::warn!("Failed to add demo event: {:?}", e);
        }
    }

    // Extract the reserve tracker from the scanner before wrapping in Arc/Mutex
    let scanner_reserve_tracker = ergo_scanner.reserve_tracker.clone();

    // Initialize tracker storage for the new API endpoint
    let tracker_storage_path = data_dir.join("tracker_boxes");
    let tracker_storage = match basis_store::persistence::TrackerStorage::open(tracker_storage_path)
    {
        Ok(storage) => storage,
        Err(e) => {
            tracing::error!("Failed to initialize tracker storage: {:?}", e);
            std::process::exit(1);
        }
    };

    // Build acceptance predicate from configuration
    let acceptance_predicate =
        match basis_server::acceptance::builder::build_predicate_tree(config.acceptance.clone()) {
            Ok(Some(pred)) => {
                tracing::info!("Acceptance predicate loaded: '{}'", pred.name());
                Some(std::sync::Arc::from(pred))
            }
            Ok(None) => {
                tracing::info!("No acceptance predicates configured");
                None
            }
            Err(e) => {
                tracing::warn!("Failed to build acceptance predicate: {}", e);
                None
            }
        };

    // Initialize policy storage for per-recipient acceptance policies
    let policy_storage_path = data_dir.join("acceptance_policies");
    let policy_storage =
        match basis_store::persistence::AcceptancePolicyStorage::open(policy_storage_path) {
            Ok(storage) => {
                tracing::info!("Acceptance policy storage initialized successfully");
                storage
            }
            Err(e) => {
                tracing::error!("Failed to initialize acceptance policy storage: {:?}", e);
                std::process::exit(1);
            }
        };

    let app_state = AppState {
        tx,
        event_store,
        ergo_scanner: std::sync::Arc::new(Mutex::new(ergo_scanner)),
        reserve_tracker: std::sync::Arc::new(Mutex::new(scanner_reserve_tracker)),
        config: std::sync::Arc::new(config.clone()),
        shared_tracker_state: std::sync::Arc::new(tokio::sync::Mutex::new(
            shared_tracker_state_for_updater,
        )),
        tracker_storage,
        acceptance_predicate,
        policy_storage,
    };

    // Build our application with routes - FIXED ROUTE ORDER
    let app = Router::new()
        // Root route
        .route("/", get(root))
        // Static routes
        .route("/events", get(get_events))
        .route("/events/paginated", get(get_events_paginated))
        .route("/notes", post(create_note).options(handle_options))
        .route(
            "/acceptance/check",
            post(check_acceptance).options(handle_options),
        )
        .route(
            "/acceptance/policy",
            post(upload_policy).options(handle_options),
        )
        .route(
            "/acceptance/policy/{pubkey}",
            get(get_policy_by_recipient).options(handle_options),
        )
        .route("/redeem", post(initiate_redemption).options(handle_options))
        .route(
            "/redeem/complete",
            post(complete_redemption).options(handle_options),
        )
        .route("/proof/redemption", get(get_redemption_proof))
        .route("/tracker/proof", get(get_tracker_proof))
        .route("/tracker/state", get(get_tracker_state))
        .route("/tracker/pending-tx", get(get_pending_tx))
        .route("/reserve/proof", get(get_reserve_proof))
        .route(
            "/tracker/signature",
            post(request_tracker_signature).options(handle_options),
        )
        .route(
            "/redemption/prepare",
            post(prepare_redemption).options(handle_options),
        )
        .route(
            "/redemption/build",
            post(build_redemption).options(handle_options),
        )
        .route(
            "/redemption/submit",
            post(submit_redemption).options(handle_options),
        )
        .route("/reserves", get(get_all_reserves))
        .route(
            "/reserves/create",
            post(create_reserve_payload).options(handle_options),
        )
        .route(
            "/reserves/submit",
            post(submit_reserve_transaction).options(handle_options),
        )
        // Most specific parameterized routes first
        .route(
            "/notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}",
            get(get_note_by_issuer_and_recipient),
        )
        // Parameterized routes
        .route("/notes/issuer/{pubkey}", get(get_notes_by_issuer))
        .route("/notes/recipient/{pubkey}", get(get_notes_by_recipient))
        .route("/notes", get(get_all_notes)) // Get all notes with age
        .route("/notes/state", post(get_note_state).options(handle_options))
        .route("/reserves/{box_id}", get(get_reserve_by_box_id))
        .route("/reserves/issuer/{pubkey}", get(get_reserves_by_issuer))
        .route("/key-status/{pubkey}", get(get_key_status))
        .route("/tracker/latest-box-id", get(get_latest_tracker_box_id))
        .route(
            "/config/reserve-contract-p2s",
            get(get_basis_reserve_contract_p2s),
        )
        .with_state(app_state.clone())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    tracing::debug!("Router built successfully");
    tracing::debug!("Registered routes:");
    tracing::debug!("  GET /");
    tracing::debug!("  POST /notes");
    tracing::debug!("  GET /notes/issuer/{{pubkey}}");
    tracing::debug!("  GET /notes/recipient/{{pubkey}}");
    tracing::debug!("  GET /notes/issuer/{{issuer_pubkey}}/recipient/{{recipient_pubkey}}");
    tracing::debug!("  GET /notes (all notes with age)");
    tracing::debug!("  POST /notes/state");
    tracing::debug!("  GET /reserves");
    tracing::debug!("  GET /reserves/{{box_id}}");
    tracing::debug!("  GET /reserves/issuer/{{pubkey}}");
    tracing::debug!("  POST /reserves/create");
    tracing::debug!("  GET /events");
    tracing::debug!("  GET /events/paginated");
    tracing::debug!("  GET /key-status/{{pubkey}}");
    tracing::debug!("  POST /redeem");
    tracing::debug!("  POST /acceptance/check");
    tracing::debug!("  POST /acceptance/policy");
    tracing::debug!("  GET /tracker/latest-box-id");
    tracing::debug!("  GET /tracker/state");
    tracing::debug!("  GET /tracker/pending-tx");

    // Run our app with hyper
    let addr = config.socket_addr();
    tracing::debug!("listening on {}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            tracing::info!("Server listening on {}", addr);
            listener
        }
        Err(e) => {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    // Scanner is already started via start_scanner() above
    // No need for duplicate background scanner task

    tracing::info!("Starting axum server...");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    };
}

#[cfg(test)]
mod publication_fence_tests {
    use super::*;

    #[tokio::test]
    async fn active_publication_rejects_state_mutation_and_root_exposure() {
        let (add_tx, add_rx) = tokio::sync::oneshot::channel();
        reject_while_publication_is_fenced(TrackerCommand::AddNote {
            issuer_pubkey: [2u8; 33],
            note: basis_store::IouNote {
                recipient_pubkey: [3u8; 33],
                amount_collected: 1,
                amount_redeemed: 0,
                timestamp: 1,
                signature: [0u8; 65],
            },
            response_tx: add_tx,
        });
        assert!(matches!(
            add_rx.await,
            Ok(Err(basis_store::NoteError::PublicationInProgress))
        ));

        let (state_tx, state_rx) = tokio::sync::oneshot::channel();
        reject_while_publication_is_fenced(TrackerCommand::GetValidatedState {
            response_tx: state_tx,
        });
        assert!(matches!(
            state_rx.await,
            Ok(Err(basis_store::NoteError::PublicationInProgress))
        ));

        let (proof_tx, proof_rx) = tokio::sync::oneshot::channel();
        reject_while_publication_is_fenced(TrackerCommand::GenerateProof {
            issuer_pubkey: [2u8; 33],
            recipient_pubkey: [3u8; 33],
            response_tx: proof_tx,
        });
        assert!(matches!(
            proof_rx.await,
            Ok(Err(basis_store::NoteError::PublicationInProgress))
        ));
    }

    #[tokio::test]
    async fn stale_publication_receipt_cannot_release_the_actor_fence() {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        reject_while_publication_is_fenced(TrackerCommand::AbortPublication {
            lease: PublicationLease {
                id: 7,
                digest: [7u8; 33],
            },
            response_tx,
        });
        assert!(matches!(
            response_rx.await,
            Ok(Err(basis_store::NoteError::PublicationLeaseMismatch))
        ));
    }
}

/// Background task that continuously scans the blockchain for reserve events
#[allow(dead_code)]
async fn background_scanner_task(state: AppState, config: AppConfig) {
    tracing::info!("Starting background blockchain scanner task");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await; // Scan every 30 seconds

        let mut scanner = match state.ergo_scanner.try_lock() {
            Ok(scanner) => scanner,
            Err(_) => {
                tracing::debug!("Scanner is busy, skipping this scan cycle");
                continue;
            }
        };

        // Check if scanner is active
        if !scanner.is_active().await {
            tracing::warn!("Scanner is not active, attempting to restart...");
            match scanner.start_scanning().await {
                Ok(()) => {
                    tracing::info!("Scanner restarted successfully");
                }
                Err(e) => {
                    tracing::error!("Failed to restart scanner: {}", e);
                    continue;
                }
            }
        }

        // Update reserve tracker with current unspent boxes
        match scanner.get_unspent_reserve_boxes().await {
            Ok(boxes) => {
                let tracker = match state.reserve_tracker.try_lock() {
                    Ok(tracker) => tracker,
                    Err(_) => {
                        tracing::debug!("Reserve tracker is busy, skipping update");
                        continue;
                    }
                };

                for scan_box in &boxes {
                    // Extract owner pubkey from box registers (R4 register)
                    let owner_pubkey = match scan_box.additional_registers.get("R4") {
                        Some(pubkey_hex) => {
                            // Parse hex-encoded public key from register
                            match hex::decode(pubkey_hex) {
                                Ok(bytes) => bytes,
                                Err(e) => {
                                    tracing::warn!(
                                        "Invalid R4 register hex for box {}: {}",
                                        scan_box.box_id,
                                        e
                                    );
                                    continue; // Skip this box
                                }
                            }
                        }
                        None => {
                            tracing::warn!("Box {} missing R4 register, skipping", scan_box.box_id);
                            continue; // Skip boxes without owner pubkey
                        }
                    };

                    let tracker_nft_bytes_option = match config.tracker_nft_bytes() {
                        Ok(bytes) => Some(bytes),
                        Err(_) => {
                            tracing::error!("Tracker NFT ID is not properly configured");
                            continue; // Skip this box update
                        }
                    };

                    let refund_initiation_height =
                        basis_store::ergo_scanner::decode_ergo_long_register(
                            scan_box.additional_registers.get("R7"),
                        );

                    let mut reserve_info = basis_store::ExtendedReserveInfo::new(
                        scan_box.box_id.as_bytes(),
                        &owner_pubkey,
                        scan_box.value,
                        tracker_nft_bytes_option.as_deref(),
                        scanner.last_scanned_height().await,
                        refund_initiation_height,
                    );

                    // Set contract address from configuration
                    reserve_info
                        .set_contract_address(config.basis_reserve_contract_p2s().to_string());

                    if let Err(e) = tracker.update_reserve(reserve_info) {
                        tracing::warn!(
                            "Failed to update reserve info for {}: {}",
                            scan_box.box_id,
                            e
                        );
                    }
                }

                tracing::debug!("Updated reserve tracker with {} unspent boxes", boxes.len());
            }
            Err(e) => {
                tracing::error!("Failed to get unspent reserve boxes: {}", e);
            }
        }
    }
}

/// Handle OPTIONS preflight requests for CORS
async fn handle_options() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::OK,
        [("Access-Control-Allow-Origin", "*")],
        "",
    )
}

/// Process a reserve event and store it in the event store
#[allow(dead_code)]
async fn process_reserve_event(
    state: &AppState,
    event: ReserveEvent,
    config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let tracker_event = match event {
        ReserveEvent::ReserveCreated {
            box_id,
            owner_pubkey,
            collateral_amount,
            height,
        } => {
            tracing::info!(
                "Reserve created: {} with {} nanoERG at height {}",
                box_id,
                collateral_amount,
                height
            );

            // Update reserve tracker
            let tracker_nft_bytes_option = match config.tracker_nft_bytes() {
                Ok(bytes) => Some(bytes),
                Err(_) => {
                    tracing::error!("Tracker NFT ID is not properly configured");
                    return Err("Tracker NFT ID is not properly configured".into());
                }
            };

            let tracker = state.reserve_tracker.lock().await;
            // Decode the hex-encoded owner public key to bytes before passing to ExtendedReserveInfo::new
            let owner_pubkey_bytes = match hex::decode(&owner_pubkey) {
                Ok(bytes) => bytes,
                Err(_) => {
                    tracing::error!("Failed to decode owner public key: {}", owner_pubkey);
                    return Err("Invalid owner public key format".into());
                }
            };
            let mut reserve_info = basis_store::ExtendedReserveInfo::new(
                box_id.as_bytes(),
                &owner_pubkey_bytes,
                collateral_amount,
                tracker_nft_bytes_option.as_deref(),
                height,
                0, // Newly created reserves have no pending refund
            );
            reserve_info.set_contract_address(config.basis_reserve_contract_p2s().to_string());
            tracker.update_reserve(reserve_info)?;

            TrackerEvent {
                id: 0,
                event_type: EventType::ReserveCreated,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                issuer_pubkey: Some(owner_pubkey),
                recipient_pubkey: None,
                amount: None,
                reserve_box_id: Some(box_id),
                collateral_amount: Some(collateral_amount),
                redeemed_amount: None,
                height: Some(height),
            }
        }
        ReserveEvent::ReserveToppedUp {
            box_id,
            additional_collateral,
            height,
        } => {
            tracing::info!(
                "Reserve topped up: {} +{} nanoERG at height {}",
                box_id,
                additional_collateral,
                height
            );

            TrackerEvent {
                id: 0,
                event_type: EventType::ReserveToppedUp,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                issuer_pubkey: None, // Will be filled from reserve tracker if needed
                recipient_pubkey: None,
                amount: None,
                reserve_box_id: Some(box_id),
                collateral_amount: Some(additional_collateral),
                redeemed_amount: None,
                height: Some(height),
            }
        }
        ReserveEvent::ReserveRedeemed {
            box_id,
            redeemed_amount,
            height,
        } => {
            tracing::info!(
                "Reserve redeemed: {} -{} nanoERG at height {}",
                box_id,
                redeemed_amount,
                height
            );

            TrackerEvent {
                id: 0,
                event_type: EventType::ReserveRedeemed,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                issuer_pubkey: None, // Will be filled from reserve tracker if needed
                recipient_pubkey: None,
                amount: None,
                reserve_box_id: Some(box_id),
                collateral_amount: None,
                redeemed_amount: Some(redeemed_amount),
                height: Some(height),
            }
        }
        ReserveEvent::ReserveSpent { box_id, height } => {
            tracing::info!("Reserve spent: {} at height {}", box_id, height);

            TrackerEvent {
                id: 0,
                event_type: EventType::ReserveSpent,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                issuer_pubkey: None, // Will be filled from reserve tracker if needed
                recipient_pubkey: None,
                amount: None,
                reserve_box_id: Some(box_id),
                collateral_amount: None,
                redeemed_amount: None,
                height: Some(height),
            }
        }
    };

    // Store the event
    state.event_store.add_event(tracker_event).await?;

    Ok(())
}
