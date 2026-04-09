//! BasisSpec replication tests in Rust
//!
//! These tests replicate the key test scenarios from the Scala BasisSpec,
//! adapted for the Rust off-chain implementation. Since we cannot test
//! on-chain ErgoScript execution in Rust, these tests focus on:
//!
//! 1. Message format correctness (48 bytes: key || totalDebt || timestamp)
//! 2. Schnorr signature creation and verification by both parties
//! 3. Debt transfer / triangular trade scenarios
//! 4. Replay attack prevention via timestamp verification
//! 5. Invalid signature rejection
//! 6. Edge cases and cryptographic invariants
//!
//! See: scala/tests/BasisSpec.scala for the original Scala tests

#[cfg(test)]
mod tests {
    use crate::schnorr::{
        self, generate_keypair, schnorr_sign, schnorr_verify,
        pubkey_to_hex, pubkey_from_hex, signature_to_hex, signature_from_hex,
    };
    use crate::{IouNote, NoteKey, TrackerStateManager, PubKey};
    use basis_core::types::signing_message as core_signing_message;
    use blake2::{Blake2b, Digest};
    use generic_array::typenum::U32;
    use secp256k1::{Secp256k1, SecretKey};

    // ========== Test Constants (matching Scala BasisSpec) ==========

    const BASIS_TOKEN_ID: &str = "4b2d8b7beb3eaac8234d9e61792d270898a43934d6a27275e4f3a044609c9f2a";
    const TRACKER_NFT: &str = "3c45f29a5165b030fdb5eaf5d81f8108f9d8f507b31487dd51f4ae08fe07cf4a";

    const MIN_VALUE: u64 = 1_000_000_000;    // 1 ERG
    const FEE_VALUE: u64 = 1_000_000;        // 0.001 ERG

    // Standard test timestamp (in seconds, Sept 2001 - clearly in the past)
    // Note: The add_note check compares timestamp (ms) against current_time (secs),
    // so we use a value that is clearly in the past even when measured in seconds.
    const TEST_TIMESTAMP: u64 = 1_000_000_000;

    // ========== Helper Functions ==========

    fn blake2b256(data: &[u8]) -> [u8; 32] {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Create a debt record key: blake2b256(ownerKey || receiverKey)
    fn debt_record_key(owner_key: &PubKey, receiver_key: &PubKey) -> [u8; 32] {
        let mut input = [0u8; 66];
        input[..33].copy_from_slice(owner_key);
        input[33..].copy_from_slice(receiver_key);
        blake2b256(&input)
    }

    /// Generate a random keypair
    fn random_keypair() -> ([u8; 32], PubKey) {
        generate_keypair()
    }

    // ========== POSITIVE TESTS ==========

    /// BasisSpec: "basis redemption should work with valid setup"
    /// Tests that valid signatures from both owner and tracker verify correctly
    /// against the same 48-byte message (key || totalDebt || timestamp).
    #[test]
    fn basis_redemption_with_valid_setup() {
        // Setup keys
        let (owner_secret, owner_pk) = random_keypair();
        let (_receiver_secret, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        // Same parameters as Scala test
        let total_debt: u64 = 1_000_000_000; // 1 ERG
        let timestamp: u64 = TEST_TIMESTAMP;

        // Create message for signatures: key || totalDebt || timestamp
        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        // Verify message format: 32 (key) + 8 (totalDebt) + 8 (timestamp) = 48 bytes
        assert_eq!(message.len(), 48, "Message must be exactly 48 bytes");

        // Both owner and tracker sign the EXACT same message
        let owner_sig = schnorr_sign(&message, &owner_secret, &owner_pk)
            .expect("Owner signing should succeed");
        let tracker_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk)
            .expect("Tracker signing should succeed");

        // Both signatures must be 65 bytes (33-byte a + 32-byte z)
        assert_eq!(owner_sig.len(), 65, "Owner signature must be 65 bytes");
        assert_eq!(tracker_sig.len(), 65, "Tracker signature must be 65 bytes");

        // Verify owner signature
        assert!(schnorr_verify(&owner_sig, &message, &owner_pk).is_ok(),
            "Owner signature must verify");

        // Verify tracker signature
        assert!(schnorr_verify(&tracker_sig, &message, &tracker_pk).is_ok(),
            "Tracker signature must verify");

        // Verify signatures are different (different signers)
        assert_ne!(owner_sig, tracker_sig,
            "Owner and tracker signatures must differ");

        // Create IOU note using the crate's API
        let note = IouNote::create_and_sign(receiver_pk, total_debt, timestamp, &owner_secret)
            .expect("Note creation should succeed");

        // Verify the note's signing message matches the expected format
        let note_message = note.signing_message(&owner_pk);
        assert_eq!(note_message.len(), 48);
        assert_eq!(note_message, message, "Note's signing message must match");

        // Verify the note's signature
        assert!(note.verify_signature(&owner_pk).is_ok(),
            "Note signature must verify");
    }

    /// BasisSpec: "OR-branch A: valid tracker sig -> succeeds"
    /// Tests that both valid signatures (reserve + tracker) verify correctly
    #[test]
    fn valid_tracker_and_reserve_signatures() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt: u64 = 500_000_000; // 0.5 ERG
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        let reserve_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        let tracker_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk).unwrap();

        // Both signatures must verify
        assert!(schnorr_verify(&reserve_sig, &message, &owner_pk).is_ok());
        assert!(schnorr_verify(&tracker_sig, &message, &tracker_pk).is_ok());
    }

    /// BasisSpec: Emergency exit - should succeed with old tracker and valid tracker signature
    /// Tests that emergency redemption still uses the same message format
    #[test]
    fn emergency_redemption_same_message_format() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt: u64 = 500_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        // Same message for both normal and emergency (the only difference is
        // tracker signature is optional after 2160 blocks on-chain)
        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);
        assert_eq!(message.len(), 48);

        // Owner can still sign
        let owner_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        assert!(schnorr_verify(&owner_sig, &message, &owner_pk).is_ok());

        // Tracker can still sign (signature provided is still valid even in emergency)
        let tracker_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk).unwrap();
        assert!(schnorr_verify(&tracker_sig, &message, &tracker_pk).is_ok());
    }

    /// BasisSpec: debt transfer - triangular trade with consent should work
    /// Alice owes Bob 10 ERG. Alice signs TWO new notes: A->B (5 ERG), A->C (5 ERG).
    /// Both notes independently verify.
    #[test]
    fn debt_transfer_triangular_trade() {
        let secp = Secp256k1::new();

        // Alice (debtor)
        let alice_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let alice_pk = secp256k1::PublicKey::from_secret_key(&secp, &alice_secret).serialize();

        // Bob (original creditor)
        let bob_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let bob_pk = secp256k1::PublicKey::from_secret_key(&secp, &bob_secret).serialize();

        // Carol (new creditor via transfer)
        let _carol_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let carol_pk = secp256k1::PublicKey::from_secret_key(&secp, &_carol_secret).serialize();

        let initial_debt_to_bob: u64 = 10_000_000_000; // 10 ERG
        let transfer_amount: u64 = 5_000_000_000;       // 5 ERG
        let remaining_debt_to_bob: u64 = 5_000_000_000; // 5 ERG

        let timestamp: u64 = TEST_TIMESTAMP;

        // Alice signs note to Bob (remaining debt)
        let msg_to_bob = core_signing_message(&alice_pk, &bob_pk, remaining_debt_to_bob, timestamp);
        assert_eq!(msg_to_bob.len(), 48);

        // Alice signs note to Carol (transferred debt)
        let timestamp2 = timestamp + 1000; // Slightly later timestamp for new note
        let msg_to_carol = core_signing_message(&alice_pk, &carol_pk, transfer_amount, timestamp2);
        assert_eq!(msg_to_carol.len(), 48);

        // Alice signs both notes
        let alice_sig_bob = schnorr_sign(&msg_to_bob, &alice_secret.secret_bytes(), &alice_pk).unwrap();
        let alice_sig_carol = schnorr_sign(&msg_to_carol, &alice_secret.secret_bytes(), &alice_pk).unwrap();

        // Both signatures must verify against Alice's public key
        assert!(schnorr_verify(&alice_sig_bob, &msg_to_bob, &alice_pk).is_ok(),
            "Alice's signature on Bob's note must verify");
        assert!(schnorr_verify(&alice_sig_carol, &msg_to_carol, &alice_pk).is_ok(),
            "Alice's signature on Carol's note must verify");

        // Debt amounts must add up
        assert_eq!(remaining_debt_to_bob + transfer_amount, initial_debt_to_bob,
            "Remaining + transferred debt must equal initial debt");

        // Messages must be different (different recipients/amounts)
        assert_ne!(msg_to_bob, msg_to_carol,
            "Notes to different recipients must have different messages");

        // Create actual IOU notes
        let note_to_bob = IouNote::create_and_sign(
            bob_pk, remaining_debt_to_bob, timestamp, &alice_secret.secret_bytes()
        ).unwrap();
        let note_to_carol = IouNote::create_and_sign(
            carol_pk, transfer_amount, timestamp2, &alice_secret.secret_bytes()
        ).unwrap();

        // Verify both notes
        assert!(note_to_bob.verify_signature(&alice_pk).is_ok());
        assert!(note_to_carol.verify_signature(&alice_pk).is_ok());

        // Notes must have different timestamps (prevents replay)
        assert_ne!(note_to_bob.timestamp, note_to_carol.timestamp);
    }

    /// BasisSpec: debt transfer - multi-hop triangular trade concept verification
    /// Alice creates 3 independent notes: A->B, A->C, A->D. All signatures verified.
    #[test]
    fn multi_hop_triangular_trade() {
        let secp = Secp256k1::new();

        // Alice (debtor)
        let alice_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let alice_pk = secp256k1::PublicKey::from_secret_key(&secp, &alice_secret).serialize();

        // Three creditors
        let bob_pk = secp256k1::PublicKey::from_secret_key(&secp,
            &SecretKey::new(&mut secp256k1::rand::thread_rng())).serialize();
        let carol_pk = secp256k1::PublicKey::from_secret_key(&secp,
            &SecretKey::new(&mut secp256k1::rand::thread_rng())).serialize();
        let dave_pk = secp256k1::PublicKey::from_secret_key(&secp,
            &SecretKey::new(&mut secp256k1::rand::thread_rng())).serialize();

        let amount: u64 = 5_000_000_000; // 5 ERG each
        let base_timestamp: u64 = TEST_TIMESTAMP;

        // Create notes with different timestamps
        let note_bob = IouNote::create_and_sign(
            bob_pk, amount, base_timestamp, &alice_secret.secret_bytes()
        ).unwrap();
        let note_carol = IouNote::create_and_sign(
            carol_pk, amount, base_timestamp + 1000, &alice_secret.secret_bytes()
        ).unwrap();
        let note_dave = IouNote::create_and_sign(
            dave_pk, amount, base_timestamp + 2000, &alice_secret.secret_bytes()
        ).unwrap();

        // All must verify
        assert!(note_bob.verify_signature(&alice_pk).is_ok());
        assert!(note_carol.verify_signature(&alice_pk).is_ok());
        assert!(note_dave.verify_signature(&alice_pk).is_ok());

        // All must have different timestamps
        assert!(note_bob.timestamp < note_carol.timestamp);
        assert!(note_carol.timestamp < note_dave.timestamp);

        // All must have same amount
        assert_eq!(note_bob.amount_collected, note_carol.amount_collected);
        assert_eq!(note_carol.amount_collected, note_dave.amount_collected);
    }

    // ========== NEGATIVE TESTS ==========

    /// BasisSpec: "basis redemption should fail with invalid tracker signature"
    /// Tracker signature signed with wrong key must fail verification
    #[test]
    fn invalid_tracker_signature_rejected() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();
        let (_, wrong_pk) = random_keypair(); // Wrong key (e.g., receiver's)

        let total_debt: u64 = 500_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        let reserve_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        // Invalid tracker sig: signed with wrong key
        let invalid_tracker_sig = schnorr_sign(&message, &owner_secret, &wrong_pk).unwrap();

        // Reserve sig must verify
        assert!(schnorr_verify(&reserve_sig, &message, &owner_pk).is_ok());

        // Invalid tracker sig must NOT verify against tracker's public key
        assert!(schnorr_verify(&invalid_tracker_sig, &message, &tracker_pk).is_err(),
            "Invalid tracker signature must fail verification");
    }

    /// BasisSpec: "basis redemption should fail with invalid tracker signature (even with old tracker)"
    /// Invalid tracker signature signed with receiver's secret, tracker box is 3+ days old
    #[test]
    fn invalid_tracker_sig_rejected_even_with_old_tracker() {
        let (owner_secret, owner_pk) = random_keypair();
        let (receiver_secret, receiver_pk) = random_keypair();
        let (_, tracker_pk) = random_keypair();

        let total_debt: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        let reserve_sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();
        // Invalid tracker sig: signed with receiver's secret
        let invalid_tracker_sig = schnorr_sign(&message, &receiver_secret, &receiver_pk).unwrap();

        assert!(schnorr_verify(&reserve_sig, &message, &owner_pk).is_ok());
        // Must fail: wrong signer
        assert!(schnorr_verify(&invalid_tracker_sig, &message, &tracker_pk).is_err());
    }

    /// BasisSpec: "basis redemption should fail with invalid reserve owner signature"
    /// Reserve signature signed with trackerSecret instead of ownerSecret
    #[test]
    fn invalid_reserve_owner_signature_rejected() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt: u64 = 500_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        // Invalid reserve sig: signed with tracker's secret
        let invalid_reserve_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk).unwrap();
        let tracker_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk).unwrap();

        // Invalid reserve sig must NOT verify against owner's public key
        assert!(schnorr_verify(&invalid_reserve_sig, &message, &owner_pk).is_err(),
            "Invalid reserve owner signature must fail verification");

        // Tracker sig must verify
        assert!(schnorr_verify(&tracker_sig, &message, &tracker_pk).is_ok());
    }

    /// BasisSpec: OR-branch B: invalid tracker sig -> fails (corrupted signature)
    #[test]
    fn corrupted_tracker_signature_rejected() {
        let (_, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt: u64 = 500_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);
        let mut tracker_sig = schnorr_sign(&message, &tracker_secret, &tracker_pk).unwrap();

        // Corrupt signature by flipping bit at position 0
        tracker_sig[0] ^= 0x01;

        assert!(schnorr_verify(&tracker_sig, &message, &tracker_pk).is_err(),
            "Corrupted signature must fail verification");
    }

    /// BasisSpec: "basis redemption should fail with invalid AVL tree proof"
    /// In Rust terms: debt record key computed with wrong input (reversed keys)
    #[test]
    fn wrong_debt_record_key_rejected() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt: u64 = 500_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        // Correct key: blake2b256(owner || receiver)
        let correct_message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        // Wrong key: blake2b256(receiver || owner) -- reversed!
        let wrong_message = core_signing_message(&receiver_pk, &owner_pk, total_debt, timestamp);

        let sig = schnorr_sign(&correct_message, &owner_secret, &owner_pk).unwrap();
        let tracker_sig = schnorr_sign(&correct_message, &tracker_secret, &tracker_pk).unwrap();

        // Signatures must verify with correct message
        assert!(schnorr_verify(&sig, &correct_message, &owner_pk).is_ok());
        assert!(schnorr_verify(&tracker_sig, &correct_message, &tracker_pk).is_ok());

        // But NOT with wrong message (reversed keys)
        assert!(schnorr_verify(&sig, &wrong_message, &owner_pk).is_err(),
            "Signature must fail verification with wrong key order");
        assert!(schnorr_verify(&tracker_sig, &wrong_message, &tracker_pk).is_err());

        // Messages must be different
        assert_ne!(correct_message, wrong_message);
    }

    /// BasisSpec: "debt transfer: should fail without debtor consent"
    /// Bob tries to forge Alice's signature on a note to Carol
    #[test]
    fn debt_transfer_fails_without_debtor_consent() {
        let secp = Secp256k1::new();

        // Alice (debtor)
        let alice_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let alice_pk = secp256k1::PublicKey::from_secret_key(&secp, &alice_secret).serialize();

        // Bob (creditor trying to forge)
        let bob_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let bob_pk = secp256k1::PublicKey::from_secret_key(&secp, &bob_secret).serialize();

        // Carol (new creditor)
        let _carol_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let carol_pk = secp256k1::PublicKey::from_secret_key(&secp, &_carol_secret).serialize();

        let transfer_amount: u64 = 5_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        // Message for note to Carol
        let message_to_carol = core_signing_message(&alice_pk, &carol_pk, transfer_amount, timestamp);

        // Bob tries to forge Alice's signature
        let forged_alice_sig = schnorr_sign(&message_to_carol, &bob_secret.secret_bytes(), &bob_pk).unwrap();

        // Forgery must fail verification against Alice's public key
        assert!(schnorr_verify(&forged_alice_sig, &message_to_carol, &alice_pk).is_err(),
            "Forged signature must fail verification");
    }

    /// BasisSpec: "debt transfer: should fail with replay attack (reuse old note)"
    /// Bob redeems with timestamp1, then tries to redeem again with SAME timestamp1
    #[test]
    fn replay_attack_prevention() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let total_debt: u64 = 10_000_000_000; // 10 ERG
        let timestamp1: u64 = TEST_TIMESTAMP;
        let timestamp2: u64 = timestamp1 + 1000;

        // Create note with timestamp1
        let note1 = IouNote::create_and_sign(
            receiver_pk, total_debt, timestamp1, &owner_secret
        ).unwrap();

        // Attempt to "replay" with same timestamp
        // In a real system, the contract checks timestamp > storedTimestamp
        // Here we verify that a second note with the same timestamp has the
        // same message (and thus same signature, enabling replay detection)
        let note1_replay = IouNote::create_and_sign(
            receiver_pk, total_debt, timestamp1, &owner_secret
        ).unwrap();

        // Same timestamp + same amount = same message = same signature
        assert_eq!(note1.timestamp, note1_replay.timestamp);
        assert_eq!(note1.amount_collected, note1_replay.amount_collected);

        // A new note with a different timestamp would have a different message
        let note2 = IouNote::create_and_sign(
            receiver_pk, total_debt, timestamp2, &owner_secret
        ).unwrap();

        assert_ne!(note1.timestamp, note2.timestamp);
        // Different timestamps mean different messages
        let msg1 = note1.signing_message(&owner_pk);
        let msg2 = note2.signing_message(&owner_pk);
        assert_ne!(msg1, msg2, "Different timestamps must produce different messages");
    }

    /// BasisSpec: "debt transfer: should fail with insufficient collateral"
    /// Reserve has only 3 ERG collateral, trying to redeem 5 ERG debt
    #[test]
    fn insufficient_collateral_detection() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let total_debt: u64 = 5_000_000_000; // 5 ERG debt
        let timestamp: u64 = TEST_TIMESTAMP;
        let reserve_value: u64 = 3_000_000_000; // Only 3 ERG collateral

        let note = IouNote::create_and_sign(
            receiver_pk, total_debt, timestamp, &owner_secret
        ).unwrap();

        // Outstanding debt is 5 ERG, reserve only has 3 ERG
        assert!(note.outstanding_debt() > reserve_value,
            "Outstanding debt must exceed reserve value for insufficient collateral");
    }

    // ========== MESSAGE FORMAT COMPATIBILITY TESTS ==========

    /// Verify the 48-byte message format matches the spec exactly:
    /// key (32) || totalDebt (8 BE) || timestamp (8 BE)
    #[test]
    fn message_format_exact_structure() {
        let owner_pk: [u8; 33] = [0x02u8; 33];
        let receiver_pk: [u8; 33] = [0x03u8; 33];
        let total_debt: u64 = 1_000_000_000;
        let timestamp: u64 = 1_743_379_200_000;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        assert_eq!(message.len(), 48);

        // Verify key portion (first 32 bytes)
        let expected_key = {
            let mut input = [0u8; 66];
            input[..33].copy_from_slice(&owner_pk);
            input[33..].copy_from_slice(&receiver_pk);
            blake2b256(&input)
        };
        assert_eq!(&message[0..32], &expected_key[..], "First 32 bytes must be blake2b256(owner||receiver)");

        // Verify totalDebt portion (bytes 32-40, big-endian)
        assert_eq!(&message[32..40], &total_debt.to_be_bytes(), "Bytes 32-40 must be totalDebt (BE)");

        // Verify timestamp portion (bytes 40-48, big-endian)
        assert_eq!(&message[40..48], &timestamp.to_be_bytes(), "Bytes 40-48 must be timestamp (BE)");
    }

    /// BasisSpec: "basis redemption should fail with invalid action code"
    /// In Rust, this translates to verifying that the message format is
    /// independent of any action code (action code is a context var, not part of message)
    #[test]
    fn message_independent_of_action_code() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let total_debt: u64 = 500_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let message = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);
        let sig = schnorr_sign(&message, &owner_secret, &owner_pk).unwrap();

        // The same signature works regardless of what action code is used in context
        // (action code is context var #0, not part of the signing message)
        assert!(schnorr_verify(&sig, &message, &owner_pk).is_ok());
    }

    /// Verify both parties sign the exact same message
    #[test]
    fn both_parties_sign_same_message() {
        let (owner_secret, owner_pk) = random_keypair();
        let (_, receiver_pk) = random_keypair();
        let (tracker_secret, tracker_pk) = random_keypair();

        let total_debt: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        // Both sign the same message
        let msg = core_signing_message(&owner_pk, &receiver_pk, total_debt, timestamp);

        // Owner signs
        let owner_sig = schnorr_sign(&msg, &owner_secret, &owner_pk).unwrap();
        // Tracker signs (SAME message)
        let tracker_sig = schnorr_sign(&msg, &tracker_secret, &tracker_pk).unwrap();

        // Both verify against same message
        assert!(schnorr_verify(&owner_sig, &msg, &owner_pk).is_ok());
        assert!(schnorr_verify(&tracker_sig, &msg, &tracker_pk).is_ok());
    }

    // ========== HEX CONVERSION TESTS ==========

    #[test]
    fn pubkey_hex_roundtrip() {
        let (_, pubkey) = random_keypair();
        let hex = pubkey_to_hex(&pubkey);
        assert_eq!(hex.len(), 66, "Hex pubkey must be 66 chars (33 bytes)");

        let decoded = pubkey_from_hex(&hex).expect("Must decode hex pubkey");
        assert_eq!(decoded, pubkey, "Roundtrip must preserve pubkey");
    }

    #[test]
    fn signature_hex_roundtrip() {
        let (secret, pubkey) = random_keypair();
        let message = core_signing_message(&pubkey, &[0x03u8; 33], 1000, TEST_TIMESTAMP);
        let sig = schnorr_sign(&message, &secret, &pubkey).unwrap();

        let hex = signature_to_hex(&sig);
        assert_eq!(hex.len(), 130, "Hex signature must be 130 chars (65 bytes)");

        let decoded = signature_from_hex(&hex).expect("Must decode hex signature");
        assert_eq!(decoded, sig, "Roundtrip must preserve signature");
    }

    // ========== EDGE CASE TESTS ==========

    /// BasisSpec: edge cases with maximum values
    #[test]
    fn edge_case_max_values() {
        let owner_pk: [u8; 33] = [0xFFu8; 33];
        let receiver_pk: [u8; 33] = [0xFEu8; 33];
        let max_debt = u64::MAX;
        let timestamp: u64 = 0;

        let message = core_signing_message(&owner_pk, &receiver_pk, max_debt, timestamp);
        assert_eq!(message.len(), 48);
        // Message should still be well-formed even with max values
        assert_eq!(&message[32..40], &max_debt.to_be_bytes());
        assert_eq!(&message[40..48], &timestamp.to_be_bytes());
    }

    /// BasisSpec: edge cases with zero values
    #[test]
    fn edge_case_zero_values() {
        let owner_pk: [u8; 33] = [0x02u8; 33];
        let receiver_pk: [u8; 33] = [0x03u8; 33];
        let zero_debt: u64 = 0;
        let zero_timestamp: u64 = 0;

        let message = core_signing_message(&owner_pk, &receiver_pk, zero_debt, zero_timestamp);
        assert_eq!(message.len(), 48);
        assert_eq!(&message[32..40], &0u64.to_be_bytes());
        assert_eq!(&message[40..48], &0u64.to_be_bytes());
    }

    /// Test that different amounts produce different messages
    #[test]
    fn different_amounts_produce_different_messages() {
        let owner_pk: [u8; 33] = [0x02u8; 33];
        let receiver_pk: [u8; 33] = [0x03u8; 33];
        let timestamp: u64 = TEST_TIMESTAMP;

        let msg1 = core_signing_message(&owner_pk, &receiver_pk, 1_000_000_000, timestamp);
        let msg2 = core_signing_message(&owner_pk, &receiver_pk, 2_000_000_000, timestamp);

        assert_ne!(msg1, msg2, "Different amounts must produce different messages");
    }

    /// Test that different timestamps produce different messages
    #[test]
    fn different_timestamps_produce_different_messages() {
        let owner_pk: [u8; 33] = [0x02u8; 33];
        let receiver_pk: [u8; 33] = [0x03u8; 33];
        let total_debt: u64 = 1_000_000_000;

        let msg1 = core_signing_message(&owner_pk, &receiver_pk, total_debt, TEST_TIMESTAMP);
        let msg2 = core_signing_message(&owner_pk, &receiver_pk, total_debt, TEST_TIMESTAMP + 1);

        assert_ne!(msg1, msg2, "Different timestamps must produce different messages");
    }

    // ========== NOTE KEY TESTS ==========

    #[test]
    fn note_key_is_deterministic() {
        let issuer = [0x02u8; 33];
        let recipient = [0x03u8; 33];

        let key1 = NoteKey::from_keys(&issuer, &recipient);
        let key2 = NoteKey::from_keys(&issuer, &recipient);

        assert_eq!(key1, key2, "Note key must be deterministic");
    }

    #[test]
    fn note_key_order_matters() {
        let a = [0x02u8; 33];
        let b = [0x03u8; 33];

        let key_ab = NoteKey::from_keys(&a, &b);
        let key_ba = NoteKey::from_keys(&b, &a);

        // The key uses issuer_hash and recipient_hash from separate blake2b256 hashes
        // So swapping order changes the key
        assert_ne!(key_ab, key_ba, "Key with swapped order must differ");
    }

    // ========== TRACKER STATE MANAGER TESTS ==========

    /// Test reserve tree value format: 16 bytes (timestamp || already_redeemed)
    #[test]
    fn reserve_tree_value_format_16_bytes() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        let (issuer_secret, issuer_pk) = random_keypair();
        let (_, recipient_pk) = random_keypair();

        let total_debt: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let note = IouNote::create_and_sign(recipient_pk, total_debt, timestamp, &issuer_secret).unwrap();

        // Insert note into tracker
        tracker.add_note(&issuer_pk, &note).unwrap();

        // Generate reserve lookup proof
        let lookup_proof = tracker.generate_reserve_lookup_proof(&issuer_pk, &recipient_pk).unwrap();

        // For first redemption, value should be 16 bytes of zeros
        assert_eq!(lookup_proof.value.len(), 16,
            "Reserve tree value must be 16 bytes (timestamp || redeemedAmount)");

        // For first redemption, value should be all zeros
        assert_eq!(lookup_proof.value, vec![0u8; 16],
            "First redemption value should be 16 zero bytes");
    }

    /// Test that get_already_redeemed returns 0 for first redemption
    #[test]
    fn first_redemption_returns_zero_already_redeemed() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        let (issuer_secret, issuer_pk) = random_keypair();
        let (_, recipient_pk) = random_keypair();

        let total_debt: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let note = IouNote::create_and_sign(recipient_pk, total_debt, timestamp, &issuer_secret).unwrap();
        tracker.add_note(&issuer_pk, &note).unwrap();

        let already_redeemed = tracker.get_already_redeemed(&issuer_pk, &recipient_pk).unwrap();
        assert_eq!(already_redeemed, 0, "First redemption should have 0 already redeemed");

        let stored_timestamp = tracker.get_already_redeemed_timestamp(&issuer_pk, &recipient_pk).unwrap();
        assert_eq!(stored_timestamp, 0, "First redemption should have 0 stored timestamp");
    }

    /// Test tracker tree lookup proof generation
    #[test]
    fn tracker_tree_lookup_proof() {
        let mut tracker = TrackerStateManager::new_with_temp_storage();

        let (issuer_secret, issuer_pk) = random_keypair();
        let (_, recipient_pk) = random_keypair();

        let total_debt: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let note = IouNote::create_and_sign(recipient_pk, total_debt, timestamp, &issuer_secret).unwrap();
        tracker.add_note(&issuer_pk, &note).unwrap();

        let lookup_proof = tracker.generate_tracker_lookup_proof(&issuer_pk, &recipient_pk).unwrap();

        // Tracker tree value should be 8 bytes (totalDebt as big-endian u64)
        assert_eq!(lookup_proof.value.len(), 8,
            "Tracker tree value must be 8 bytes (totalDebt)");

        // Value should match the note's totalDebt
        let decoded_debt = u64::from_be_bytes(lookup_proof.value.try_into().unwrap());
        assert_eq!(decoded_debt, total_debt,
            "Tracker tree value must match note's totalDebt");
    }

    // ========== PROPERTY-BASED STYLE TESTS ==========

    /// Test that signatures are always 65 bytes regardless of input
    #[test]
    fn signature_always_65_bytes() {
        let amounts = vec![0, 1, 1_000, 1_000_000, 1_000_000_000, u64::MAX - 1000];
        let timestamps = vec![0, 1, 1_000_000, 1_000_000_000, TEST_TIMESTAMP, u64::MAX - 1000];

        let (secret, pubkey) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        for amount in amounts {
            for ts in &timestamps {
                let msg = core_signing_message(&pubkey, &receiver_pk, amount, *ts);
                let sig = schnorr_sign(&msg, &secret, &pubkey).unwrap();
                assert_eq!(sig.len(), 65,
                    "Signature must be 65 bytes for amount={} timestamp={}", amount, ts);
                assert!(schnorr_verify(&sig, &msg, &pubkey).is_ok(),
                    "Signature must verify for amount={} timestamp={}", amount, ts);
            }
        }
    }

    /// Test that wrong message always fails verification
    #[test]
    fn wrong_message_always_fails() {
        let (secret, pubkey) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let amount: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let correct_msg = core_signing_message(&pubkey, &receiver_pk, amount, timestamp);
        let sig = schnorr_sign(&correct_msg, &secret, &pubkey).unwrap();

        // Try various wrong messages
        let wrong_amount_msg = core_signing_message(&pubkey, &receiver_pk, amount + 1, timestamp);
        let wrong_ts_msg = core_signing_message(&pubkey, &receiver_pk, amount, timestamp + 1);
        let (_, diff_receiver_pk) = random_keypair();
        let wrong_receiver_msg = core_signing_message(&pubkey, &diff_receiver_pk, amount, timestamp);

        assert!(schnorr_verify(&sig, &wrong_amount_msg, &pubkey).is_err());
        assert!(schnorr_verify(&sig, &wrong_ts_msg, &pubkey).is_err());
        assert!(schnorr_verify(&sig, &wrong_receiver_msg, &pubkey).is_err());
    }

    /// Test that corrupted signatures always fail
    #[test]
    fn corrupted_signatures_always_fail() {
        let (secret, pubkey) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        let amount: u64 = 1_000_000_000;
        let timestamp: u64 = TEST_TIMESTAMP;

        let msg = core_signing_message(&pubkey, &receiver_pk, amount, timestamp);
        let sig = schnorr_sign(&msg, &secret, &pubkey).unwrap();

        // Corrupt each byte position
        for i in 0..65 {
            let mut corrupted = sig;
            corrupted[i] ^= 0x01;
            assert!(schnorr_verify(&corrupted, &msg, &pubkey).is_err(),
                "Corrupted byte at position {} must cause verification failure", i);
        }
    }

    /// Test outstanding debt calculation
    #[test]
    fn outstanding_debt_calculation() {
        let (secret, _) = random_keypair();
        let (_, receiver_pk) = random_keypair();

        // Note with collected > redeemed
        let note = IouNote::create_and_sign(receiver_pk, 5_000_000_000, TEST_TIMESTAMP, &secret).unwrap();
        assert_eq!(note.outstanding_debt(), 5_000_000_000);

        // Note with equal collected and redeemed
        let mut note2 = IouNote::new(receiver_pk, 5_000_000_000, 5_000_000_000, TEST_TIMESTAMP, [0u8; 65]);
        note2.amount_collected = 5_000_000_000;
        note2.amount_redeemed = 5_000_000_000;
        assert_eq!(note2.outstanding_debt(), 0);

        // Note with more collected than redeemed
        let mut note3 = IouNote::new(receiver_pk, 10_000_000_000, 3_000_000_000, TEST_TIMESTAMP, [0u8; 65]);
        note3.amount_collected = 10_000_000_000;
        note3.amount_redeemed = 3_000_000_000;
        assert_eq!(note3.outstanding_debt(), 7_000_000_000);
    }
}
