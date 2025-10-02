mod api;
mod config;
mod models;
mod reserve_api;
mod store;

use axum::{
    routing::{get, post},
    Router,
};
use basis_store::{
    ergo_scanner::{NodeConfig, ReserveEvent, ServerState},
    ReserveTracker,
};
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{api::*, config::*, models::*, reserve_api::*, store::EventStore};

// Application state that holds a channel to communicate with the tracker thread
#[derive(Clone)]
struct AppState {
    tx: tokio::sync::mpsc::Sender<TrackerCommand>,
    event_store: std::sync::Arc<EventStore>,
    ergo_scanner: std::sync::Arc<Mutex<ServerState>>,
    reserve_tracker: std::sync::Arc<Mutex<ReserveTracker>>,
}

// Commands that can be sent to the tracker thread
#[derive(Debug)]
enum TrackerCommand {
    AddNote {
        issuer_pubkey: basis_store::PubKey,
        note: basis_store::IouNote,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::NoteError>>,
    },
    GetNotesByIssuer {
        issuer_pubkey: basis_store::PubKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<Vec<basis_store::IouNote>, basis_store::NoteError>>,
    },
    GetNotesByRecipient {
        recipient_pubkey: basis_store::PubKey,
        response_tx:
            tokio::sync::oneshot::Sender<Result<Vec<basis_store::IouNote>, basis_store::NoteError>>,
    },
    GetNoteByIssuerAndRecipient {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<
            Result<Option<basis_store::IouNote>, basis_store::NoteError>,
        >,
    },
    InitiateRedemption {
        request: basis_store::RedemptionRequest,
        response_tx: tokio::sync::oneshot::Sender<
            Result<basis_store::RedemptionData, basis_store::RedemptionError>,
        >,
    },
    CompleteRedemption {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        redeemed_amount: u64,
        response_tx: tokio::sync::oneshot::Sender<Result<(), basis_store::RedemptionError>>,
    },
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
                        host: "127.0.0.1".to_string(),
                        port: 3000,
                        database_url: Some("sqlite:data/basis.db".to_string()),
                    },
                    ergo: ErgoConfig {
                        node: NodeConfig {
                            start_height: None,
                            contract_template: None,
                        },
                        basis_contract_template: "2WbQhe1AudMj9Cx2DtNYwDVn6YVS5GA5S9otJfkAmARDrZ6wQczry4SbM2RafQoJ5gZj83L9BkjjkYUE95HrPM5dDSxeJCApKtomhTHvXFfyXBNAKj2rV2PVdnkJnZBFzvRoXwCMwgfP1shCPau2CrMYJmBg5HoFtLAvcHYuKNpjK8NRHoHVtCMvkVN2QnSezJcUukCudUyT1Gqy4hQFbLAEo9ZPUPnjuuoqscsvWouf4DRXJX3uPeaNaCEEeJtBRfx4aXaX36WEfauDCZ6Kc6XSVTDanXkGqvveLfLtk9DAA3Z7EU1jBhVoGy8nscW5UbUdJm7dLT6ZjaH29LjnPo3GaJfhcoRE6wUnDgX2xea4t23xkQNWebDEn2Yiv4JLTirGnGH5fBRZjueUivRv1ipp8G3tm3wKP5UM79AaRfVw5NecDTpR4QrKooqchNGSanTfLwzTEnwvqGSnqKbqJtJXyAfLX6Mf374ULUNa2C7ui8xip9RfmqNnv6cNDpexbQgTDKghhNtP2YWj8vssV65LNvVEaVNZAyrmCNfV3QVdn".to_string(),
                        start_height: 0,
                    },
                }
            })
        }
    };

    tracing::info!("Configuration loaded successfully");
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "basis_server=debug,tower_http=debug,axum=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize Ergo scanner
    tracing::info!("Initializing Ergo scanner...");
    let node_config = config.ergo_node_config();
    let mut ergo_scanner = ServerState::new(node_config);

    // Start scanner
    match ergo_scanner.start_scanning().await {
        Ok(()) => {
            tracing::info!("Ergo scanner started successfully");
            let current_height = ergo_scanner.get_current_height().await.unwrap_or(0);
            tracing::info!("Current blockchain height: {}", current_height);
        }
        Err(e) => {
            tracing::warn!(
                "Failed to start Ergo scanner: {}. Continuing without scanner...",
                e
            );
        }
    }

    // Initialize reserve tracker
    tracing::info!("Initializing reserve tracker...");
    let reserve_tracker = ReserveTracker::new();
    tracing::info!("Reserve tracker initialized successfully");

    // Create channel for communicating with tracker thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);

    // Spawn tracker thread (using tokio::task::spawn_blocking for CPU-bound work)
    tokio::task::spawn_blocking(move || {
        use basis_store::{RedemptionManager, TrackerStateManager};

        tracing::debug!("Tracker thread started");
        let tracker = TrackerStateManager::new();
        let mut redemption_manager = RedemptionManager::new(tracker);

        while let Some(cmd) = rx.blocking_recv() {
            tracing::debug!("Tracker thread received command: {:?}", cmd);
            match cmd {
                TrackerCommand::AddNote {
                    issuer_pubkey,
                    note,
                    response_tx,
                } => {
                    let result = redemption_manager.tracker.add_note(&issuer_pubkey, &note);

                    // Create event if successful
                    if result.is_ok() {
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


    let event_store = match EventStore::new().await {
        Ok(store) => {

            std::sync::Arc::new(store)
        }
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
    };


    // Build our application with routes

    let app = Router::new()
        .route("/", get(root))
        .route("/notes", post(create_note))
        .route("/notes/issuer/{pubkey}", get(get_notes_by_issuer))
        .route("/notes/recipient/{pubkey}", get(get_notes_by_recipient))
        .route(
            "/notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}",
            get(get_note_by_issuer_and_recipient),
        )
        .route("/reserves/issuer/{pubkey}", get(get_reserves_by_issuer))
        .route("/events", get(get_events))
        .route("/events/paginated", get(get_events_paginated))
        .route("/key-status/{pubkey}", get(get_key_status))
        .route("/redeem", post(initiate_redemption))
        .route("/redeem/complete", post(complete_redemption))
        .route("/proof", get(get_proof))
        .with_state(app_state.clone())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    tracing::debug!("Router built successfully");
    tracing::debug!("Registered routes:");
    tracing::debug!("  GET /");
    tracing::debug!("  POST /notes");
    tracing::debug!("  GET /notes/issuer/{{pubkey}}");
    tracing::debug!("  GET /notes/recipient/{{pubkey}}");
    tracing::debug!("  GET /notes/issuer/{{issuer_pubkey}}/recipient/{{recipient_pubkey}}");
    tracing::debug!("  GET /reserves/issuer/{{pubkey}}");
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

    // Start background scanner task
    let scanner_state = app_state.clone();
    tokio::spawn(async move {
        background_scanner_task(scanner_state).await;
    });

    tracing::info!("Starting axum server...");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    };
}

/// Background task that continuously scans the blockchain for reserve events
async fn background_scanner_task(state: AppState) {
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
        if !scanner.is_active() {
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

        // Scan for new blocks
        match scanner.scan_new_blocks().await {
            Ok(events) => {
                if !events.is_empty() {
                    tracing::info!("Found {} new reserve events", events.len());

                    // Process each event
                    for event in events {
                        if let Err(e) = process_reserve_event(&state, event).await {
                            tracing::error!("Failed to process reserve event: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to scan new blocks: {}", e);
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
                    // For now, use a placeholder owner pubkey since extract_owner_pubkey is private
                    // In real implementation, we'd need to extract this from the box registers
                    let owner_pubkey = format!("owner_of_{}", &ergo_box.box_id[..16]);

                    let reserve_info = basis_store::ExtendedReserveInfo::new(
                        ergo_box.box_id.as_bytes(),
                        owner_pubkey.as_bytes(),
                        ergo_box.value,
                        None, // tracker_nft_id
                        scanner.last_scanned_height(),
                    );

                    if let Err(e) = tracker.update_reserve(reserve_info) {
                        tracing::warn!("Failed to update reserve info for {}: {}", owner_pubkey, e);
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

/// Process a reserve event and store it in the event store
async fn process_reserve_event(
    state: &AppState,
    event: ReserveEvent,
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
            let tracker = state.reserve_tracker.lock().await;
            let reserve_info = basis_store::ExtendedReserveInfo::new(
                box_id.as_bytes(),
                owner_pubkey.as_bytes(),
                collateral_amount,
                None, // tracker_nft_id
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
