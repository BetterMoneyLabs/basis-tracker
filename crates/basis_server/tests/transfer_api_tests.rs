//! POST /notes/transfer on the v3 tracker (transfer version 3): seven
//! signatures including both creditors' prior acknowledgments, the one-hour
//! maximum age, a strict request (unknown fields refused) and a status and
//! machine-readable `code` for every store refusal. Every request goes through
//! the axum router and a tracker thread backed by a real store.

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
const E: [u8; 32] = [5u8; 32];

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
        "recipient_pubkey": hex::encode(pk(recipient)),
        "amount": total,
        "timestamp": ts,
        "signature": sign(issuer, issuer, recipient, total, ts),
        "issuer_pubkey": hex::encode(pk(issuer)),
        "tracker_nft_id": NFT_HEX,
        "denomination": "erg",
    })
}

/// The POST /notes/transfer body moving `amount` of A's ERG debt from A->old (stored total `from_before`) to
/// A->new (stored total `to_before`, 0 when absent) at `ts`, with all seven signatures and no diagnostic priors.
fn transfer_body(old: &[u8; 32], new: &[u8; 32], from_before: u64, to_before: u64, amount: u64, ts: u64) -> Value {
    let (from_total, to_total) = (from_before.saturating_sub(amount), to_before + amount);
    json!({
        "issuer_pubkey": hex::encode(pk(&A)),
        "from_recipient_pubkey": hex::encode(pk(old)),
        "to_recipient_pubkey": hex::encode(pk(new)),
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
    tracker_with(None).await
}

async fn tracker_with(acceptance_predicate: Option<Arc<dyn basis_server::acceptance::NotePredicate>>) -> Tracker {
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
        acceptance_predicate,
        policy_storage,
        pending_redemptions: Arc::new(basis_server::redemption_build::PendingRedemptionRegistry::new()),
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

    async fn transfer(&self, body: &Value) -> (StatusCode, Value) {
        self.post("/notes/transfer", body).await
    }

    /// The stored total of issuer -> recipient in `denomination`, if the edge exists.
    async fn total_in(&self, issuer: &[u8; 32], recipient: &[u8; 32], denomination: DenomKey) -> Option<u64> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.state
            .tx
            .send(TrackerCommand::GetNoteByIssuerAndRecipient {
                issuer_pubkey: pk(issuer),
                recipient_pubkey: pk(recipient),
                denomination,
                response_tx: tx,
            })
            .await
            .unwrap();
        rx.await.unwrap().unwrap().map(|n: IouNote| n.amount_collected)
    }

    /// The stored ERG total of issuer -> recipient, if the edge exists.
    async fn total(&self, issuer: &[u8; 32], recipient: &[u8; 32]) -> Option<u64> {
        self.total_in(issuer, recipient, DenomKey::Erg).await
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
/// ("Transfer rejected: <Debug>") must never appear.
fn assert_refused(response: &(StatusCode, Value), status: StatusCode, code: Option<&str>) {
    let (s, v) = response;
    assert_eq!(*s, status, "response: {}", v);
    assert_eq!(v["success"], false, "response: {}", v);
    match code {
        Some(c) => assert_eq!(v["code"], c, "response: {}", v),
        None => assert!(v.get("code").is_none(), "unexpected code in {}", v),
    }
    assert!(!v["error"].as_str().unwrap_or("").contains("Transfer rejected"), "response: {}", v);
}

fn assert_created(response: &(StatusCode, Value)) {
    let (s, v) = response;
    assert_eq!(*s, StatusCode::CREATED, "response: {}", v);
    assert_eq!(v["success"], true);
}

fn assert_transferred(response: &(StatusCode, Value)) {
    let (s, v) = response;
    assert_eq!(*s, StatusCode::OK, "response: {}", v);
    assert_eq!(v["success"], true, "response: {}", v);
    assert_eq!(v["data"]["transferred"], true, "response: {}", v);
}

/// A tracker where A owes B 100 and C 10 (ERG), noted about a minute ago.
async fn funded() -> (Tracker, u64) {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 100, at_residue(base, 0))).await);
    assert_created(&t.note(&note_body(&A, &C, 10, at_residue(base, 1))).await);
    (t, base)
}

#[tokio::test]
async fn a_transfer_with_seven_signatures_applies_and_reports_the_prior_totals() {
    let (t, base) = funded().await;
    let ts = at_residue(base + 2_000, 999);
    let mut body = transfer_body(&B, &C, 100, 10, 30, ts);
    body["from_prior_total"] = json!(100);
    body["to_prior_total"] = json!(10);
    let (status, raw) = t.post_raw("/notes/transfer", &body).await;
    assert_eq!(status, StatusCode::OK, "{}", raw);
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v,
        json!({"success": true, "error": null, "data": {
            "transferred": true, "denomination": "erg", "timestamp": ts,
            "from_new_total": 70, "to_new_total": 40, "from_prior_total": 100, "to_prior_total": 10,
        }})
    );
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(70), Some(40)));
}

#[tokio::test]
async fn an_unknown_field_is_refused_and_a_version_2_request_is_incomplete() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 100, at_residue(base, 0))).await);
    let ts = at_residue(base + 2_000, 999);
    let body = transfer_body(&B, &C, 100, 0, 30, ts);
    // deny_unknown_fields: a field this tracker does not know is refused, never silently dropped.
    for extra in ["extra", "prior_signature", "from_recipient_prior_total"] {
        let (status, raw) = t.post_raw("/notes/transfer", &with(&body, extra, json!(1))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{}: {}", extra, raw);
        assert!(raw.contains("unknown field"), "{}: {}", extra, raw);
    }
    // A version-2 request (five signatures, no prior acknowledgments) lacks required fields.
    let mut v2 = body.clone();
    v2.as_object_mut().unwrap().remove("from_recipient_prior_signature");
    v2.as_object_mut().unwrap().remove("to_recipient_prior_signature");
    let (status, raw) = t.post_raw("/notes/transfer", &v2).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{}", raw);
    assert!(raw.contains("missing field"), "{}", raw);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(100), None));
    // The denomination stays required, non-empty and served (400, as before).
    let mut no_denomination = body.clone();
    no_denomination.as_object_mut().unwrap().remove("denomination");
    for b in [no_denomination, with(&body, "denomination", json!("")), with(&body, "denomination", json!(format!("token:{}", hex::encode(TOKEN))))] {
        let r = t.transfer(&b).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, None);
        assert!(r.1["error"].as_str().unwrap().starts_with("invalid denomination"), "{}", r.1);
    }
    // A malformed prior acknowledgment is a request-format refusal (400, no code).
    let r = t.transfer(&with(&body, "to_recipient_prior_signature", json!("zz"))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, None);
    assert_eq!(r.1["error"], "to_recipient_prior_signature must be hex-encoded");
    let r = t.transfer(&with(&body, "from_recipient_prior_signature", json!("00".repeat(64)))).await;
    assert_refused(&r, StatusCode::BAD_REQUEST, None);
    assert_eq!(r.1["error"], "from_recipient_prior_signature must be 65 bytes");
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(100), None));
    // The complete request applies; the diagnostic priors may be null.
    let complete = with(&with(&body, "from_prior_total", Value::Null), "to_prior_total", Value::Null);
    assert_transferred(&t.transfer(&complete).await);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(70), Some(30)));
}

#[tokio::test]
async fn a_prior_acknowledgment_signed_by_the_wrong_key_is_refused() {
    let (t, base) = funded().await;
    let ts = at_residue(base + 2_000, 999);
    let body = transfer_body(&B, &C, 100, 10, 30, ts);
    for (slot, forged) in [
        ("from_recipient_prior_signature", sign(&A, &A, &B, 100, ts)), // the issuer signs in the old creditor's place
        ("from_recipient_prior_signature", sign(&C, &A, &B, 100, ts)), // the new creditor does
        ("to_recipient_prior_signature", sign(&B, &A, &C, 10, ts)),   // the old creditor signs in the new creditor's place
        ("to_recipient_prior_signature", sign(&A, &A, &C, 10, ts)),   // the issuer does
    ] {
        let r = t.transfer(&with(&body, slot, json!(forged))).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_signature"));
        assert_eq!(r.1["error"], "Invalid signature");
    }
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(100), Some(10)));
    assert_transferred(&t.transfer(&body).await);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(70), Some(40)));
}

#[tokio::test]
async fn an_absent_to_edge_with_a_prior_over_zero_is_accepted_and_created() {
    let t = tracker().await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 100, at_residue(base, 0))).await);
    let ts = at_residue(base + 2_000, 999);
    let body = transfer_body(&B, &C, 100, 0, 30, ts);
    // Any other prior for the absent edge is refused.
    assert_refused(&t.transfer(&with(&body, "to_prior_total", json!(5))).await, StatusCode::CONFLICT, Some("stale_prior"));
    let five = with(&body, "to_recipient_prior_signature", json!(sign(&C, &A, &C, 5, ts)));
    assert_refused(&t.transfer(&five).await, StatusCode::BAD_REQUEST, Some("invalid_signature"));
    assert_eq!(t.total(&A, &C).await, None);
    let r = t.transfer(&with(&body, "to_prior_total", json!(0))).await;
    assert_transferred(&r);
    assert_eq!((r.1["data"]["to_prior_total"].clone(), r.1["data"]["to_new_total"].clone()), (json!(0), json!(30)));
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(70), Some(30)));
}

#[tokio::test]
async fn transfer_signatures_submitted_alone_to_post_notes_are_refused() {
    let (t, base) = funded().await;
    assert_created(&t.note(&note_body(&B, &A, 50, at_residue(base, 2))).await);
    let ts = at_residue(base + 4_000, 999);
    let body = transfer_body(&B, &C, 100, 10, 30, ts);
    // Each of the seven signatures is over a note message (A -> recipient, total, ts) at residue 999.
    let slots: [(&str, &[u8; 32], u64); 7] = [
        ("issuer_from_signature", &B, 70),
        ("issuer_to_signature", &C, 40),
        ("from_recipient_prior_signature", &B, 100),
        ("from_recipient_signature", &B, 70),
        ("from_recipient_to_signature", &C, 40),
        ("to_recipient_prior_signature", &C, 10),
        ("to_recipient_signature", &C, 40),
    ];
    let as_note = |slot: &str, recipient: &[u8; 32], total: u64| with(&note_body(&A, recipient, total, ts), "signature", body[slot].clone());
    // The issuer's signature on the raised edge is a valid note that would raise A->C: the residue refuses it,
    // with or without a prior acknowledgment attached.
    assert_refused(&t.note(&as_note("issuer_to_signature", &C, 40)).await, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    let bound = with(&as_note("issuer_to_signature", &C, 40), "prior_signature", json!(sign(&A, &A, &C, 10, ts)));
    assert_refused(&t.note(&bound).await, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    // The creditors' signatures are not the issuer's, so none of them is a note at all.
    for (slot, recipient, total) in slots.iter().skip(2) {
        let r = t.note(&as_note(slot, recipient, *total)).await;
        if *slot == "from_recipient_signature" {
            assert_refused(&r, StatusCode::BAD_REQUEST, None); // a decrease of A->B, refused before the signature
            assert!(r.1["error"].as_str().unwrap().starts_with("Cumulative debt cannot decrease"), "{}", r.1);
        } else {
            assert_refused(&r, StatusCode::BAD_REQUEST, Some("invalid_signature"));
        }
    }
    // The issuer's signature on the lowered edge is a decrease while A->B is 100...
    assert_refused(&t.note(&as_note("issuer_from_signature", &B, 70)).await, StatusCode::BAD_REQUEST, None);
    // ...and once a set-off has lowered A->B to 60 it would be an increase: the residue refuses it there too.
    let setoff_ts = at_residue(base + 3_000, 998);
    let edge = |issuer: &[u8; 32], recipient: &[u8; 32], new_total: u64| {
        json!({
            "issuer_pubkey": hex::encode(pk(issuer)),
            "recipient_pubkey": hex::encode(pk(recipient)),
            "decrease": 40,
            "issuer_signature": sign(issuer, issuer, recipient, new_total, setoff_ts),
            "recipient_signature": sign(recipient, issuer, recipient, new_total, setoff_ts),
        })
    };
    let setoff = json!({"denomination": "erg", "timestamp": setoff_ts, "edges": [edge(&A, &B, 60), edge(&B, &A, 10)]});
    let (s, v) = t.post("/notes/setoff", &setoff).await;
    assert_eq!(s, StatusCode::OK, "{}", v);
    assert_refused(&t.note(&as_note("issuer_from_signature", &B, 70)).await, StatusCode::BAD_REQUEST, Some("reserved_timestamp"));
    assert_refused(&t.note(&as_note("from_recipient_signature", &B, 70)).await, StatusCode::BAD_REQUEST, Some("invalid_signature"));
    // No signature moved anything; only the set-off did.
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await, t.total(&B, &A).await), (Some(60), Some(10), Some(10)));
}

#[tokio::test]
async fn two_attempts_of_the_same_payment_exclude_each_other_in_either_order() {
    for t1_first in [true, false] {
        let (t, base) = funded().await;
        let (t1, t2) = (at_residue(base + 2_000, 999), at_residue(base + 3_000, 999));
        let priors = |body: Value| with(&with(&body, "from_prior_total", json!(100)), "to_prior_total", json!(10));
        let first = priors(transfer_body(&B, &C, 100, 10, 30, t1));
        let second = priors(transfer_body(&B, &C, 100, 10, 30, t2));
        let (applied, other) = if t1_first { (&first, &second) } else { (&second, &first) };
        assert_transferred(&t.transfer(applied).await);
        // Either way the other attempt no longer matches the acknowledged totals.
        assert_refused(&t.transfer(other).await, StatusCode::CONFLICT, Some("stale_prior"));
        let mut bare = other.clone();
        bare.as_object_mut().unwrap().remove("from_prior_total");
        bare.as_object_mut().unwrap().remove("to_prior_total");
        if t1_first {
            assert_refused(&t.transfer(&bare).await, StatusCode::BAD_REQUEST, Some("invalid_signature"));
        } else {
            assert_refused(&t.transfer(&bare).await, StatusCode::BAD_REQUEST, Some("past_timestamp"));
        }
        assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(70), Some(40)), "t1 first: {}", t1_first);
    }
}

#[tokio::test]
async fn both_edges_are_bound_only_after_a_transfer() {
    let (t, base) = funded().await;
    assert_created(&t.note(&note_body(&A, &E, 5, at_residue(base, 2))).await);
    let ts = at_residue(base + 2_000, 999);
    assert_transferred(&t.transfer(&transfer_body(&B, &C, 100, 10, 30, ts)).await);
    // ts ends in 999, so ts + 2 ends in 001 (ordinary) and ts + 998 in 997 (bound note).
    for (recipient, total) in [(&B, 80), (&C, 50)] {
        let r = t.note(&note_body(&A, recipient, total, ts + 2)).await;
        assert_refused(&r, StatusCode::BAD_REQUEST, Some("bound_note_required"));
    }
    assert_created(&t.note(&with(&note_body(&A, &C, 50, ts + 998), "prior_signature", json!(sign(&A, &A, &C, 40, ts + 998)))).await);
    // The edge the transfer did not touch still takes ordinary notes.
    assert_created(&t.note(&note_body(&A, &E, 6, ts + 2)).await);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await, t.total(&A, &E).await), (Some(70), Some(50), Some(6)));
}

#[tokio::test]
async fn store_refusals_map_to_statuses_and_codes() {
    let (t, base) = funded().await;
    let ts = at_residue(base + 2_000, 999);
    let ok = transfer_body(&B, &C, 100, 10, 30, ts);
    let forged = with(&ok, "issuer_to_signature", ok["issuer_from_signature"].clone());
    let cases: Vec<(&str, Value, StatusCode, &str)> = vec![
        ("zero amount", with(&ok, "amount", json!(0)), StatusCode::BAD_REQUEST, "invalid_transfer"),
        ("the same creditor twice", transfer_body(&B, &B, 100, 100, 30, ts), StatusCode::BAD_REQUEST, "invalid_transfer"),
        ("not the transfer residue", transfer_body(&B, &C, 100, 10, 30, ts - 1), StatusCode::BAD_REQUEST, "invalid_transfer"),
        ("more than is owed", transfer_body(&B, &C, 100, 10, 101, ts), StatusCode::BAD_REQUEST, "invalid_transfer"),
        ("a signature that does not verify", forged, StatusCode::BAD_REQUEST, "invalid_signature"),
        ("a stale diagnostic prior", with(&ok, "from_prior_total", json!(99)), StatusCode::CONFLICT, "stale_prior"),
        ("older than the edges", transfer_body(&B, &C, 100, 10, 30, at_residue(base - 5_000, 999)), StatusCode::BAD_REQUEST, "past_timestamp"),
        ("in the future", transfer_body(&B, &C, 100, 10, 30, at_residue(now_ms() + HOUR_MS, 999)), StatusCode::BAD_REQUEST, "future_timestamp"),
        ("older than an hour", transfer_body(&B, &C, 100, 10, 30, at_residue(now_ms() - HOUR_MS - 1_000, 999)), StatusCode::BAD_REQUEST, "novation_expired"),
        ("no edge from the issuer to the old creditor", transfer_body(&D, &C, 50, 10, 30, ts), StatusCode::NOT_FOUND, "not_found"),
    ];
    for (name, body, status, code) in &cases {
        let r = t.transfer(body).await;
        assert_eq!(r.0, *status, "{}: {}", name, r.1);
        assert_refused(&r, *status, Some(*code));
    }
    // Messages are plain text, never a Debug rendering.
    let r = t.transfer(&with(&ok, "amount", json!(0))).await;
    assert_eq!(r.1["error"], "Invalid transfer: the transfer amount must be positive");
    let r = t.transfer(&transfer_body(&B, &C, 100, 10, 30, at_residue(now_ms() - HOUR_MS - 1_000, 999))).await;
    assert_eq!(r.1["error"], "the novation's timestamp is older than the maximum novation age");
    let r = t.transfer(&transfer_body(&D, &C, 50, 10, 30, ts)).await;
    assert_eq!(r.1["error"], "the issuer has no note to the old creditor in this denomination");
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(100), Some(10)));

    // The two edges bound to different tracker NFTs (A -> E is stored directly: POST /notes refuses a foreign NFT).
    let foreign = IouNote::create_and_sign(pk(&E), 10, at_residue(base, 3), &A, &OTHER_NFT, DenomKey::Erg).unwrap();
    t.add_note_directly(&A, foreign).await;
    let r = t.transfer(&transfer_body(&B, &E, 100, 10, 30, ts)).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("tracker_id_mismatch"));
    assert_eq!(r.1["error"], "the two edges are bound to different tracker NFTs");

    // The applied transfer still works on this tracker.
    assert_transferred(&t.transfer(&ok).await);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(70), Some(40)));
}

#[tokio::test]
async fn a_staged_exchange_is_a_conflict() {
    let (t, base) = funded().await;
    // An ERG -> TOKEN exchange on A's pair with B, sent straight to the tracker thread: its ERG decrease of A->B
    // (100 -> 90) applies, and its TOKEN increase stays staged.
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
            from_amount: 10,
            to_denomination: token,
            to_amount: 10,
            timestamp: xts,
            issuer_from_signature: sign_raw(&A, &A, &B, &DenomKey::Erg, 90, xts, &nft()),
            recipient_from_signature: sign_raw(&B, &A, &B, &DenomKey::Erg, 90, xts, &nft()),
            issuer_to_signature: sign_raw(&A, &A, &B, &token, 15, xts, &nft()),
            recipient_to_signature: sign_raw(&B, &A, &B, &token, 15, xts, &nft()),
            from_reserve_box_id: "reserve_box".to_string(),
            acceptance_check: None,
            response_tx,
        })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();
    assert_eq!((t.total(&A, &B).await, t.total_in(&A, &B, token).await), (Some(90), Some(5)));
    let ts = at_residue(base + 2_000, 999);
    let r = t.transfer(&transfer_body(&B, &C, 90, 10, 30, ts)).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("staged_exchange"));
    assert_eq!(r.1["error"], "an exchange on one of these issuer/creditor pairs is staged");
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(90), Some(10)));
}

#[tokio::test]
async fn an_edge_with_on_chain_state_is_out_of_scope() {
    let (t, base) = funded().await;
    let (response_tx, rx) = tokio::sync::oneshot::channel();
    t.state
        .tx
        .send(TrackerCommand::MarkNotesPending { digest: [0u8; 33], tx_id: "tx1".to_string(), submitted_height: 100, response_tx })
        .await
        .unwrap();
    assert_eq!(rx.await.unwrap().unwrap(), 2);
    let ts = at_residue(base + 2_000, 999);
    let r = t.transfer(&transfer_body(&B, &C, 100, 10, 30, ts)).await;
    assert_refused(&r, StatusCode::CONFLICT, Some("onchain_scope"));
    assert!(r.1["error"].as_str().unwrap().starts_with("the old creditor's edge has a committed or pending on-chain total"), "{}", r.1);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(100), Some(10)));
}

#[tokio::test]
async fn the_new_creditors_acceptance_policy_refuses_with_403_after_the_signatures_verify() {
    // C accepts A's debt up to 35 in total.
    let cap: Arc<dyn basis_server::acceptance::NotePredicate> = Arc::new(basis_server::acceptance::WhitelistPredicate::new_with_limit(
        "cap-35",
        std::collections::HashSet::from([pk(&A)]),
        35,
    ));
    let t = tracker_with(Some(cap)).await;
    let base = now_ms() - 60_000;
    assert_created(&t.note(&note_body(&A, &B, 100, at_residue(base, 0))).await);
    assert_created(&t.note(&note_body(&A, &C, 10, at_residue(base, 1))).await);
    let ts = at_residue(base + 2_000, 999);
    let body = transfer_body(&B, &C, 100, 10, 30, ts); // A->C would reach 40
    let r = t.transfer(&body).await;
    assert_refused(&r, StatusCode::FORBIDDEN, None);
    assert_eq!(r.1["error"], "rejected by the new creditor's acceptance policy 'cap-35'");
    // A request whose signatures do not verify is refused for them first.
    let forged = with(&body, "to_recipient_signature", body["to_recipient_prior_signature"].clone());
    assert_refused(&t.transfer(&forged).await, StatusCode::BAD_REQUEST, Some("invalid_signature"));
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(100), Some(10)));
    // Within the cap the transfer applies.
    assert_transferred(&t.transfer(&transfer_body(&B, &C, 100, 10, 25, ts)).await);
    assert_eq!((t.total(&A, &B).await, t.total(&A, &C).await), (Some(75), Some(35)));
}
