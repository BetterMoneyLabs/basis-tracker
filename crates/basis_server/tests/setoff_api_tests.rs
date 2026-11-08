//! POST /notes/setoff on the v3 tracker (set-off version 3): residue 998,
//! per-edge timestamps (an edge may carry its own; the request's timestamp is
//! the default), the one-hour maximum age for every edge, a strict request
//! (unknown fields refused), bound-only marks on every lowered edge and a
//! status and machine-readable `code` for every store refusal. Every request
//! goes through the axum router and a tracker thread backed by a real store.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use basis_server::{api, store::EventStore, AppState, TrackerCommand};
use basis_store::{DenomKey, IouNote, TrackerStateManager};
use secp256k1::{Secp256k1, SecretKey};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

const NFT_HEX: &str = "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b";
const OTHER_NFT: [u8; 32] = [0x43; 32];
const TOKEN: [u8; 32] = [0xa5; 32];
const A: [u8; 32] = [1u8; 32];
const B: [u8; 32] = [2u8; 32];
const C: [u8; 32] = [3u8; 32];
const D: [u8; 32] = [4u8; 32];

/// The maximum novation age (basis_store::NOVATION_MAX_AGE_MS).
const HOUR_MS: u64 = 3_600_000;

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

/// `signer`'s signature over the note message (issuer -> recipient, `denomination`, `total`, `ts`, `nft`).
fn sign_raw(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], denomination: &DenomKey, total: u64, ts: u64, nft: &[u8; 32]) -> [u8; 65] {
    let msg = basis_store::schnorr::signing_message(&pk(issuer), &pk(recipient), denomination, total, ts, nft);
    basis_store::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
}

/// `signer`'s signature (hex) over the ERG note message (issuer -> recipient, `total`, `ts`).
fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> String {
    hex::encode(sign_raw(signer, issuer, recipient, &DenomKey::Erg, total, ts, &nft()))
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

/// A set-off edge lowering issuer -> recipient from `before` by `decrease`, signed by both parties at `ts`,
/// WITHOUT its own timestamp: the request's timestamp must be `ts`.
fn shared(issuer: &[u8; 32], recipient: &[u8; 32], before: u64, decrease: u64, ts: u64) -> Value {
    let new_total = before.saturating_sub(decrease);
    json!({
        "issuer_pubkey": pk_hex(issuer),
        "recipient_pubkey": pk_hex(recipient),
        "decrease": decrease,
        "issuer_signature": sign(issuer, issuer, recipient, new_total, ts),
        "recipient_signature": sign(recipient, issuer, recipient, new_total, ts),
    })
}

/// The same edge carrying its own timestamp `ts` (per-edge timestamps).
fn own(issuer: &[u8; 32], recipient: &[u8; 32], before: u64, decrease: u64, ts: u64) -> Value {
    with(&shared(issuer, recipient, before, decrease, ts), "timestamp", json!(ts))
}

/// A POST /notes/setoff body in ERG whose (default) timestamp is `ts`.
fn setoff_body(ts: u64, edges: Vec<Value>) -> Value {
    json!({"denomination": "erg", "timestamp": ts, "edges": edges})
}

/// The POST /notes/transfer body moving `amount` of A's ERG debt from A->old (stored total `from_before`) to
/// A->new (stored total `to_before`, 0 when absent) at `ts`, with all seven signatures.
fn transfer_body(old: &[u8; 32], new: &[u8; 32], from_before: u64, to_before: u64, amount: u64, ts: u64) -> Value {
    let (from_total, to_total) = (from_before.saturating_sub(amount), to_before + amount);
    json!({
        "issuer_pubkey": pk_hex(&A),
        "from_recipient_pubkey": pk_hex(old),
        "to_recipient_pubkey": pk_hex(new),
        "denomination": "erg",
        "amount": amount,
        "timestamp": ts,
        "issuer_from_signature": sign(&A, &A, old, from_total, ts),
        "issuer_to_signature": sign(&A, &A, new, to_total, ts),
        "from_recipient_prior_signature": sign(old, &A, old, from_before, ts),
        "from_recipient_signature": sign(old, &A, old, from_total, ts),
        "from_recipient_to_signature": sign(old, &A, new, to_total, ts),
        "to_recipient_prior_signature": sign(new, &A, new, to_before, ts),
        "to_recipient_signature": sign(new, &A, new, to_total, ts),
    })
}

/// `body` with `field` set to `value`.
fn with(body: &Value, field: &str, value: Value) -> Value {
    let mut b = body.clone();
    b[field] = value;
    b
}

struct Tracker {
    app: axum::Router,
    state: AppState,
    _dirs: Vec<tempfile::TempDir>,
}

/// A router with POST /notes, /notes/transfer and /notes/setoff over a tracker thread with a fresh store.
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
                TrackerCommand::ExchangeNotes {
                    issuer_pubkey,
                    recipient_pubkey,
                    from_denomination,
                    from_amount,
                    to_denomination,
                    to_amount,
                    timestamp,
                    issuer_from_signature,
                    recipient_from_signature,
                    issuer_to_signature,
                    recipient_to_signature,
                    from_reserve_box_id,
                    acceptance_check,
                    response_tx,
                } => {
                    let _ = response_tx.send(tracker.exchange_debt_checked(
                        &issuer_pubkey,
                        &recipient_pubkey,
                        &from_denomination,
                        from_amount,
                        &to_denomination,
                        to_amount,
                        timestamp,
                        &issuer_from_signature,
                        &recipient_from_signature,
                        &issuer_to_signature,
                        &recipient_to_signature,
                        &from_reserve_box_id,
                        acceptance_check.map(|check| check.0),
                    ));
                }
                TrackerCommand::MarkNotesPending { digest, tx_id, submitted_height, response_tx } => {
                    let _ = response_tx.send(tracker.mark_notes_pending(digest, &tx_id, submitted_height));
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
        .route("/notes/transfer", axum::routing::post(api::transfer_notes))
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
        (status, serde_json::from_str(&raw).unwrap_or_else(|_| panic!("not JSON ({}): {}", status, raw)))
    }

    async fn note(&self, body: &Value) -> (StatusCode, Value) {
        self.post("/notes", body).await
    }

    async fn setoff(&self, body: &Value) -> (StatusCode, Value) {
        self.post("/notes/setoff", body).await
    }

    async fn transfer(&self, body: &Value) -> (StatusCode, Value) {
        self.post("/notes/transfer", body).await
    }

    /// The stored ERG total of issuer -> recipient, if the edge exists.
    async fn total(&self, issuer: &[u8; 32], recipient: &[u8; 32]) -> Option<u64> {
        self.stored(issuer, recipient).await.map(|n| n.amount_collected)
    }

    /// The stored ERG note of issuer -> recipient, if the edge exists.
    async fn stored(&self, issuer: &[u8; 32], recipient: &[u8; 32]) -> Option<IouNote> {
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
        rx.await.unwrap().unwrap()
    }

    /// The triangle's ERG totals (A -> B, B -> C, C -> A).
    async fn triangle_totals(&self) -> (Option<u64>, Option<u64>, Option<u64>) {
        (self.total(&A, &B).await, self.total(&B, &C).await, self.total(&C, &A).await)
    }

    /// The stored ERG totals of the given (issuer, recipient) edges, in order.
    async fn totals_of(&self, edges: &[([u8; 32], [u8; 32])]) -> Vec<Option<u64>> {
        let mut out = Vec::with_capacity(edges.len());
        for (issuer, recipient) in edges {
            out.push(self.total(issuer, recipient).await);
        }
        out
    }

    /// Add a note straight to the store, bypassing POST /notes and its checks of the request.
    async fn add_note_directly(&self, issuer: &[u8; 32], note: IouNote) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.state
            .tx
            .send(TrackerCommand::AddNote { issuer_pubkey: pk(issuer), note, prior: None, response_tx: tx })
            .await
            .unwrap();
        rx.await.unwrap().unwrap();
    }
}

/// Assert an error response: status, `code` (or no `code` key at all) and `success: false`. The old catch-all
/// ("Set-off rejected: <Debug>") must never appear.
fn assert_refused(response: &(StatusCode, Value), status: StatusCode, code: Option<&str>) {
    let (s, v) = response;
    assert_eq!(*s, status, "response: {}", v);
    assert_eq!(v["success"], false, "response: {}", v);
    match code {
        Some(c) => assert_eq!(v["code"], c, "response: {}", v),
        None => assert!(v.get("code").is_none(), "unexpected code in {}", v),
    }
    assert!(!v["error"].as_str().unwrap_or("").contains("rejected:"), "response: {}", v);
}

fn assert_created(response: &(StatusCode, Value)) {
    let (s, v) = response;
    assert_eq!(*s, StatusCode::CREATED, "response: {}", v);
    assert_eq!(v["success"], true);
}

fn assert_cleared(response: &(StatusCode, Value)) {
    let (s, v) = response;
    assert_eq!(*s, StatusCode::OK, "response: {}", v);
    assert_eq!(v["success"], true, "response: {}", v);
    assert_eq!(v["data"]["cleared"], true, "response: {}", v);
}

/// A tracker where A owes B 10, B owes C 7 and C owes A 5 (ERG), noted about a minute ago.
async fn triangle() -> (Tracker, u64) {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 10, at_residue(base, 0))).await);
    assert_created(&t.note(&note_body(&B, &C, 7, at_residue(base, 1))).await);
    assert_created(&t.note(&note_body(&C, &A, 5, at_residue(base, 2))).await);
    (t, base)
}

#[tokio::test]
async fn a_moved_edge_is_re_signed_alone_and_every_edge_echoes_its_own_timestamp() {
    let (t, base) = triangle().await;
    let t1 = at_residue(base + 2_000, 998);
    // Everyone signs the cycle at t1. Before it is submitted, a new deal raises B -> C from 7 to 9 (after t1; it
    // ends in 498 ms, an ordinary note).
    let signed = setoff_body(t1, vec![shared(&A, &B, 10, 5, t1), shared(&B, &C, 7, 5, t1), shared(&C, &A, 5, 5, t1)]);
    assert_created(&t.note(&note_body(&B, &C, 9, t1 + 500)).await);
    assert_refused(&t.setoff(&signed).await, StatusCode::BAD_REQUEST, Some("past_timestamp"));
    assert_eq!(t.triangle_totals().await, (Some(10), Some(9), Some(5)));
    // B and C re-sign B -> C alone at a newer t2 and the new total 4; A -> B keeps t1 (stated) and C -> A keeps
    // it by default.
    let t2 = t1 + 1_000;
    let body = setoff_body(t1, vec![own(&A, &B, 10, 5, t1), own(&B, &C, 9, 5, t2), shared(&C, &A, 5, 5, t1)]);
    let (status, raw) = t.post_raw("/notes/setoff", &body).await;
    assert_eq!(status, StatusCode::OK, "{}", raw);
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v,
        json!({"success": true, "error": null, "data": {
            "cleared": true, "denomination": "erg", "timestamp": t1, "total_cleared": 15,
            "edges": [
                {"issuer_pubkey": pk_hex(&A), "recipient_pubkey": pk_hex(&B), "new_total": 5, "timestamp": t1},
                {"issuer_pubkey": pk_hex(&B), "recipient_pubkey": pk_hex(&C), "new_total": 4, "timestamp": t2},
                {"issuer_pubkey": pk_hex(&C), "recipient_pubkey": pk_hex(&A), "new_total": 0, "timestamp": t1},
            ],
        }})
    );
    assert_eq!(t.triangle_totals().await, (Some(5), Some(4), Some(0)));
    // Each edge is stored at its own timestamp, with its issuer's signature.
    for (issuer, recipient, ts) in [(&A, &B, t1), (&B, &C, t2), (&C, &A, t1)] {
        let n = t.stored(issuer, recipient).await.unwrap();
        assert_eq!(n.timestamp, ts);
        assert!(n.verify_signature(&pk(issuer)).is_ok());
    }
    // The NoteUpdated events carry each edge's own timestamp.
    let events = t.state.event_store.get_events_paginated(0, 1000).await.unwrap();
    let last: Vec<(String, String, Option<u64>, u64)> = events[events.len() - 3..]
        .iter()
        .map(|e| {
            assert!(matches!(e.event_type, basis_server::models::EventType::NoteUpdated));
            (e.issuer_pubkey.clone().unwrap(), e.recipient_pubkey.clone().unwrap(), e.amount, e.timestamp)
        })
        .collect();
    assert_eq!(
        last,
        vec![
            (pk_hex(&A), pk_hex(&B), Some(5), t1),
            (pk_hex(&B), pk_hex(&C), Some(4), t2),
            (pk_hex(&C), pk_hex(&A), Some(0), t1),
        ]
    );
}

#[tokio::test]
async fn edges_without_their_own_timestamp_use_the_requests() {
    // A version-2 shaped request (no edge timestamps; one null) applies at the request's timestamp.
    let (t, base) = triangle().await;
    let ts = at_residue(base + 2_000, 998);
    let body = setoff_body(ts, vec![shared(&A, &B, 10, 5, ts), with(&shared(&B, &C, 7, 5, ts), "timestamp", Value::Null), shared(&C, &A, 5, 5, ts)]);
    let r = t.setoff(&body).await;
    assert_cleared(&r);
    assert_eq!(r.1["data"]["timestamp"], json!(ts));
    let stamps: Vec<Value> = r.1["data"]["edges"].as_array().unwrap().iter().map(|e| e["timestamp"].clone()).collect();
    assert_eq!(stamps, vec![json!(ts), json!(ts), json!(ts)]);
    assert_eq!(t.triangle_totals().await, (Some(5), Some(2), Some(0)));

    // The request's timestamp is only that default: when every edge carries its own, the request's is not used.
    let (t, base) = triangle().await;
    let ts = at_residue(base + 2_000, 998);
    let stale_default = at_residue(now_ms() - 2 * HOUR_MS, 0);
    let body = setoff_body(stale_default, vec![own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, ts)]);
    let r = t.setoff(&body).await;
    assert_cleared(&r);
    assert_eq!(r.1["data"]["timestamp"], json!(stale_default));
    assert_eq!(r.1["data"]["edges"][1]["timestamp"], json!(ts));
    assert_eq!(t.triangle_totals().await, (Some(5), Some(2), Some(0)));
}

#[tokio::test]
async fn unknown_fields_are_refused_on_the_request_and_on_every_edge() {
    let (t, base) = triangle().await;
    let ts = at_residue(base + 2_000, 998);
    let body = setoff_body(ts, vec![own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, ts)]);
    // deny_unknown_fields: a field this tracker does not know is refused, never silently dropped.
    for extra in ["extra", "timestamps", "prior_total"] {
        let (status, raw) = t.post_raw("/notes/setoff", &with(&body, extra, json!(1))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{}: {}", extra, raw);
        assert!(raw.contains("unknown field"), "{}: {}", extra, raw);
    }
    for extra in ["prior_signature", "prior_total", "new_total", "ts"] {
        let mut b = body.clone();
        b["edges"][1][extra] = json!(1);
        let (status, raw) = t.post_raw("/notes/setoff", &b).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "edge {}: {}", extra, raw);
        assert!(raw.contains("unknown field"), "edge {}: {}", extra, raw);
    }
    // The request's timestamp stays required.
    let mut no_timestamp = body.clone();
    no_timestamp.as_object_mut().unwrap().remove("timestamp");
    let (status, raw) = t.post_raw("/notes/setoff", &no_timestamp).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{}", raw);
    assert!(raw.contains("missing field"), "{}", raw);
    assert_eq!(t.triangle_totals().await, (Some(10), Some(7), Some(5)));
    // The complete request applies.
    assert_cleared(&t.setoff(&body).await);
    assert_eq!(t.triangle_totals().await, (Some(5), Some(2), Some(0)));
}

#[tokio::test]
async fn store_refusals_map_to_statuses_and_codes() {
    let (t, base) = triangle().await;
    let ts = at_residue(base + 2_000, 998);
    let cycle = |a: Value, b: Value, c: Value| setoff_body(ts, vec![a, b, c]);
    let ok = cycle(own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, ts));
    let mut forged = ok.clone();
    forged["edges"][0]["issuer_signature"] = ok["edges"][1]["issuer_signature"].clone();
    let at = |ms: u64| at_residue(base + 2_000, ms);
    let cases: Vec<(&str, Value, StatusCode, &str)> = vec![
        ("a single edge", setoff_body(ts, vec![own(&A, &B, 10, 5, ts)]), StatusCode::BAD_REQUEST, "invalid_setoff"),
        ("unbalanced", cycle(own(&A, &B, 10, 5, ts), own(&B, &C, 7, 4, ts), own(&C, &A, 5, 5, ts)), StatusCode::BAD_REQUEST, "invalid_setoff"),
        ("an edge at the transfer residue", cycle(own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, at(999)), own(&C, &A, 5, 5, ts)), StatusCode::BAD_REQUEST, "invalid_setoff"),
        ("an edge at the bound-note residue", cycle(own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, at(997))), StatusCode::BAD_REQUEST, "invalid_setoff"),
        ("an ordinary millisecond", setoff_body(at(0), vec![shared(&A, &B, 10, 5, at(0)), shared(&B, &C, 7, 5, at(0)), shared(&C, &A, 5, 5, at(0))]), StatusCode::BAD_REQUEST, "invalid_setoff"),
        ("more than an edge's total", cycle(own(&A, &B, 10, 6, ts), own(&B, &C, 7, 6, ts), own(&C, &A, 5, 6, ts)), StatusCode::BAD_REQUEST, "invalid_setoff"),
        ("a signature that does not verify", forged, StatusCode::BAD_REQUEST, "invalid_signature"),
        ("an edge not newer than its note", cycle(own(&A, &B, 10, 5, at_residue(base - 5_000, 998)), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, ts)), StatusCode::BAD_REQUEST, "past_timestamp"),
        ("an edge in the future", cycle(own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, at_residue(now_ms() + HOUR_MS, 998)), own(&C, &A, 5, 5, ts)), StatusCode::BAD_REQUEST, "future_timestamp"),
        ("an edge older than an hour", cycle(own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, at_residue(now_ms() - HOUR_MS - 1_000, 998))), StatusCode::BAD_REQUEST, "novation_expired"),
        ("an edge that does not exist", setoff_body(ts, vec![own(&A, &D, 10, 5, ts), own(&D, &A, 10, 5, ts)]), StatusCode::NOT_FOUND, "not_found"),
        ("more than 64 edges", setoff_body(ts, vec![own(&A, &B, 10, 1, ts); 65]), StatusCode::BAD_REQUEST, "invalid_setoff"),
    ];
    for (name, body, status, code) in &cases {
        let r = t.setoff(body).await;
        assert_eq!(r.0, *status, "{}: {}", name, r.1);
        assert_refused(&r, *status, Some(*code));
    }
    // Messages are plain text, never a Debug rendering.
    for (body, message) in [
        (cases[0].1.clone(), "Invalid set-off: a set-off lowers at least two edges"),
        (cases[2].1.clone(), "Invalid set-off: every edge's timestamp must end in 998 ms (the set-off timestamp residue)"),
        (cases[6].1.clone(), "Invalid signature"),
        (cases[7].1.clone(), "every edge's timestamp must be newer than that edge's current timestamp"),
        (cases[8].1.clone(), "timestamp is in the future"),
        (cases[9].1.clone(), "the novation's timestamp is older than the maximum novation age"),
        (cases[10].1.clone(), "one of the edges does not exist in this denomination"),
        (cases[11].1.clone(), "a set-off lowers at most 64 edges"),
    ] {
        assert_eq!(t.setoff(&body).await.1["error"], message);
    }
    assert_eq!(t.triangle_totals().await, (Some(10), Some(7), Some(5)));

    // Edges bound to different tracker NFTs (B -> A is stored directly: POST /notes refuses a foreign NFT).
    let foreign = IouNote::create_and_sign(pk(&A), 10, at_residue(base, 3), &B, &OTHER_NFT, DenomKey::Erg).unwrap();
    t.add_note_directly(&B, foreign).await;
    let r = t.setoff(&setoff_body(ts, vec![own(&A, &B, 10, 4, ts), own(&B, &A, 10, 4, ts)])).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("tracker_id_mismatch"));
    assert_eq!(r.1["error"], "the edges are bound to different tracker NFTs");

    // The valid set-off still applies on this tracker.
    assert_cleared(&t.setoff(&ok).await);
    assert_eq!(t.triangle_totals().await, (Some(5), Some(2), Some(0)));
}

#[tokio::test]
async fn a_staged_exchange_is_a_conflict() {
    let (t, base) = triangle().await;
    // An ERG -> TOKEN exchange on A's pair with B, sent straight to the tracker thread: its ERG decrease of A->B
    // (10 -> 5) applies, and its TOKEN increase stays staged.
    let token = DenomKey::Token(TOKEN);
    let token_note = IouNote::create_and_sign(pk(&B), 5, at_residue(base, 3), &A, &nft(), token).unwrap();
    t.add_note_directly(&A, token_note).await;
    let xts = at_residue(base + 1_000, 500);
    let (response_tx, rx) = tokio::sync::oneshot::channel();
    t.state
        .tx
        .send(TrackerCommand::ExchangeNotes {
            issuer_pubkey: pk(&A),
            recipient_pubkey: pk(&B),
            from_denomination: DenomKey::Erg,
            from_amount: 5,
            to_denomination: token,
            to_amount: 5,
            timestamp: xts,
            issuer_from_signature: sign_raw(&A, &A, &B, &DenomKey::Erg, 5, xts, &nft()),
            recipient_from_signature: sign_raw(&B, &A, &B, &DenomKey::Erg, 5, xts, &nft()),
            issuer_to_signature: sign_raw(&A, &A, &B, &token, 10, xts, &nft()),
            recipient_to_signature: sign_raw(&B, &A, &B, &token, 10, xts, &nft()),
            from_reserve_box_id: "reserve_box".to_string(),
            acceptance_check: None,
            response_tx,
        })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();
    assert_eq!(t.triangle_totals().await, (Some(5), Some(7), Some(5)));
    let ts = at_residue(base + 2_000, 998);
    let r = t.setoff(&setoff_body(ts, vec![own(&A, &B, 5, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, ts)])).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("staged_exchange"));
    assert_eq!(r.1["error"], "an exchange on one of these pairs is staged");
    assert_eq!(t.triangle_totals().await, (Some(5), Some(7), Some(5)));
}

#[tokio::test]
async fn an_edge_with_on_chain_state_is_out_of_scope() {
    let (t, base) = triangle().await;
    let (response_tx, rx) = tokio::sync::oneshot::channel();
    t.state
        .tx
        .send(TrackerCommand::MarkNotesPending { digest: [0u8; 33], tx_id: "tx1".to_string(), submitted_height: 100, response_tx })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap().unwrap(), 3);
    let ts = at_residue(base + 2_000, 998);
    let r = t.setoff(&setoff_body(ts, vec![own(&A, &B, 10, 5, ts), own(&B, &C, 7, 5, ts), own(&C, &A, 5, 5, ts)])).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("onchain_scope"));
    assert!(r.1["error"].as_str().unwrap().starts_with("an edge has a committed or pending on-chain total"), "{}", r.1);
    assert_eq!(t.triangle_totals().await, (Some(10), Some(7), Some(5)));
}

#[tokio::test]
async fn set_off_and_transfer_signatures_do_not_cross_routes() {
    // A owes B 100 and B owes A 60. A transfer of 60 from A->B to A->C at T (999 ms) has A and B sign
    // M(A,B,40,T); a set-off lowering A->B and B->A by 30 at S (998 ms) has A and B sign M(A,B,70,S).
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 100, at_residue(base, 0))).await);
    assert_created(&t.note(&note_body(&B, &A, 60, at_residue(base, 1))).await);
    let tt = at_residue(base + 2_000, 999);
    let s = tt + 999; // ends in 998 ms, after T
    let transfer = transfer_body(&B, &C, 100, 0, 60, tt);
    let setoff = setoff_body(s, vec![shared(&A, &B, 100, 30, s), shared(&B, &A, 60, 30, s)]);
    let edges = [(A, B), (B, A), (A, C)];

    // The transfer's A->B signatures as a set-off edge (at T), balanced by B->A signed at T or at 998 ms.
    for back_ts in [tt, s] {
        let from_edge = json!({
            "issuer_pubkey": pk_hex(&A),
            "recipient_pubkey": pk_hex(&B),
            "decrease": 60,
            "timestamp": tt,
            "issuer_signature": transfer["issuer_from_signature"].clone(),
            "recipient_signature": transfer["from_recipient_signature"].clone(),
        });
        let r = t.setoff(&setoff_body(s, vec![from_edge, own(&B, &A, 60, 60, back_ts)])).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_setoff"));
    }
    // The set-off's A->B signatures as a transfer's from-edge signatures at S (a transfer of 30), every other slot
    // validly signed at S.
    let mut at_s = transfer_body(&B, &C, 100, 0, 30, s);
    at_s["issuer_from_signature"] = setoff["edges"][0]["issuer_signature"].clone();
    at_s["from_recipient_signature"] = setoff["edges"][0]["recipient_signature"].clone();
    let r = t.transfer(&at_s).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_transfer"));
    assert_eq!(t.totals_of(&edges).await, vec![Some(100), Some(60), None]);

    // The genuine transfer applies at T. A's set-off signature M(A,B,70,S) would now RAISE A->B from 40 to 70 at
    // a newer timestamp: POST /notes refuses its residue. The set-off itself no longer verifies (40 - 30 != 70).
    let r = t.transfer(&transfer).await;
    assert_eq!(r.0, StatusCode::OK, "{}", r.1);
    assert_eq!(t.totals_of(&edges).await, vec![Some(40), Some(60), Some(60)]);
    let as_note = with(&note_body(&A, &B, 70, s), "signature", setoff["edges"][0]["issuer_signature"].clone());
    assert_refused(&t.note(&as_note).await, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    assert_refused(&t.setoff(&setoff).await, StatusCode::BAD_REQUEST, Some("invalid_signature"));
    assert_eq!(t.totals_of(&edges).await, vec![Some(40), Some(60), Some(60)]);
}

#[tokio::test]
async fn every_lowered_edge_is_bound_only_after_a_set_off_with_per_edge_timestamps() {
    let (t, base) = triangle().await;
    assert_created(&t.note(&note_body(&A, &C, 2, at_residue(base, 3))).await);
    let t1 = at_residue(base + 2_000, 998);
    let t2 = t1 + 1_000;
    let body = setoff_body(t1, vec![own(&A, &B, 10, 5, t1), own(&B, &C, 7, 5, t2), own(&C, &A, 5, 5, t1)]);
    assert_cleared(&t.setoff(&body).await);
    // t2 ends in 998, so t2 + 2 ends in 000 (ordinary) and t2 + 999 in 997 (bound note).
    for (issuer, recipient, total) in [(&A, &B, 6), (&B, &C, 5), (&C, &A, 1)] {
        let r = t.note(&note_body(issuer, recipient, total, t2 + 2)).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, Some("bound_note_required"));
    }
    assert_eq!(t.triangle_totals().await, (Some(5), Some(2), Some(0)));
    for (issuer, recipient, prior, total) in [(&A, &B, 5, 6), (&B, &C, 2, 5), (&C, &A, 0, 1)] {
        let bound = with(&note_body(issuer, recipient, total, t2 + 999), "prior_signature", json!(sign(issuer, issuer, recipient, prior, t2 + 999)));
        assert_created(&t.note(&bound).await);
    }
    assert_eq!(t.triangle_totals().await, (Some(6), Some(5), Some(1)));
    // The edge the set-off did not touch still takes ordinary notes.
    assert_created(&t.note(&note_body(&A, &C, 3, t2 + 2)).await);
    assert_eq!(t.total(&A, &C).await, Some(3));
}
