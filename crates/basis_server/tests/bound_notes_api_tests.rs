//! POST /notes on the v3 tracker: timestamp residues (995-999 ms reserved), bound
//! notes (residue 997 with the issuer's prior acknowledgment of the edge's current
//! total) and bound-only edges (changed by a set-off or transfer), with the
//! machine-readable `code` on every refusal. Every request goes through the axum
//! router and a tracker thread backed by a real store.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use basis_server::{api, store::EventStore, AppState, TrackerCommand};
use basis_store::{DenomKey, IouNote, TrackerStateManager};
use secp256k1::{Secp256k1, SecretKey};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

const NFT_HEX: &str = "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b";
const A: [u8; 32] = [1u8; 32];
const B: [u8; 32] = [2u8; 32];
const C: [u8; 32] = [3u8; 32];

/// Serializes fjall storage initialization across concurrent tests (as in the
/// other integration tests).
static STORAGE_INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn nft() -> [u8; 32] {
    hex::decode(NFT_HEX).unwrap().try_into().unwrap()
}

fn pk(secret: &[u8; 32]) -> [u8; 33] {
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(secret).expect("valid secret key");
    secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
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
        "recipient_pubkey": hex::encode(pk(recipient)),
        "amount": total,
        "timestamp": ts,
        "signature": sign(issuer, issuer, recipient, total, ts),
        "issuer_pubkey": hex::encode(pk(issuer)),
        "tracker_nft_id": NFT_HEX,
        "denomination": "erg",
    })
}

/// `body` with a prior acknowledgment of `prior` signed by `signer`, and the optional `prior_total`.
fn with_prior(mut body: Value, signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], prior: u64, prior_total: Option<u64>) -> Value {
    let ts = body["timestamp"].as_u64().unwrap();
    body["prior_signature"] = json!(sign(signer, issuer, recipient, prior, ts));
    if let Some(p) = prior_total {
        body["prior_total"] = json!(p);
    }
    body
}

struct Tracker {
    app: axum::Router,
    state: AppState,
    _dirs: Vec<tempfile::TempDir>,
}

/// A router with POST /notes and POST /notes/setoff over a tracker thread with a fresh store.
async fn tracker() -> Tracker {
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
                TrackerCommand::GetNotesByIssuer { issuer_pubkey, response_tx } => {
                    let _ = response_tx.send(tracker.get_issuer_notes(&issuer_pubkey));
                }
                TrackerCommand::GetNoteByIssuerAndRecipient { issuer_pubkey, recipient_pubkey, denomination, response_tx } => {
                    let _ = response_tx.send(tracker.get_note_optional(&issuer_pubkey, &recipient_pubkey, &denomination));
                }
                TrackerCommand::SetOff { denomination, edges, response_tx } => {
                    let _ = response_tx.send(tracker.set_off(&denomination, &edges));
                }
                _ => {}
            }
        }
    });

    let server_dir = tempfile::tempdir().unwrap();
    let scanner_dir = tempfile::tempdir().unwrap();
    let storage_dir = tempfile::tempdir().unwrap();
    // Built from JSON so that settings with serde defaults keep their defaults.
    let config: basis_server::config::AppConfig = serde_json::from_value(json!({
        "server": {
            "host": "127.0.0.1",
            "port": 3048,
            "data_dir": server_dir.path().to_string_lossy(),
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
        novations_disabled_reason: None,
        redemption_build_rate: Arc::new(basis_server::redemption_build::BuildRateLimiter::new()),
    };
    let app = axum::Router::new()
        .route("/notes", axum::routing::post(api::create_note))
        .route("/notes/setoff", axum::routing::post(api::setoff_notes))
        .with_state(state.clone());
    Tracker { app, state, _dirs: vec![server_dir, scanner_dir, storage_dir] }
}

impl Tracker {
    /// POST `body` to `uri`; returns the status and the raw response body.
    async fn post_raw(&self, uri: &str, body: &Value) -> (StatusCode, String) {
        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn post(&self, uri: &str, body: &Value) -> (StatusCode, Value) {
        let (status, raw) = self.post_raw(uri, body).await;
        (status, serde_json::from_str(&raw).unwrap())
    }

    async fn note(&self, body: &Value) -> (StatusCode, Value) {
        self.post("/notes", body).await
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
}

/// Assert an error response: status, `code` (or no `code` key at all) and `success: false`.
fn assert_refused(response: &(StatusCode, Value), status: StatusCode, code: Option<&str>) {
    let (s, v) = response;
    assert_eq!(*s, status, "response: {}", v);
    assert_eq!(v["success"], false, "response: {}", v);
    match code {
        Some(c) => assert_eq!(v["code"], c, "response: {}", v),
        None => assert!(v.get("code").is_none(), "unexpected code in {}", v),
    }
}

fn assert_created(response: &(StatusCode, Value)) {
    let (s, v) = response;
    assert_eq!(*s, StatusCode::CREATED, "response: {}", v);
    assert_eq!(v["success"], true);
    assert!(v.get("code").is_none(), "a success has no code: {}", v);
}

#[tokio::test]
async fn post_notes_refuses_reserved_residues_and_accepts_994_and_000() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 10, at_residue(base, 0))).await);
    let later = base + 2_000;
    for ms in [995, 996, 998, 999] {
        let r = t.note(&note_body(&A, &B, 20, at_residue(later, ms))).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
        if ms == 999 {
            // The message of the residue that was already reserved is unchanged.
            assert_eq!(
                r.1["error"],
                "timestamps ending in 999 ms are reserved for novations (debt transfer, set-off); use another millisecond"
            );
        }
    }
    // 997 without prior_signature is a bound note missing its acknowledgment.
    let r = t.note(&note_body(&A, &B, 20, at_residue(later, 997))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    assert!(r.1["error"].as_str().unwrap().contains("prior_signature"), "{}", r.1);
    // A transfer's raised-edge signature submitted alone (999) is refused on a new edge too.
    assert_refused(&t.note(&note_body(&A, &C, 5, at_residue(later, 999))).await, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(10), None));
    // The largest ordinary residue is accepted.
    assert_created(&t.note(&note_body(&A, &B, 20, at_residue(later, 994))).await);
    assert_eq!(t.total(&A, &B).await, Some(20));
}

#[tokio::test]
async fn a_bound_note_needs_the_issuers_acknowledgment_of_the_current_total() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 10, at_residue(base, 0))).await);
    let ts = at_residue(base + 2_000, 997);
    // A prior at a wrong total: stale_prior (409) when its total is sent, invalid_prior_signature when not.
    let r = t.note(&with_prior(note_body(&A, &B, 15, ts), &A, &A, &B, 9, Some(9))).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("stale_prior"));
    let r = t.note(&with_prior(note_body(&A, &B, 15, ts), &A, &A, &B, 9, None)).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_prior_signature"));
    // A prior signed by the recipient instead of the issuer.
    let r = t.note(&with_prior(note_body(&A, &B, 15, ts), &B, &A, &B, 10, Some(10))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_prior_signature"));
    // An equal total is not an increase.
    let r = t.note(&with_prior(note_body(&A, &B, 10, ts), &A, &A, &B, 10, Some(10))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("not_an_increase"));
    assert_eq!(t.total(&A, &B).await, Some(10));
    // With the issuer's acknowledgment of the stored total the bound note applies.
    assert_created(&t.note(&with_prior(note_body(&A, &B, 15, ts), &A, &A, &B, 10, Some(10))).await);
    assert_eq!(t.total(&A, &B).await, Some(15));
    // prior_total is optional.
    let ts2 = ts + 1_000;
    assert_created(&t.note(&with_prior(note_body(&A, &B, 18, ts2), &A, &A, &B, 15, None)).await);
    assert_eq!(t.total(&A, &B).await, Some(18));
}

#[tokio::test]
async fn a_first_bound_note_on_an_absent_edge_acknowledges_zero() {
    let t = tracker().await;
    let ts = at_residue(now_ms() - 60_000, 997);
    let r = t.note(&with_prior(note_body(&A, &C, 7, ts), &A, &A, &C, 3, Some(3))).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("stale_prior"));
    assert_eq!(t.total(&A, &C).await, None);
    assert_created(&t.note(&with_prior(note_body(&A, &C, 7, ts), &A, &A, &C, 0, Some(0))).await);
    assert_eq!(t.total(&A, &C).await, Some(7));
}

#[tokio::test]
async fn the_prior_acknowledgment_itself_is_refused_as_a_note() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 10, at_residue(base, 0))).await);
    let ts = at_residue(base + 2_000, 997);
    // The issuer's acknowledgment of P = 10 is itself a valid note message at 10 and ts.
    let ack = note_body(&A, &B, 10, ts);
    assert_eq!(ack["signature"].as_str().unwrap().len(), 130);
    assert_refused(&t.note(&ack).await, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    let mut with_itself = ack.clone();
    with_itself["prior_signature"] = ack["signature"].clone();
    with_itself["prior_total"] = json!(10);
    assert_refused(&t.note(&with_itself).await, StatusCode::BAD_REQUEST, Some("not_an_increase"));
    assert_eq!(t.total(&A, &B).await, Some(10));
}

#[tokio::test]
async fn a_prior_on_an_ordinary_note_or_a_malformed_prior_is_an_invalid_request() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 10, at_residue(base, 0))).await);
    let ts = at_residue(base + 2_000, 500);
    let r = t.note(&with_prior(note_body(&A, &B, 20, ts), &A, &A, &B, 10, Some(10))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_request"));
    let mut body = note_body(&A, &B, 20, ts);
    body["prior_signature"] = json!("not hex");
    let r = t.note(&body).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_request"));
    assert_eq!(r.1["error"], "prior_signature must be hex-encoded");
    body["prior_signature"] = json!("00".repeat(64));
    let r = t.note(&body).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_request"));
    assert_eq!(r.1["error"], "prior_signature must be 65 bytes");
    let mut body = note_body(&A, &B, 20, ts);
    body["prior_total"] = json!(10);
    assert_refused(&t.note(&body).await, StatusCode::BAD_REQUEST, Some("invalid_request"));
    assert_eq!(t.total(&A, &B).await, Some(10));
    // The same note without the prior applies.
    assert_created(&t.note(&note_body(&A, &B, 20, ts)).await);
}

#[tokio::test]
async fn on_a_bound_only_edge_unbound_notes_are_refused_and_bound_notes_accepted() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 10, at_residue(base, 0))).await);
    assert_created(&t.note(&note_body(&B, &A, 4, at_residue(base, 1))).await);
    assert_created(&t.note(&note_body(&A, &C, 2, at_residue(base, 2))).await);
    // A bilateral set-off (residue 998) lowers A -> B and B -> A by 4; both become bound-only.
    let ts = at_residue(base + 3_000, 998);
    let edge = |issuer: &[u8; 32], recipient: &[u8; 32], new_total: u64| {
        json!({
            "issuer_pubkey": hex::encode(pk(issuer)),
            "recipient_pubkey": hex::encode(pk(recipient)),
            "decrease": 4,
            "issuer_signature": sign(issuer, issuer, recipient, new_total, ts),
            "recipient_signature": sign(recipient, issuer, recipient, new_total, ts),
        })
    };
    let setoff = json!({"denomination": "erg", "timestamp": ts, "edges": [edge(&A, &B, 6), edge(&B, &A, 0)]});
    let (s, v) = t.post("/notes/setoff", &setoff).await;
    assert_eq!(s, StatusCode::OK, "{}", v);
    assert_eq!((t.total(&A, &B).await, t.total(&B, &A).await), (Some(6), Some(0)));
    // ts ends in 998, so ts + 2 ends in 000 (ordinary) and ts + 999 in 997 (bound note).
    for (issuer, recipient, total) in [(&A, &B, 7), (&B, &A, 1)] {
        let r = t.note(&note_body(issuer, recipient, total, ts + 2)).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, Some("bound_note_required"));
    }
    assert_eq!((t.total(&A, &B).await, t.total(&B, &A).await), (Some(6), Some(0)));
    assert_created(&t.note(&with_prior(note_body(&A, &B, 7, ts + 999), &A, &A, &B, 6, Some(6))).await);
    assert_created(&t.note(&with_prior(note_body(&B, &A, 1, ts + 999), &B, &B, &A, 0, None)).await);
    assert_eq!((t.total(&A, &B).await, t.total(&B, &A).await), (Some(7), Some(1)));
    // The edge the set-off did not touch still takes ordinary notes.
    assert_created(&t.note(&note_body(&A, &C, 3, ts + 2)).await);
    assert_eq!(t.total(&A, &C).await, Some(3));
}

#[tokio::test]
async fn existing_refusals_get_codes_and_keep_their_messages() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    let ts0 = at_residue(base, 0);
    assert_created(&t.note(&note_body(&A, &B, 10, ts0)).await);
    let r = t.note(&note_body(&A, &B, 20, ts0 - 1_000)).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("past_timestamp"));
    assert_eq!(r.1["error"], "Past timestamp");
    let r = t.note(&note_body(&A, &B, 20, at_residue(now_ms() + 3_600_000, 0))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("future_timestamp"));
    assert_eq!(r.1["error"], "Future timestamp");
    let mut forged = note_body(&A, &B, 20, ts0 + 100);
    forged["signature"] = note_body(&A, &B, 21, ts0 + 100)["signature"].clone();
    let r = t.note(&forged).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_signature"));
    assert_eq!(r.1["error"], "Invalid signature");
    let mut other_tracker = note_body(&A, &B, 20, ts0 + 100);
    other_tracker["tracker_nft_id"] = json!("11".repeat(32));
    let r = t.note(&other_tracker).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("tracker_id_mismatch"));
    assert!(r.1["error"].as_str().unwrap().starts_with("Note is bound to tracker NFT"), "{}", r.1);
    // Refusals without a code keep their earlier response exactly.
    let r = t.note(&note_body(&A, &B, 5, ts0 + 100)).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, None);
    assert!(r.1["error"].as_str().unwrap().starts_with("Cumulative debt cannot decrease"), "{}", r.1);
    let mut bad_hex = note_body(&A, &B, 20, ts0 + 100);
    bad_hex["recipient_pubkey"] = json!("zz");
    assert_eq!(
        t.post_raw("/notes", &bad_hex).await,
        (StatusCode::BAD_REQUEST, r#"{"success":false,"data":null,"error":"recipient_pubkey must be hex-encoded"}"#.to_string())
    );
    assert_eq!(t.total(&A, &B).await, Some(10));
}

#[tokio::test]
async fn a_request_without_the_new_fields_gets_the_same_response_as_before() {
    let t = tracker().await;
    let ts = at_residue(now_ms() - 60_000, 0);
    assert_eq!(
        t.post_raw("/notes", &note_body(&A, &B, 10, ts)).await,
        (StatusCode::CREATED, r#"{"success":true,"data":null,"error":null}"#.to_string())
    );
    assert_eq!(t.total(&A, &B).await, Some(10));
}
