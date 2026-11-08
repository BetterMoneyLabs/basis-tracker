//! GET /capabilities on the v3 tracker (the exact contract clients pin), the pure-credit startup decision, the
//! 503 `novations_disabled` answer of the novation routes while the confirmation records failed to load, and the
//! Write role of POST /notes/transfer and POST /notes/setoff. Requests go through the axum router (with the
//! auth and authorization layers where roles matter) and a tracker thread backed by a real store.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use basis_server::{
    api,
    auth_middleware::{auth_middleware, AuthState},
    authorization::authorization_middleware,
    config::{AuthConfig, AuthMode, AuthorizedClient, ClientRole},
    store::EventStore,
    AppState, TrackerCommand,
};
use basis_store::{DenomKey, IouNote, TrackerStateManager, NOVATIONS_DISABLED_CONFIRMATIONS};
use secp256k1::{Secp256k1, SecretKey};
use serde_json::{json, Value};
use sha2::Digest;
use std::sync::Arc;
use tower::ServiceExt;

const NFT_HEX: &str = "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b";
const A: [u8; 32] = [1u8; 32];
const B: [u8; 32] = [2u8; 32];
const C: [u8; 32] = [3u8; 32];

/// Serializes fjall storage initialization across concurrent tests (as in the other integration tests).
static STORAGE_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn nft() -> [u8; 32] {
    hex::decode(NFT_HEX).unwrap().try_into().unwrap()
}

fn pk(secret: &[u8; 32]) -> [u8; 33] {
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(secret).expect("valid secret key");
    secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
}

fn pk_hex(secret: &[u8; 32]) -> String {
    hex::encode(pk(secret))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// The latest timestamp at or before `t` whose residue (`ts % 1000`) is `ms`.
fn at_residue(t: u64, ms: u64) -> u64 {
    let candidate = t - t % 1000 + ms;
    if candidate <= t {
        candidate
    } else {
        candidate - 1000
    }
}

/// `signer`'s signature (hex) over the ERG note message (issuer -> recipient, `total`, `ts`).
fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> String {
    let msg = basis_store::schnorr::signing_message(&pk(issuer), &pk(recipient), &DenomKey::Erg, total, ts, &nft());
    hex::encode(basis_store::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap())
}

/// The POST /notes body of `issuer`'s note to `recipient` at the cumulative `total` and `ts`.
fn note_body(issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> Value {
    json!({
        "recipient_pubkey": pk_hex(recipient),
        "amount": total,
        "timestamp": ts,
        "signature": sign(issuer, issuer, recipient, total, ts),
        "issuer_pubkey": pk_hex(issuer),
        "tracker_nft_id": NFT_HEX,
        "denomination": "erg",
    })
}

/// The POST /notes/transfer body moving `amount` of A's debt from A->B (stored total `x`) to A->C (`y`) at `ts`,
/// with all seven signatures.
fn transfer_body(x: u64, y: u64, amount: u64, ts: u64) -> Value {
    json!({
        "issuer_pubkey": pk_hex(&A),
        "from_recipient_pubkey": pk_hex(&B),
        "to_recipient_pubkey": pk_hex(&C),
        "denomination": "erg",
        "amount": amount,
        "timestamp": ts,
        "issuer_from_signature": sign(&A, &A, &B, x - amount, ts),
        "issuer_to_signature": sign(&A, &A, &C, y + amount, ts),
        "from_recipient_prior_signature": sign(&B, &A, &B, x, ts),
        "from_recipient_signature": sign(&B, &A, &B, x - amount, ts),
        "from_recipient_to_signature": sign(&B, &A, &C, y + amount, ts),
        "to_recipient_prior_signature": sign(&C, &A, &C, y, ts),
        "to_recipient_signature": sign(&C, &A, &C, y + amount, ts),
    })
}

/// The POST /notes/setoff body clearing the cycle A->B (stored total `ab`), B->A (`ba`) by `d` at `ts`.
fn setoff_body(ab: u64, ba: u64, d: u64, ts: u64) -> Value {
    let edge = |issuer: &[u8; 32], recipient: &[u8; 32], before: u64| {
        json!({
            "issuer_pubkey": pk_hex(issuer),
            "recipient_pubkey": pk_hex(recipient),
            "decrease": d,
            "issuer_signature": sign(issuer, issuer, recipient, before - d, ts),
            "recipient_signature": sign(recipient, issuer, recipient, before - d, ts),
        })
    };
    json!({"denomination": "erg", "timestamp": ts, "edges": [edge(&A, &B, ab), edge(&B, &A, ba)]})
}

/// The exact GET /capabilities body of this build (design §3.6, with decision D-1: bound notes version 2 with their
/// one-hour maximum age, tracker v3.1) with the run-time values filled in.
fn expected_capabilities(pure_credit: bool, onchain_edges: u64, novations_disabled_reason: Value) -> Value {
    json!({"success": true, "data": {
        "tracker": "basis", "build": "valun-circulating-credit/3", "commit": commit(),
        "pure_credit": pure_credit, "reserved_ms": [995, 996, 997, 998, 999],
        "bound_notes": {"ms": 997, "version": 2, "max_age_ms": 3600000}, "bound_only_edges": true,
        "novation": {"max_age_ms": 3600000,
            "transfer": {"ms": 999, "version": 3, "signatures": ["issuer_from", "issuer_to",
                "from_recipient_prior", "from_recipient", "from_recipient_to", "to_recipient_prior", "to_recipient"]},
            "setoff": {"ms": 998, "version": 3, "max_edges": 64, "per_edge_timestamps": true,
                "signatures_per_edge": ["issuer", "recipient"]}},
        "onchain_edges": onchain_edges, "novations_disabled_reason": novations_disabled_reason}})
}

/// The commit the build reports: BASIS_GIT_COMMIT at build time, else "unknown".
fn commit() -> &'static str {
    option_env!("BASIS_GIT_COMMIT").filter(|c| !c.is_empty()).unwrap_or("unknown")
}

/// A tracker thread over a fresh store (created on the thread: the store is not `Send`), serving the commands
/// these tests use.
fn spawn_tracker() -> tokio::sync::mpsc::Sender<TrackerCommand> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TrackerCommand>(100);
    std::thread::spawn(move || {
        let mut tracker = {
            let _guard = STORAGE_INIT_LOCK.lock().unwrap();
            TrackerStateManager::new_with_temp_storage()
        };
        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                TrackerCommand::AddNote { issuer_pubkey, note, prior, response_tx } => {
                    let _ = response_tx.send(tracker.add_note(&issuer_pubkey, &note, prior));
                }
                TrackerCommand::GetNoteByIssuerAndRecipient { issuer_pubkey, recipient_pubkey, denomination, response_tx } => {
                    let _ = response_tx.send(tracker.get_note_optional(&issuer_pubkey, &recipient_pubkey, &denomination));
                }
                TrackerCommand::GetNotesByIssuer { issuer_pubkey, response_tx } => {
                    let _ = response_tx.send(tracker.get_issuer_notes(&issuer_pubkey));
                }
                TrackerCommand::TransferNotes {
                    issuer_pubkey,
                    from_recipient_pubkey,
                    to_recipient_pubkey,
                    denomination,
                    amount,
                    timestamp,
                    signatures,
                    from_prior_total,
                    to_prior_total,
                    acceptance_check,
                    response_tx,
                } => {
                    let _ = response_tx.send(tracker.transfer_debt_checked(
                        &issuer_pubkey,
                        &from_recipient_pubkey,
                        &to_recipient_pubkey,
                        &denomination,
                        amount,
                        timestamp,
                        &signatures,
                        from_prior_total,
                        to_prior_total,
                        acceptance_check.map(|check| check.0),
                    ));
                }
                TrackerCommand::SetOff { denomination, edges, response_tx } => {
                    let _ = response_tx.send(tracker.set_off(&denomination, &edges));
                }
                TrackerCommand::MarkNotesPending { digest, tx_id, submitted_height, response_tx } => {
                    let _ = response_tx.send(tracker.mark_notes_pending(digest, &tx_id, submitted_height));
                }
                TrackerCommand::OnchainEdgeCount { response_tx } => {
                    let _ = response_tx.send(tracker.onchain_edge_count());
                }
                _ => {}
            }
        }
    });
    tx
}

struct Tracker {
    state: AppState,
    _dirs: Vec<tempfile::TempDir>,
}

/// An AppState over a tracker thread with a fresh store, with `[server] pure_credit = pure_credit` and the given
/// novations-disabled reason (what main.rs sets from `basis_server::tracker_startup`).
async fn tracker(pure_credit: bool, novations_disabled_reason: Option<String>) -> Tracker {
    let tx = spawn_tracker();
    let server_dir = tempfile::tempdir().unwrap();
    let scanner_dir = tempfile::tempdir().unwrap();
    let storage_dir = tempfile::tempdir().unwrap();
    // Built from JSON so that settings with serde defaults keep their defaults.
    let config: basis_server::config::AppConfig = serde_json::from_value(json!({
        "server": {
            "host": "127.0.0.1",
            "port": 3048,
            "data_dir": server_dir.path().to_string_lossy(),
            "pure_credit": pure_credit,
        },
        "ergo": {
            "scanner": { "node_url": "http://127.0.0.1:9" },
            "basis_reserve_contract_p2s": "test",
            "basis_token_reserve_contract_p2s": "test_token",
            "tracker_nft_id": NFT_HEX,
            "tracker_public_key": null,
            "tracker_secret_key": null,
        },
        "transaction": { "fee": 1000000, "change_address": null },
    }))
    .unwrap();
    assert_eq!(config.server.pure_credit, pure_credit);
    let (scanner, tracker_storage, policy_storage) = {
        let _guard = STORAGE_INIT_LOCK.lock().unwrap();
        let scanner = basis_store::ergo_scanner::ServerState::new(config.ergo.scanner.clone(), scanner_dir.path()).unwrap();
        let tracker_storage = basis_store::persistence::TrackerStorage::open(storage_dir.path().join("tracker")).unwrap();
        let policy_storage =
            basis_store::persistence::AcceptancePolicyStorage::open(storage_dir.path().join("policies")).unwrap();
        (scanner, tracker_storage, policy_storage)
    };
    let state = AppState {
        tx,
        event_store: Arc::new(EventStore::new_in_memory()),
        ergo_scanner: Arc::new(tokio::sync::Mutex::new(scanner)),
        reserve_tracker: Arc::new(tokio::sync::Mutex::new(basis_store::ReserveTracker::new())),
        config: Arc::new(config),
        shared_tracker_state: Arc::new(tokio::sync::Mutex::new(
            basis_server::tracker_box_updater::SharedTrackerState::new(),
        )),
        tracker_storage,
        acceptance_predicate: None,
        policy_storage,
        pending_redemptions: Arc::new(basis_server::redemption_build::PendingRedemptionRegistry::new()),
        redemption_build_rate: Arc::new(basis_server::redemption_build::BuildRateLimiter::new()),
        novations_disabled_reason,
    };
    Tracker { state, _dirs: vec![server_dir, scanner_dir, storage_dir] }
}

/// The routes these tests use, as main.rs registers them.
fn routes(state: &AppState) -> axum::Router {
    axum::Router::new()
        .route("/capabilities", axum::routing::get(api::get_capabilities))
        .route("/notes", axum::routing::post(api::create_note))
        .route("/notes/transfer", axum::routing::post(api::transfer_notes))
        .route("/notes/setoff", axum::routing::post(api::setoff_notes))
        .with_state(state.clone())
}

/// The routes behind the auth and authorization layers, in main.rs's order.
fn authed_routes(state: &AppState, auth: AuthConfig) -> axum::Router {
    routes(state)
        .layer(axum::middleware::from_fn(authorization_middleware))
        .layer(axum::middleware::from_fn_with_state(Arc::new(AuthState::new(auth)), auth_middleware))
}

/// Send `request`; returns the status and the raw body.
async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().method(Method::GET).uri(uri).body(Body::empty()).unwrap()
}

fn json_of(status: StatusCode, raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| panic!("not JSON ({}): {}", status, raw))
}

impl Tracker {
    async fn capabilities(&self) -> (StatusCode, String) {
        send(&routes(&self.state), get("/capabilities")).await
    }

    async fn post(&self, uri: &str, body: &Value) -> (StatusCode, Value) {
        let (status, raw) = send(&routes(&self.state), post(uri, body)).await;
        (status, json_of(status, &raw))
    }

    /// The stored ERG total of issuer -> recipient, if the edge exists.
    async fn total(&self, issuer: &[u8; 32], recipient: &[u8; 32]) -> Option<u64> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.state
            .tx
            .send(TrackerCommand::GetNoteByIssuerAndRecipient {
                issuer_pubkey: pk(issuer),
                recipient_pubkey: pk(recipient),
                denomination: DenomKey::Erg,
                response_tx: tx,
            })
            .await
            .unwrap();
        rx.await.unwrap().unwrap().map(|n: IouNote| n.amount_collected)
    }

    /// A owes B 100 and C 50, and B owes A 100 (ordinary notes at `t0`, `t0` + 1, `t0` + 2).
    async fn seed(&self, t0: u64) {
        for (issuer, recipient, total, ts) in [(&A, &B, 100, t0), (&A, &C, 50, t0 + 1), (&B, &A, 100, t0 + 2)] {
            let (status, body) = self.post("/notes", &note_body(issuer, recipient, total, ts)).await;
            assert_eq!(status, StatusCode::CREATED, "{body}");
        }
    }
}

#[tokio::test]
async fn capabilities_report_the_exact_v3_contract() {
    let t = tracker(true, None).await;
    let (status, raw) = t.capabilities().await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    // Exactly the design's object, nothing more: compared as JSON values...
    assert_eq!(json_of(status, &raw), expected_capabilities(true, 0, Value::Null));
    // ...and as the exact bytes, so the key order and the absence of an `error` field are pinned too.
    let exact = format!(
        concat!(
            r#"{{"success":true,"data":{{"tracker":"basis","build":"valun-circulating-credit/3","commit":"{}","#,
            r#""pure_credit":true,"reserved_ms":[995,996,997,998,999],"bound_notes":{{"ms":997,"version":2,"max_age_ms":3600000}},"#,
            r#""bound_only_edges":true,"novation":{{"max_age_ms":3600000,"transfer":{{"ms":999,"version":3,"#,
            r#""signatures":["issuer_from","issuer_to","from_recipient_prior","from_recipient","from_recipient_to","#,
            r#""to_recipient_prior","to_recipient"]}},"setoff":{{"ms":998,"version":3,"max_edges":64,"#,
            r#""per_edge_timestamps":true,"signatures_per_edge":["issuer","recipient"]}}}},"onchain_edges":0,"#,
            r#""novations_disabled_reason":null}}}}"#
        ),
        commit()
    );
    assert_eq!(raw, exact);

    // Without pure_credit only that flag differs.
    let t = tracker(false, None).await;
    let (status, raw) = t.capabilities().await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(json_of(status, &raw), expected_capabilities(false, 0, Value::Null));
}

#[tokio::test]
async fn capabilities_count_the_edges_with_a_committed_or_pending_total() {
    let t = tracker(false, None).await;
    t.seed(at_residue(now_ms() - 60_000, 100)).await;
    let (status, raw) = t.capabilities().await;
    assert_eq!(json_of(status, &raw)["data"]["onchain_edges"], json!(0));

    // The box updater submits a tracker box update covering every edge: they now have pending on-chain totals.
    let (tx, rx) = tokio::sync::oneshot::channel();
    t.state
        .tx
        .send(TrackerCommand::MarkNotesPending { digest: [0u8; 33], tx_id: "tx1".to_string(), submitted_height: 1, response_tx: tx })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap().unwrap(), 3);
    let (status, raw) = t.capabilities().await;
    assert_eq!(json_of(status, &raw), expected_capabilities(false, 3, Value::Null));
}

#[tokio::test]
async fn novations_answer_503_while_the_confirmation_records_are_unreadable() {
    // main.rs sets this reason when the store's confirmation records failed to load (basis_server::tracker_startup).
    let t = tracker(false, Some(NOVATIONS_DISABLED_CONFIRMATIONS.to_string())).await;
    let t0 = at_residue(now_ms() - 60_000, 100);
    // Ordinary notes still apply.
    t.seed(t0).await;

    let disabled = json!({
        "success": false,
        "data": null,
        "error": format!("Novations are disabled: {}", NOVATIONS_DISABLED_CONFIRMATIONS),
        "code": "novations_disabled",
    });
    // The transfer at tt (residue 999); the set-off one second later (residue 998), still in the past.
    let tt = at_residue(now_ms() - 1000, 999);
    let transfer = transfer_body(100, 50, 20, tt);
    assert_eq!(t.post("/notes/transfer", &transfer).await, (StatusCode::SERVICE_UNAVAILABLE, disabled.clone()));
    let setoff = setoff_body(100, 100, 30, tt + 999);
    assert_eq!(t.post("/notes/setoff", &setoff).await, (StatusCode::SERVICE_UNAVAILABLE, disabled.clone()));
    // Before anything else is looked at: a request that would be refused for its content gets the same answer.
    let mut malformed = transfer.clone();
    malformed["issuer_from_signature"] = json!("zz");
    assert_eq!(t.post("/notes/transfer", &malformed).await, (StatusCode::SERVICE_UNAVAILABLE, disabled.clone()));
    assert_eq!(
        (t.total(&A, &B).await, t.total(&A, &C).await, t.total(&B, &A).await),
        (Some(100), Some(50), Some(100))
    );
    // GET /capabilities reports the reason.
    let (status, raw) = t.capabilities().await;
    assert_eq!(
        json_of(status, &raw),
        expected_capabilities(false, 0, json!(NOVATIONS_DISABLED_CONFIRMATIONS))
    );

    // The same requests apply on a tracker whose confirmation records loaded.
    let t = tracker(false, None).await;
    t.seed(t0).await;
    let (status, body) = t.post("/notes/transfer", &transfer).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = t.post("/notes/setoff", &setoff_body(80, 100, 30, tt + 999)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        (t.total(&A, &B).await, t.total(&A, &C).await, t.total(&B, &A).await),
        (Some(50), Some(70), Some(70))
    );
}

#[test]
fn the_pure_credit_startup_decision_follows_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let t0 = at_residue(now_ms() - 60_000, 100);
    {
        let _guard = STORAGE_INIT_LOCK.lock().unwrap();
        let mut store = TrackerStateManager::new(dir.path());
        let note = IouNote::create_and_sign(pk(&B), 100, t0, &A, &nft(), DenomKey::Erg).unwrap();
        store.add_note(&pk(&A), &note, None).unwrap();
        // A pure-credit store: both modes start, and novations are enabled.
        assert_eq!(basis_server::tracker_startup(&store, true), Ok(None));
        assert_eq!(basis_server::tracker_startup(&store, false), Ok(None));
        // A tracker box commits the tree on-chain: the edge has a confirmed total.
        let digest = store.get_state().avl_root_digest;
        assert_eq!(store.reconcile_with_confirmed_digest(&digest, "box1", 100).unwrap(), 1);
    }
    let _guard = STORAGE_INIT_LOCK.lock().unwrap();
    let store = TrackerStateManager::new(dir.path());
    let refusal = basis_server::tracker_startup(&store, true).unwrap_err();
    assert!(
        refusal.starts_with(
            "refusing to start in pure_credit mode: 1 stored confirmation record(s) carry a committed or pending on-chain total."
        ),
        "{refusal}"
    );
    // Without pure_credit the same store starts as before (its on-chain edge is refused by the scope check).
    assert_eq!(basis_server::tracker_startup(&store, false), Ok(None));
}

/// A request signed for signature auth as `secret` (as in auth_middleware_tests.rs), with a unique nonce.
fn signed(method: Method, uri: &str, body: Option<&Value>, secret: &SecretKey, nonce: &str) -> Request<Body> {
    let secp = Secp256k1::new();
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp, secret).serialize();
    let body_bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
    let timestamp = now_ms();
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        uri,
        "",
        timestamp,
        nonce,
        hex::encode(sha2::Sha256::digest(&body_bytes))
    );
    let signature = basis_offchain::schnorr::schnorr_sign(canonical.as_bytes(), secret, &pubkey).unwrap();
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("X-Signature-Pubkey", hex::encode(pubkey))
        .header("X-Signature", hex::encode(signature))
        .header("X-Signature-Timestamp", timestamp.to_string())
        .header("X-Signature-Nonce", nonce)
        .body(Body::from(body_bytes))
        .unwrap()
}

#[tokio::test]
async fn the_novation_routes_need_the_write_role_and_capabilities_the_read_role() {
    let t = tracker(true, None).await;
    let reader = SecretKey::from_slice(&[0x21; 32]).unwrap();
    let writer = SecretKey::from_slice(&[0x22; 32]).unwrap();
    let secp = Secp256k1::new();
    let client = |secret: &SecretKey, role| AuthorizedClient {
        pubkey: hex::encode(secp256k1::PublicKey::from_secret_key(&secp, secret).serialize()),
        role,
    };
    let auth = AuthConfig {
        mode: AuthMode::Signature,
        authorized_clients: vec![client(&reader, ClientRole::Read), client(&writer, ClientRole::Write)],
        ..Default::default()
    };
    let app = authed_routes(&t.state, auth);
    // No such edges exist, so a request that passes authorization reaches the store and is refused there (404).
    let transfer = transfer_body(100, 50, 20, at_residue(now_ms() - 1000, 999));
    let setoff = setoff_body(100, 100, 30, at_residue(now_ms() - 1000, 998));

    // Write is enough for both novation routes (they used to require Admin).
    for (n, (uri, body)) in [("/notes/transfer", &transfer), ("/notes/setoff", &setoff)].into_iter().enumerate() {
        let (status, raw) = send(&app, signed(Method::POST, uri, Some(body), &writer, &format!("w{n}"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {raw}");
        assert_eq!(json_of(status, &raw)["code"], json!("not_found"), "{uri}");
    }
    // Read is not.
    for (n, (uri, body)) in [("/notes/transfer", &transfer), ("/notes/setoff", &setoff)].into_iter().enumerate() {
        let (status, raw) = send(&app, signed(Method::POST, uri, Some(body), &reader, &format!("r{n}"))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {raw}");
        assert!(raw.contains("route requires Write access"), "{uri}: {raw}");
    }
    // GET /capabilities needs only Read.
    let (status, raw) = send(&app, signed(Method::GET, "/capabilities", None, &reader, "r-cap")).await;
    assert_eq!((status, json_of(status, &raw)), (StatusCode::OK, expected_capabilities(true, 0, Value::Null)));

    // With auth.mode none (Valun's loopback tracker), no credentials are needed.
    let app = authed_routes(&t.state, AuthConfig::default());
    let (status, raw) = send(&app, get("/capabilities")).await;
    assert_eq!((status, json_of(status, &raw)), (StatusCode::OK, expected_capabilities(true, 0, Value::Null)));
}
