//! One-off repair: reset the Alice->Bob note to the state matching the on-chain
//! reserve box ef912b0c (cumulative redeemed = 0.1 ERG, payment timestamp
//! 1783612740170) so that a single `/redeem/complete` call re-syncs the
//! in-memory reserve AVL tree with the correct payment timestamp.
//!
//! Run with:
//! cargo test -p basis_store --test fix_note_state -- --ignored --nocapture

use basis_store::persistence::NoteStorage;
use basis_store::PubKey;

const NOTES_PATH: &str = "/home/kushti/chaincash/basis-tracker/crates/basis_server/data/notes";
const ISSUER_HEX: &str = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
const RECIPIENT_HEX: &str = "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea";
const ONCHAIN_PAYMENT_TIMESTAMP: u64 = 1783612740170;

#[test]
#[ignore = "one-off manual repair of persisted note state"]
fn reset_alice_bob_note_to_onchain_state() {
    let storage = NoteStorage::open(NOTES_PATH).expect("open notes storage");

    let rebuilt = storage.rebuild_indices().expect("rebuild indices");
    println!("rebuilt indices: {} notes", rebuilt);

    let all = storage.get_all_notes_with_issuer().expect("list notes");
    println!("notes in storage: {}", all.len());
    for (iss, n) in &all {
        println!(
            "  issuer={} recipient={} redeemed={} ts={}",
            hex::encode(iss),
            hex::encode(n.recipient_pubkey),
            n.amount_redeemed,
            n.timestamp
        );
    }

    let issuer: PubKey = hex::decode(ISSUER_HEX).unwrap().try_into().unwrap();
    let recipient: PubKey = hex::decode(RECIPIENT_HEX).unwrap().try_into().unwrap();

    let mut note = storage
        .get_note(&issuer, &recipient)
        .expect("get note")
        .expect("note must exist");

    println!(
        "before: amount_redeemed={} timestamp={}",
        note.amount_redeemed, note.timestamp
    );

    note.amount_redeemed = 0;
    note.timestamp = ONCHAIN_PAYMENT_TIMESTAMP;

    storage
        .store_note(&issuer, &note)
        .expect("store repaired note");

    let check = storage
        .get_note(&issuer, &recipient)
        .expect("get note")
        .expect("note must exist");
    println!(
        "after:  amount_redeemed={} timestamp={}",
        check.amount_redeemed, check.timestamp
    );
    assert_eq!(check.amount_redeemed, 0);
    assert_eq!(check.timestamp, ONCHAIN_PAYMENT_TIMESTAMP);
}
