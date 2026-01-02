use axum::{
    routing::{get, post},
    Router,
};
use basis_server::{
    api::*, reserve_api::*, store::EventStore, AppConfig, AppState, ErgoConfig, EventType,
    ServerConfig, TrackerCommand, TrackerEvent, TransactionConfig,
    TrackerBoxUpdateConfig, TrackerBoxUpdater, SharedTrackerState,
};
use basis_store::{
    ergo_scanner::{start_scanner, NodeConfig, ReserveEvent, ServerState},
    tracker_scanner::{create_tracker_server_state, TrackerNodeConfig},
    ReserveTracker,
};
use basis_store::persistence::{TrackerStorage, ScannerMetadataStorage};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
                        database_url: Some("sqlite:data/basis.db".to_string()),
                    },
                    ergo: ErgoConfig {
                        node: NodeConfig {
                            start_height: None,
                            reserve_contract_p2s: None,
                            node_url: "http://159.89.116.15:11088".to_string(),
                            scan_name: Some("Basis Reserve Scanner".to_string()),
                            api_key: Some("hello".to_string()),
                        },
                        basis_reserve_contract_p2s: "W52Uvz86YC7XkV8GXjM9DDkMLHWqZLyZGRi1FbmyppvPy7cREnehzz21DdYTdrsuw268CxW3gkXE6D5B8748FYGg3JEVW9R6VFJe8ZDknCtiPbh56QUCJo5QDizMfXaKnJ3jbWV72baYPCw85tmiJowR2wd4AjsEuhZP4Ry4QRDcZPvGogGVbdk7ykPAB7KN2guYEhS7RU3xm23iY1YaM5TX1ditsWfxqCBsvq3U6X5EU2Y5KCrSjQxdtGcwoZsdPQhfpqcwHPcYqM5iwK33EU1cHqggeSKYtLMW263f1TY7Lfu3cKMkav1CyomR183TLnCfkRHN3vcX2e9fSaTpAhkb74yo6ZRXttHNP23JUASWs9ejCaguzGumwK3SpPCLBZY6jFMYWqeaanH7XAtTuJA6UCnxvrKko5PX1oSB435Bxd3FbvDAsEmHpUqqtP78B7SKxFNPvJeZuaN7r5p8nDLxUPZBrWwz2vtcgWPMq5RrnoJdrdqrnXMcMEQPF5AKDYuKMKbCRgn3HLvG98JXJ4bCc2wzuZhnCRQaFXTy88knEoj".to_string(),
                        tracker_nft_id: None,
                        tracker_public_key: None,
                    },
                    transaction: TransactionConfig {
                        fee: 1000000, // 0.001 ERG
                    },
                }
            })
        }
    };

    // Validate that tracker NFT ID is properly configured
    if let Err(_) = config.tracker_nft_bytes() {
        tracing::error!("Tracker NFT ID is not properly configured in the configuration file. The server requires a valid tracker_nft_id value.");
        std::process::exit(1); // Exit with error code if tracker NFT ID is not configured
    }

    tracing::info!("Configuration loaded successfully");
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "basis_server=debug,basis_store=debug,tower_http=debug,axum=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize real Ergo scanner with blockchain monitoring
    tracing::info!("Initializing Ergo scanner with blockchain monitoring...");

    // Create scanner configuration with actual reserve contract P2S
    let mut scanner_config = config.ergo.node.clone();
    scanner_config.reserve_contract_p2s = Some(config.ergo.basis_reserve_contract_p2s.clone());

    // Create real scanner state with configured node URL and contract template
    let ergo_scanner = match ServerState::new(scanner_config) {
        Ok(scanner) => scanner,
        Err(e) => {
            tracing::warn!("Failed to create Ergo scanner: {}", e);
            tracing::info!("Continuing without blockchain scanner...");
            // Create a minimal scanner that won't actually scan
            let minimal_config = NodeConfig {
                node_url: "http://159.89.116.15:11088".to_string(), // Dummy URL that won't be used
                ..Default::default()
            };
            ServerState::new(minimal_config).unwrap_or_else(|_| panic!("Failed to create minimal scanner"))
        }
    };

    // Start the scanner background task
    if let Err(e) = start_scanner(ergo_scanner.clone()).await {
        tracing::warn!("Failed to start background scanner: {}", e);
        tracing::info!("Continuing without background scanner...");
    } else {
        tracing::info!("Ergo scanner started successfully");
    }

    // Initialize tracker scanner for monitoring tracker state commitment boxes
    tracing::debug!("Tracker NFT ID from config: {:?}", config.ergo.tracker_nft_id);
    if config.ergo.tracker_nft_id.is_some() && config.ergo.tracker_nft_id.as_ref().map_or(false, |id| !id.is_empty()) {
        tracing::info!("Initializing tracker scanner with tracker NFT ID...");
        let tracker_scanner_config = TrackerNodeConfig {
            start_height: config.ergo.node.start_height,
            tracker_nft_id: config.ergo.tracker_nft_id.clone(),
            node_url: config.ergo.node.node_url.clone(),
            scan_name: Some("Basis Tracker Scanner".to_string()),
            api_key: config.ergo.node.api_key.clone(),
        };

        // Create tracker scanner state with persistent storage paths (similar to reserve scanner)
        let metadata_storage_path = std::path::Path::new("data").join("tracker_scanner_metadata");
        let tracker_storage_path = std::path::Path::new("data").join("tracker_boxes");

        // Ensure data directory exists
        std::fs::create_dir_all(&metadata_storage_path.parent().unwrap_or(std::path::Path::new("data"))).unwrap_or_else(|e| {
            tracing::warn!("Failed to create data directory: {}", e);
        });

        match basis_store::persistence::ScannerMetadataStorage::open(metadata_storage_path.clone()) {
            Ok(metadata_storage) => {
                match basis_store::persistence::TrackerStorage::open(tracker_storage_path.clone()) {
                    Ok(tracker_storage) => {
                        let tracker_scanner = create_tracker_server_state(
                            tracker_scanner_config,
                            metadata_storage,
                            tracker_storage,
                        );

                        // Ensure the tracker scan is registered on startup
                        match tracker_scanner.ensure_scan_registered().await {
                            Ok(scan_id) => {
                                tracing::info!("Tracker scan registered with ID: {}", scan_id);
                                tracing::info!("Tracker scanner initialization completed successfully");
                            },
                            Err(e) => {
                                tracing::warn!("Failed to register tracker scan: {:?}", e);
                                tracing::info!("Continuing without tracker scanner registration...");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create tracker storage for tracker scanner: {:?}", e);
                        tracing::info!("Continuing without tracker scanner...");
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create metadata storage for tracker scanner: {:?}", e);
                tracing::info!("Continuing without tracker scanner...");
            }
        }
    } else {
        tracing::info!("Tracker NFT ID not configured, skipping tracker scanner initialization");
        tracing::info!("To enable tracker scanner, configure 'ergo.tracker_nft_id' in your configuration");
    }

    // Initialize reserve tracker
    tracing::info!("Initializing reserve tracker...");
    let reserve_tracker = ReserveTracker::new();
    tracing::info!("Reserve tracker initialized successfully");

    // Create channel for communicating with tracker thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);

    // Initialize tracker manager outside of the blocking task so it can be shared
    use basis_store::{RedemptionManager, TrackerStateManager};
    let shared_tracker_state = std::sync::Arc::new(std::sync::Mutex::new(TrackerStateManager::new()));

    // Create shared tracker state for the updater
    tracing::info!("Initializing shared tracker state...");

    // Get tracker public key from config, exit with error if not provided
    let tracker_pubkey = if let Some(tracker_pubkey_bytes) = match config.tracker_public_key_bytes() {
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

    let shared_tracker_state_for_updater = SharedTrackerState::new_with_tracker_key(tracker_pubkey);

    // Spawn tracker thread (using tokio::task::spawn_blocking for CPU-bound work)
    let shared_tracker_state_clone = shared_tracker_state.clone();
    let shared_state_for_tracker = shared_tracker_state_for_updater.clone(); // Also pass shared state for updater
    tokio::task::spawn_blocking(move || {
        use basis_store::RedemptionManager;

        tracing::debug!("Tracker thread started");
        let mut tracker = TrackerStateManager::new();
        let mut redemption_manager = RedemptionManager::new(tracker);

        while let Some(cmd) = rx.blocking_recv() {
            tracing::debug!("Tracker thread received command: {:?}", cmd);
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
                        let current_root = redemption_manager.tracker.get_state().avl_root_digest;
                        shared_state_for_tracker.set_avl_root_digest(current_root);

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
                TrackerCommand::GetNotesByRecipient {
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager.tracker.get_recipient_notes(&recipient_pubkey);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNoteByIssuerAndRecipient {
                    issuer_pubkey,
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = redemption_manager.tracker
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

                    // Update shared state for tracker box updater if successful
                    if result.is_ok() {
                        // Update the shared AVL root digest to match the current tracker state
                        let current_root = redemption_manager.tracker.get_state().avl_root_digest;
                        shared_state_for_tracker.set_avl_root_digest(current_root);
                    }

                    let _ = response_tx.send(result);
                }
            }
        }
    });

    // Create tracker box updater
    tracing::info!("Initializing tracker box updater...");

    // Check if node configuration is provided, abort if not
    if config.ergo.node.node_url.is_empty() {
        tracing::error!("No Ergo node URL provided in configuration. Tracker box updater requires node connection.");
        std::process::exit(1);
    }

    // Try to determine the network prefix from the tracker public key using the config method
    let network_prefix = match config.network_prefix_from_tracker_key() {
        Ok(prefix) => prefix,
        Err(_) => {
            // Default to mainnet if we can't determine the network prefix
            ergo_lib::ergotree_ir::address::NetworkPrefix::Mainnet
        }
    };

    let tracker_box_config = TrackerBoxUpdateConfig {
        update_interval_seconds: 600, // 10 minutes
        enabled: true,
        submit_transaction: config.tracker_public_key_bytes().ok().is_some(), // Enable submission if tracker key is configured
        ergo_node_url: config.ergo.node.node_url.clone(),
        ergo_api_key: config.ergo.node.api_key.clone(),
    };
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Clone the channel for the tracker updater
    let updater_shutdown_rx = shutdown_tx.subscribe();

    // Start the tracker box updater in the background
    let updater_config = tracker_box_config.clone();
    let shared_state_clone = shared_tracker_state_for_updater.clone();
    let updater_network_prefix = network_prefix; // Use the network_prefix determined above
    // Get the tracker NFT ID from config - it must be present since it's now required
    let tracker_nft_id = config.ergo.tracker_nft_id.clone().expect("Tracker NFT ID must be configured in server configuration");
    tokio::spawn(async move {
        if let Err(e) = TrackerBoxUpdater::start(
            updater_config,
            shared_state_clone,
            updater_network_prefix,
            tracker_nft_id, // Pass the required tracker NFT ID
            updater_shutdown_rx,
        ).await {
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

    let app_state = AppState {
        tx,
        event_store,
        ergo_scanner: std::sync::Arc::new(Mutex::new(ergo_scanner)),
        reserve_tracker: std::sync::Arc::new(Mutex::new(reserve_tracker)),
        config: std::sync::Arc::new(config.clone()),
    };

    // Build our application with routes - FIXED ROUTE ORDER
    let app = Router::new()
        // Root route
        .route("/", get(root))
        // Static routes
        .route("/events", get(get_events))
        .route("/events/paginated", get(get_events_paginated))
        .route("/notes", post(create_note).options(handle_options))
        .route("/redeem", post(initiate_redemption).options(handle_options))
        .route("/redeem/complete", post(complete_redemption).options(handle_options))
        .route("/proof", get(get_proof))
        .route("/reserves", get(get_all_reserves))
        .route("/reserves/create", post(create_reserve_payload).options(handle_options))
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
    tracing::debug!("  GET /reserves");
    tracing::debug!("  GET /reserves/issuer/{{pubkey}}");
    tracing::debug!("  POST /reserves/create");
    tracing::debug!("  GET /events");
    tracing::debug!("  GET /events/paginated");
    tracing::debug!("  GET /key-status/{{pubkey}}");
    tracing::debug!("  POST /redeem");
    tracing::debug!("  GET /proof");

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

/// Background task that continuously scans the blockchain for reserve events
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

                for ergo_box in &boxes {
                    // Extract owner pubkey from box registers (R4 register)
                    let owner_pubkey = match ergo_box.get_register("R4") {
                        Some(pubkey_hex) => {
                            // Parse hex-encoded public key from register
                            hex::decode(pubkey_hex).unwrap_or_default()
                        }
                        None => {
                            // Fallback to placeholder if register not found
                            format!("owner_of_{}", &ergo_box.box_id[..16]).into_bytes()
                        }
                    };

                    let tracker_nft_bytes_option = match config.tracker_nft_bytes() {
                        Ok(bytes) => Some(bytes),
                        Err(_) => {
                            tracing::error!("Tracker NFT ID is not properly configured");
                            continue; // Skip this box update
                        }
                    };

                    let reserve_info = basis_store::ExtendedReserveInfo::new(
                        ergo_box.box_id.as_bytes(),
                        &owner_pubkey,
                        ergo_box.value,
                        tracker_nft_bytes_option.as_deref(),
                        scanner.last_scanned_height().await,
                    );

                    if let Err(e) = tracker.update_reserve(reserve_info) {
                        tracing::warn!(
                            "Failed to update reserve info for {}: {}",
                            ergo_box.box_id,
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
            let reserve_info = basis_store::ExtendedReserveInfo::new(
                box_id.as_bytes(),
                owner_pubkey.as_bytes(),
                collateral_amount,
                tracker_nft_bytes_option.as_deref(),
                height,
            );
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
