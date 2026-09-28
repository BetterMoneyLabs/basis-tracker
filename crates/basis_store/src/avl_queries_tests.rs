//! Tests for tracker/reserve AVL state queries and public-key normalization.
//!
//! These functions sit on security- and correctness-critical paths but previously had no test
//! coverage at all (see specs/PRODUCTION_READINESS_AUDIT.md):
//!
//! - `get_total_debt` — the cumulative debt used to build the redemption signing message.
//! - `update_already_redeemed` — writes the value committed to the on-chain reserve AVL tree.
//! - `generate_reserve_insert_proof` / `reserve_state_digest` — the proof whose starting digest
//!   must match the on-chain reserve R5 for the reserve to evaluate.
//! - `normalize_public_key` — whose non-hex passthrough is what allowed the acceptance-policy
//!   check to be silently skipped (audit Issue H4).

use crate::{schnorr::generate_keypair, IouNote, NoteError, TrackerStateManager};

/// A manager plus a fresh (issuer, recipient) keypair, with a signed note of `amount` added.
fn manager_with_note(amount: u64) -> (TrackerStateManager, [u8; 32], [u8; 33], [u8; 33]) {
    let mut manager = TrackerStateManager::new_with_temp_storage();
    let (secret, issuer_pubkey) = generate_keypair();
    let (_, recipient_pubkey) = generate_keypair();

    let note = IouNote::create_and_sign(recipient_pubkey, amount, 1_700_000_000_000, &secret)
        .expect("create_and_sign should succeed");
    manager
        .add_note(&issuer_pubkey, &note)
        .expect("add_note should succeed");

    (manager, secret, issuer_pubkey, recipient_pubkey)
}

// ============================================================================
// get_total_debt
// ============================================================================

#[test]
fn test_get_total_debt_returns_note_amount() {
    let (manager, _, issuer, recipient) = manager_with_note(5_000_000);

    let debt = manager
        .get_total_debt(&issuer, &recipient)
        .expect("note was just added, so the AVL tree should contain it");

    assert_eq!(
        debt, 5_000_000,
        "get_total_debt must return the note's amount_collected"
    );
}

#[test]
fn test_get_total_debt_unknown_pair_is_error_not_zero() {
    let manager = TrackerStateManager::new_with_temp_storage();
    let (_, issuer) = generate_keypair();
    let (_, recipient) = generate_keypair();

    let err = manager
        .get_total_debt(&issuer, &recipient)
        .expect_err("an unknown (issuer, recipient) pair must not silently read as zero debt");

    // A zero return here would be dangerous: callers use this value to build the signing
    // message, and "no record" must never be indistinguishable from "no debt".
    assert!(
        matches!(err, NoteError::StorageError(_)),
        "expected StorageError, got: {:?}",
        err
    );
    assert!(
        format!("{:?}", err).contains("not found"),
        "error should say the record is absent, got: {:?}",
        err
    );
}

#[test]
fn test_get_total_debt_is_per_pair() {
    let (mut manager, secret, issuer, recipient_a) = manager_with_note(1_000);
    let (_, recipient_b) = generate_keypair();

    let note_b = IouNote::create_and_sign(recipient_b, 7_000, 1_700_000_001_000, &secret)
        .expect("create_and_sign should succeed");
    manager
        .add_note(&issuer, &note_b)
        .expect("add_note should succeed");

    assert_eq!(
        manager.get_total_debt(&issuer, &recipient_a).unwrap(),
        1_000
    );
    assert_eq!(
        manager.get_total_debt(&issuer, &recipient_b).unwrap(),
        7_000
    );
    // The two lookups must not have clobbered each other in the shared tree.
}

// ============================================================================
// update_already_redeemed
// ============================================================================

#[test]
fn test_update_already_redeemed_changes_reserve_state_digest() {
    let (mut manager, _, issuer, recipient) = manager_with_note(1_000);

    let before = manager.reserve_state_digest(&issuer);
    manager
        .update_already_redeemed(&issuer, &recipient, 1_700_000_000_000, 4_000)
        .expect("first reserve AVL update should succeed");
    let after_first = manager.reserve_state_digest(&issuer);

    assert_ne!(
        before, after_first,
        "writing a reserve AVL entry must change the root digest (this digest is the on-chain R5)"
    );

    manager
        .update_already_redeemed(&issuer, &recipient, 1_700_000_002_000, 9_000)
        .expect("second reserve AVL update should succeed");
    let after_second = manager.reserve_state_digest(&issuer);

    assert_ne!(
        after_first, after_second,
        "a second update with a different value must change the digest again"
    );
}

#[test]
fn test_update_already_redeemed_is_idempotent_for_same_value() {
    let (mut manager, _, issuer, recipient) = manager_with_note(1_000);

    manager
        .update_already_redeemed(&issuer, &recipient, 1_700_000_000_000, 4_000)
        .unwrap();
    let first = manager.reserve_state_digest(&issuer);

    manager
        .update_already_redeemed(&issuer, &recipient, 1_700_000_000_000, 4_000)
        .unwrap();
    let second = manager.reserve_state_digest(&issuer);

    assert_eq!(
        first, second,
        "re-writing the same (timestamp, already_redeemed) must not change the root digest"
    );
}

#[test]
fn test_update_already_redeemed_creates_tree_for_unknown_issuer() {
    // reserve_tree_mut creates the issuer's tree on demand, so this must not error even though
    // no reserve was ever registered for this issuer.
    let mut manager = TrackerStateManager::new_with_temp_storage();
    let (_, issuer) = generate_keypair();
    let (_, recipient) = generate_keypair();

    manager
        .update_already_redeemed(&issuer, &recipient, 1_700_000_000_000, 1)
        .expect("a fresh issuer should get a tree created on demand");

    let digest = manager.reserve_state_digest(&issuer);
    assert_eq!(
        digest.len(),
        33,
        "root digest should be 33 bytes (32-byte digest + 1 flags byte), got {}",
        digest.len()
    );
}

// ============================================================================
// generate_reserve_insert_proof
// ============================================================================

#[test]
fn test_generate_reserve_insert_proof_for_fresh_issuer() {
    let (manager, _, issuer, recipient) = manager_with_note(1_000);

    let (proof, updated_digest) = manager
        .generate_reserve_insert_proof(&issuer, &recipient, 1_700_000_000_000, 0)
        .expect("insert proof against an empty tree should succeed");

    assert!(
        !proof.is_empty(),
        "insert proof bytes must not be empty — an empty proof would be committed as context var #5"
    );
    assert_eq!(
        updated_digest.len(),
        33,
        "updated digest should be 33 bytes, got {}",
        updated_digest.len()
    );
    assert_ne!(
        updated_digest,
        manager.reserve_state_digest(&issuer),
        "for a fresh issuer the current root is the empty-tree digest, so the post-insert \
         digest predicted by the proof must differ from it"
    );
}

#[test]
fn test_reserve_insert_proof_predicts_persisted_state() {
    // The invariant that makes the on-chain flow work: a proof generated for
    // (timestamp, already_redeemed) must predict exactly the root digest the tracker ends up
    // with once that update is persisted, because the reserve box being spent has to carry
    // this value in R5 for the proof to evaluate.
    let (mut manager, _, issuer, recipient) = manager_with_note(1_000);

    let (_, predicted) = manager
        .generate_reserve_insert_proof(&issuer, &recipient, 1_700_000_000_000, 6_500)
        .expect("insert proof should succeed");

    manager
        .update_already_redeemed(&issuer, &recipient, 1_700_000_000_000, 6_500)
        .expect("persisting the same update should succeed");

    assert_eq!(
        predicted,
        manager.reserve_state_digest(&issuer),
        "the digest predicted by generate_reserve_insert_proof must equal the root after the \
         same update is persisted"
    );
}

#[test]
fn test_generate_reserve_insert_proof_is_deterministic() {
    // The doc comment promises the non-mutating generator returns the same proof on repeat
    // calls; that promise is what lets a client retry a build without re-deriving state.
    let (manager, _, issuer, recipient) = manager_with_note(1_000);

    let first = manager
        .generate_reserve_insert_proof(&issuer, &recipient, 1_700_000_000_000, 2_500)
        .unwrap();
    let second = manager
        .generate_reserve_insert_proof(&issuer, &recipient, 1_700_000_000_000, 2_500)
        .unwrap();

    assert_eq!(first.0, second.0, "insert proof must be deterministic");
    assert_eq!(first.1, second.1, "resulting digest must be deterministic");
}

#[test]
fn test_generate_reserve_insert_proof_digest_tracks_value() {
    let (manager, _, issuer, recipient) = manager_with_note(1_000);

    let (_, digest_a) = manager
        .generate_reserve_insert_proof(&issuer, &recipient, 1_700_000_000_000, 1_000)
        .unwrap();
    let (_, digest_b) = manager
        .generate_reserve_insert_proof(&issuer, &recipient, 1_700_000_000_000, 2_000)
        .unwrap();

    assert_ne!(
        digest_a, digest_b,
        "a different already_redeemed value must yield a different resulting digest"
    );
}

// ============================================================================
// normalize_public_key
// ============================================================================

#[test]
fn test_normalize_public_key_strips_group_element_prefix() {
    let (_, pubkey) = generate_keypair();

    // 0x07 || 33-byte compressed pubkey, as read from an R4 register.
    let prefixed = format!("07{}", hex::encode(pubkey));
    assert_eq!(
        crate::normalize_public_key(&prefixed),
        hex::encode(pubkey),
        "a 0x07-prefixed 34-byte key must normalize to the bare 33-byte key"
    );
}

#[test]
fn test_normalize_public_key_passes_through_bare_key() {
    let (_, pubkey) = generate_keypair();
    let bare = hex::encode(pubkey);

    assert_eq!(
        crate::normalize_public_key(&bare),
        bare,
        "an unprefixed 33-byte key must be returned unchanged"
    );
}

#[test]
fn test_normalize_public_key_passes_through_non_hex() {
    // Characterisation test for audit Issue H4: invalid hex is returned verbatim rather than
    // erroring. Callers that feed the result into a decoder then fail to decode and, in the
    // redemption path, previously skipped the acceptance-policy check entirely.
    assert_eq!(
        crate::normalize_public_key("not-hex!!"),
        "not-hex!!",
        "invalid hex must be returned unchanged (this passthrough is what audit H4 flags)"
    );
}

#[test]
fn test_normalize_public_key_handles_empty_and_short_inputs() {
    assert_eq!(crate::normalize_public_key(""), "");
    // A 0x07 prefix with fewer than 34 bytes must NOT be sliced — the length guard exists to
    // prevent exactly that class of panic.
    assert_eq!(crate::normalize_public_key("07"), "07");
    assert_eq!(crate::normalize_public_key("0701"), "0701");
}

#[test]
fn test_normalize_public_key_0x07_without_enough_bytes_is_unchanged() {
    // 0x07 followed by only 5 bytes: below the 34-byte threshold, so no slicing occurs.
    let short = format!("07{}", "ab".repeat(5));
    assert_eq!(
        crate::normalize_public_key(&short),
        short,
        "a 0x07-prefixed key shorter than 34 bytes must be left alone"
    );
}

// ============================================================================
// Register accessors and R7 (refund initiation height) decoding
// ============================================================================

fn box_with_registers(regs: &[(&str, &str)]) -> crate::ErgoBox {
    crate::ErgoBox {
        box_id: "testbox".to_string(),
        value: 1_000,
        ergo_tree: "00".to_string(),
        creation_height: 1,
        transaction_id: "tx".to_string(),
        additional_registers: regs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

#[test]
fn test_get_register_returns_value_or_none() {
    let b = box_with_registers(&[("R4", "aabb"), ("R5", "ccdd")]);

    assert_eq!(b.get_register("R4"), Some("aabb"));
    assert_eq!(b.get_register("R5"), Some("ccdd"));
    assert_eq!(
        b.get_register("R7"),
        None,
        "an absent register must return None, not an empty string"
    );
}

#[test]
fn test_has_register_distinguishes_present_and_absent() {
    let b = box_with_registers(&[("R4", "aabb")]);

    assert!(b.has_register("R4"));
    assert!(!b.has_register("R5"));
    assert!(!b.has_register("R7"), "R7 drives the two-phase refund flow");
}

#[test]
fn test_get_register_empty_box_has_no_registers() {
    let b = box_with_registers(&[]);
    assert!(!b.has_register("R4"));
    assert_eq!(b.get_register("R4"), None);
}

#[test]
fn test_decode_ergo_long_register_known_values() {
    // Vectors are hand-computed (zigzag then VLQ), deliberately NOT produced by
    // transaction_builder::serialize_long, so encoder and decoder are not validated against
    // each other circularly.
    //   0    -> zigzag 0    -> 0x00
    //   1    -> zigzag 2    -> 0x02
    //   100  -> zigzag 200  -> 0xC8 0x01
    //   2160 -> zigzag 4320 -> 0xE0 0x21
    for (hex, expected) in [
        ("0500", 0u64),
        ("0502", 1),
        ("05c801", 100),
        ("05e021", 2160),
    ] {
        let s = hex.to_string();
        assert_eq!(
            crate::ergo_scanner::decode_ergo_long_register(Some(&s)),
            expected,
            "decoding {} should yield {}",
            hex,
            expected
        );
    }
}

#[test]
fn test_decode_ergo_long_register_malformed_input_yields_zero() {
    // Characterisation test: every failure mode collapses to 0, which is indistinguishable
    // from a genuinely-zero refund height. Callers treat non-zero as "refund pending"
    // (the no_pending_refund acceptance predicate), so a malformed R7 reads as "no refund".
    for hex in [
        "05",   // prefix only, no payload
        "0100", // wrong type byte
        "05ff", // truncated VLQ (continuation bit set, no terminator)
        "05zz", // not hex
        "",     // empty
    ] {
        let s = hex.to_string();
        assert_eq!(
            crate::ergo_scanner::decode_ergo_long_register(Some(&s)),
            0,
            "malformed input {} should decode to 0",
            hex
        );
    }

    assert_eq!(
        crate::ergo_scanner::decode_ergo_long_register(None),
        0,
        "an absent R7 register should decode to 0"
    );
}

#[test]
fn test_decode_ergo_long_register_rejects_negative_values() {
    // zigzag(-1) == 1, so 0x01 decodes to -1 and must be rejected rather than wrapping.
    let s = "0501".to_string();
    assert_eq!(
        crate::ergo_scanner::decode_ergo_long_register(Some(&s)),
        0,
        "a negative Long must decode to 0, not wrap to u64::MAX"
    );
}
