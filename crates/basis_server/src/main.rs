mod api;
mod config;
mod models;
mod reserve_api;
mod store;

use axum::{routing::{get, post}, Router};
use basis_store::ergo_scanner::NodeConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{api::*, config::*, models::*, store::EventStore};

// Application state that holds a channel to communicate with the tracker thread
#[derive(Clone)]
struct AppState {
    tx: tokio::sync::mpsc::Sender<TrackerCommand>,
    event_store: std::sync::Arc<EventStore>,
    ergo_scanner: std::sync::Arc<basis_store::ergo_scanner::ErgoScanner>,
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
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<basis_store::IouNote>, basis_store::NoteError>>,
    },
    GetNoteByIssuerAndRecipient {
        issuer_pubkey: basis_store::PubKey,
        recipient_pubkey: basis_store::PubKey,
        response_tx: tokio::sync::oneshot::Sender<Result<Option<basis_store::IouNote>, basis_store::NoteError>>,
    },
}

#[tokio::main]
async fn main() {
    eprintln!("Starting basis server...");
    // Load configuration
    eprintln!("Loading configuration...");
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            eprintln!("Using default configuration...");
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
                            url: "http://localhost:9053".to_string(),
                            api_key: "".to_string(),
                            timeout_secs: 30,
                        },
                        basis_contract_template: "".to_string(),
                        start_height: 0,
                    },
                }
            })
        }
    };

    eprintln!("Configuration loaded successfully");
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "basis_server=debug,tower_http=debug,axum=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create channel for communicating with tracker thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);

    // Spawn tracker thread (using tokio::task::spawn_blocking for CPU-bound work)
    tokio::task::spawn_blocking(move || {
        use basis_store::TrackerStateManager;

        tracing::debug!("Tracker thread started");
        let mut tracker = TrackerStateManager::new();

        while let Some(cmd) = rx.blocking_recv() {
            tracing::debug!("Tracker thread received command: {:?}", cmd);
            match cmd {
                TrackerCommand::AddNote {
                    issuer_pubkey,
                    note,
                    response_tx,
                } => {
                    let result = tracker.add_note(&issuer_pubkey, &note);
                    
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
                    let result = tracker.get_issuer_notes(&issuer_pubkey);
                    let _ = response_tx.send(result);
                }
                TrackerCommand::GetNoteByIssuerAndRecipient {
                    issuer_pubkey,
                    recipient_pubkey,
                    response_tx,
                } => {
                    let result = tracker
                        .lookup_note(&issuer_pubkey, &recipient_pubkey)
                        .map(Some);
                    let _ = response_tx.send(result);
                }
            }
        }
    });

    eprintln!("Creating event store...");
    let event_store = match EventStore::new().await {
        Ok(store) => {
            eprintln!("Event store created successfully");
            std::sync::Arc::new(store)
        },
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
            issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
            recipient_pubkey: Some("020202020202020202020202020202020202020202020202020202020202020202".to_string()),
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
            issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
            recipient_pubkey: Some("030303030303030303030303030303030303030303030303030303030303030303".to_string()),
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
            issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
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
            issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
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
            issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
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
            issuer_pubkey: Some("010101010101010101010101010101010101010101010101010101010101010101".to_string()),
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: None,
            collateral_amount: None,
            redeemed_amount: None,
            height: None,
        },
    ];
    
    eprintln!("Adding demo events...");
    for event in demo_events {
        if let Err(e) = event_store.add_event(event).await {
            tracing::warn!("Failed to add demo event: {:?}", e);
        }
    }
    eprintln!("Demo events added successfully");
    
    eprintln!("Creating ergo scanner...");
    let basis_contract_bytes = config.basis_contract_bytes().unwrap_or_default();
    let ergo_scanner = std::sync::Arc::new(basis_store::ergo_scanner::ErgoScanner::new(config.ergo_node_config(), basis_contract_bytes));
    eprintln!("Ergo scanner created successfully");
    let app_state = AppState { tx, event_store, ergo_scanner };
    eprintln!("App state created successfully");

    // Build our application with routes
    eprintln!("Building router...");
    let app = Router::new()
        .route("/", get(root))
        .route("/notes", post(create_note))
        .route("/notes/issuer/{pubkey}", get(get_notes_by_issuer))
        .with_state(app_state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    eprintln!("Router built successfully");
    tracing::debug!("Registered routes:");
    eprintln!("Registered routes:");
    eprintln!("  GET /");
    eprintln!("  POST /notes");
    eprintln!("  GET /notes/issuer/{{pubkey}}");
    tracing::debug!("  GET /");
    tracing::debug!("  POST /notes");
    tracing::debug!("  GET /notes/issuer/{{pubkey}}");
    tracing::debug!("  GET /notes/issuer/{{issuer_pubkey}}/recipient/{{recipient_pubkey}}");
    tracing::debug!("  GET /reserves/issuer/{{pubkey}}");
    tracing::debug!("  GET /events (Event polling)");

    // Run our app with hyper
    let addr = config.socket_addr();
    tracing::debug!("listening on {}", addr);
    eprintln!("Starting server on {}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            eprintln!("Server listening on {}", addr);
            listener
        },
        Err(e) => {
            eprintln!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    
    eprintln!("Starting axum server...");
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    };
}