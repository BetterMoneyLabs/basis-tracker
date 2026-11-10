//! Regression tests for follow-up review findings that are NOT fixed in this change.
//!
//! Each test asserts the correct behaviour, fails at `3df7ed9`, and is `#[ignore]`d with the
//! finding id so the default test run stays green. Run them with
//! `cargo test -p basis_store review_regression -- --ignored` and drop the `#[ignore]` once the
//! finding is fixed.

use crate::{DenomKey, IouNote, PubKey, TrackerStateManager};
use secp256k1::{Secp256k1, SecretKey};

const TRACKER_NFT: [u8; 32] = [0x42; 32];

fn keypair(i: u8) -> ([u8; 32], PubKey) {
    let mut secret = [7u8; 32];
    secret[31] = i;
    let public = secp256k1::PublicKey::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&secret).unwrap(),
    )
    .serialize();
    (secret, public)
}

fn post(
    tracker: &mut TrackerStateManager,
    issuer: &([u8; 32], PubKey),
    recipient: PubKey,
    amount: u64,
    timestamp: u64,
) {
    let note = IouNote::create_and_sign(
        recipient,
        amount,
        timestamp,
        &issuer.0,
        &TRACKER_NFT,
        DenomKey::Erg,
    )
    .unwrap();
    tracker.add_note(&issuer.1, &note).unwrap();
}

/// What the tracker box updater does for one cycle: snapshot the live digest, submit it,
/// mark notes pending, and confirm once the update transaction is on-chain.
fn commit(tracker: &mut TrackerStateManager, tx: &str) -> [u8; 33] {
    let digest = tracker.get_state().avl_root_digest;
    tracker.mark_notes_pending(digest, tx, 1).unwrap();
    tracker.confirm_pending_notes(digest, tx, 1).unwrap();
    digest
}

fn assert_committed_proofs_work(
    tracker: &mut TrackerStateManager,
    issuer: PubKey,
    recipients: &[PubKey],
) {
    for (i, recipient) in recipients.iter().enumerate() {
        let proof =
            tracker.generate_committed_tracker_lookup_proof(&issuer, recipient, &DenomKey::Erg);
        assert!(
            proof.is_ok(),
            "committed proof for pair {i} failed (every /redemption/build fails the same way): {:?}",
            proof.err()
        );
    }
}

/// Follow-up review N2: the committed tree is rebuilt in order of each note's LATEST
/// timestamp, but the live (committed) tree has first-insertion shape. An ordinary repeat
/// payment to an earlier creditor reorders the rebuild and breaks every committed proof.
#[test]
#[ignore = "follow-up review N2: committed tree rebuilt in the wrong insertion order"]
fn committed_proofs_survive_a_repeat_payment() {
    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let issuer = keypair(1);
    let recipients: Vec<PubKey> = (2..10).map(|i| keypair(i).1).collect();
    for (i, recipient) in recipients.iter().enumerate() {
        post(&mut tracker, &issuer, *recipient, 1_000, 1 + i as u64);
    }
    commit(&mut tracker, "tx1");
    assert_committed_proofs_work(&mut tracker, issuer.1, &recipients);

    // Pay the first creditor again, then commit.
    post(&mut tracker, &issuer, recipients[0], 2_000, 100);
    commit(&mut tracker, "tx2");
    assert_committed_proofs_work(&mut tracker, issuer.1, &recipients);
}

/// Follow-up review N2: same root cause, triggered by a NEW pair whose timestamp is older than
/// an existing note's (client clock skew, or one deliberately backdated self-note by any
/// Write client).
#[test]
#[ignore = "follow-up review N2: committed tree rebuilt in the wrong insertion order"]
fn committed_proofs_survive_a_backdated_new_note() {
    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let issuer = keypair(1);
    let recipients: Vec<PubKey> = (2..10).map(|i| keypair(i).1).collect();
    for (i, recipient) in recipients.iter().enumerate().take(7) {
        post(&mut tracker, &issuer, *recipient, 1_000, 1_000 + i as u64);
    }
    // Arrives last, but carries the oldest timestamp.
    post(&mut tracker, &issuer, recipients[7], 1_000, 1);
    commit(&mut tracker, "tx1");
    assert_committed_proofs_work(&mut tracker, issuer.1, &recipients);
}

/// Follow-up review N2 (confirmation state): a note re-posted while its previous value is
/// Pending becomes LocalOnly, `confirm_pending_notes` only promotes Pending notes, so the value
/// the chain committed is never recorded and the rebuild no longer matches.
#[test]
#[ignore = "follow-up review N2: confirmation state loses values re-posted while pending"]
fn committed_proofs_survive_a_note_updated_while_its_commit_is_pending() {
    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let issuer = keypair(1);
    let (a, b) = (keypair(2).1, keypair(3).1);
    post(&mut tracker, &issuer, a, 1_000, 1);
    commit(&mut tracker, "tx1");

    post(&mut tracker, &issuer, b, 1_000, 2);
    let digest = tracker.get_state().avl_root_digest;
    tracker.mark_notes_pending(digest, "tx2", 2).unwrap();
    // B is paid again before tx2 confirms.
    post(&mut tracker, &issuer, b, 1_500, 3);
    tracker.confirm_pending_notes(digest, "tx2", 2).unwrap();

    assert_committed_proofs_work(&mut tracker, issuer.1, &[a]);
}

/// Follow-up review N2 (updater race): the updater snapshots the digest, makes several node
/// calls, then sends MarkNotesPending, which marks the CURRENT note values. A note posted in
/// that window is recorded as committed although the submitted digest does not contain it.
#[test]
#[ignore = "follow-up review N2: mark_notes_pending ignores the digest it is given"]
fn committed_proofs_survive_a_note_posted_while_the_update_is_built() {
    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let issuer = keypair(1);
    let (a, b) = (keypair(2).1, keypair(3).1);
    post(&mut tracker, &issuer, a, 1_000, 1);
    let snapshot = tracker.get_state().avl_root_digest;
    // Posted after the snapshot, before MarkNotesPending.
    post(&mut tracker, &issuer, b, 1_000, 2);
    tracker.mark_notes_pending(snapshot, "tx1", 1).unwrap();
    tracker.confirm_pending_notes(snapshot, "tx1", 1).unwrap();

    assert_committed_proofs_work(&mut tracker, issuer.1, &[a]);
}

/// Follow-up review N15: the JVM `AvlTree.insert` FAILS on an existing key, which is what the
/// C1 fix in `basis.es` relies on (omitting var #7 forces `insert`). The vendored
/// `ergo_avltree_rust` turns `Insert` into an upsert for the whole workspace, including
/// sigma-rust's interpreter, so local reduction accepts a #7-less replay the node rejects.
#[test]
#[ignore = "follow-up review N15: vendored ergo_avltree_rust treats Insert as insert-or-update"]
fn avl_insert_of_an_existing_key_fails_like_the_jvm() {
    use ergo_avltree_rust::operation::{KeyValue, Operation};

    let insert = Operation::Insert(KeyValue {
        key: vec![1u8; 32].into(),
        value: vec![2u8; 16].into(),
    });
    assert!(insert.update_fn(Some(vec![3u8; 16].into())).is_err());
}

/// Follow-up review N11: `EUNip3sG…` is `basis-token.es` as of `bb0cfbd`. It derives the debt
/// key and signed message exactly like `basis.es`, so on the multi-token tracker an ERG note
/// also redeems the same number of raw token units from such a reserve. The demos still
/// create reserves at it and the specs still document it.
#[test]
#[ignore = "follow-up review N11: previous token reserve script still shipped in demos/specs"]
fn previous_token_reserve_script_is_not_shipped() {
    const PREVIOUS_TOKEN_P2S_PREFIX: &str = "EUNip3sG";

    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !matches!(
                    path.file_name().and_then(|n| n.to_str()),
                    Some("target" | ".git" | "__pycache__")
                ) {
                    scan(&path, hits);
                }
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                if text.contains(PREVIOUS_TOKEN_P2S_PREFIX) {
                    hits.push(path.display().to_string());
                }
            }
        }
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut hits = Vec::new();
    for dir in ["demo", "specs", "config", "scala/specs"] {
        scan(&root.join(dir), &mut hits);
    }
    assert!(
        hits.is_empty(),
        "previous token reserve P2S still in: {hits:#?}"
    );
}
