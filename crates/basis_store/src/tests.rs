// Manual test runner functions

use crate::{blake2b256_hash, schnorr_tests, IouNote, NoteKey};

pub fn run_all_tests() -> Result<(), String> {
    println!("Running Basis Store tests...");

    test_iou_note_creation()?;
    test_signing_message()?;
    test_note_key_generation()?;
    test_signature_verification()?;
    test_simple_hash_consistency()?;
    test_roundtrip_signature()?;
    test_signature_tampering()?;
    test_multiple_signatures()?;
    test_timestamp_validation_future_timestamp()?;
    test_timestamp_validation_increasing_timestamps()?;
    test_timestamp_validation_non_increasing_timestamps()?;
    test_different_issuer_recipient_pairs_allow_same_timestamps()?;
    schnorr_tests::run_schnorr_test_vectors()?;

    println!("All tests passed!");
    Ok(())
}

fn test_iou_note_creation() -> Result<(), String> {
    let recipient_pubkey = [1u8; 33];
    let signature = [2u8; 65];

    let note = IouNote::new(recipient_pubkey, 1000, 0, 1234567890, signature);

    if note.recipient_pubkey != recipient_pubkey {
        return Err("recipient_pubkey mismatch".to_string());
    }
    if note.amount_collected != 1000 {
        return Err("amount_collected mismatch".to_string());
    }
    if note.amount_redeemed != 0 {
        return Err("amount_redeemed mismatch".to_string());
    }
    if note.timestamp != 1234567890 {
        return Err("timestamp mismatch".to_string());
    }
    if note.signature != signature {
        return Err("signature mismatch".to_string());
    }

    println!("✓ test_iou_note_creation passed");
    Ok(())
}

fn test_signing_message() -> Result<(), String> {
    let owner_pubkey = [1u8; 33];
    let receiver_pubkey = [2u8; 33];
    let timestamp = 1743379200000u64;
    let note = IouNote::new(receiver_pubkey, 1000, 0, timestamp, [3u8; 65]);

    // Format: key (32) || totalDebt (8) || timestamp (8) = 48 bytes
    let message = note.signing_message(&owner_pubkey);
    if message.is_empty() {
        return Err("signing message is empty".to_string());
    }

    if message.len() != 48 {
        return Err(format!(
            "signing message should be 48 bytes, got {}",
            message.len()
        ));
    }

    println!("✓ test_signing_message passed");
    Ok(())
}

fn test_note_key_generation() -> Result<(), String> {
    let issuer_pubkey = [1u8; 33];
    let recipient_pubkey = [2u8; 33];

    let note_key = NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey);
    let note_key_reverse = NoteKey::from_keys(&recipient_pubkey, &issuer_pubkey);

    if note_key.key_hash == note_key_reverse.key_hash {
        return Err("issuer+recipient and recipient+issuer hashes should be different".to_string());
    }

    let note_key2 = NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey);
    if note_key.key_hash != note_key2.key_hash {
        return Err("key hash should be consistent for same inputs".to_string());
    }

    println!("✓ test_note_key_generation passed");
    Ok(())
}

fn test_signature_verification() -> Result<(), String> {
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();

    // Generate a test key pair
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let issuer_pubkey = public_key.serialize();

    // Create a test note
    let recipient_pubkey = [2u8; 33];
    let amount = 1000u64;
    let timestamp = 1234567890u64;

    // Create a valid signature using our implementation
    let note = IouNote::create_and_sign(recipient_pubkey, amount, timestamp, &[1u8; 32])
        .expect("Failed to create valid signature");

    // Test valid signature (basic format validation)
    if note.verify_signature(&issuer_pubkey).is_err() {
        return Err("should pass with valid signature format".to_string());
    }

    // Test invalid signature (all zeros)
    let invalid_note = IouNote::new(recipient_pubkey, amount, 0, timestamp, [0u8; 65]);

    if invalid_note.verify_signature(&issuer_pubkey).is_ok() {
        return Err("should fail with zero signature".to_string());
    }

    println!("✓ test_signature_verification passed");
    Ok(())
}

fn test_simple_hash_consistency() -> Result<(), String> {
    let data1 = [1u8; 33];
    let data2 = [2u8; 33];

    let hash1 = blake2b256_hash(&data1);
    let hash2 = blake2b256_hash(&data1);
    if hash1 != hash2 {
        return Err("same input should produce same hash".to_string());
    }

    let hash3 = blake2b256_hash(&data2);
    if hash1 == hash3 {
        return Err("different input should produce different hash".to_string());
    }

    println!("✓ test_simple_hash_consistency passed");
    Ok(())
}

fn test_roundtrip_signature() -> Result<(), String> {
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();

    // Generate test key pair
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let issuer_pubkey = public_key.serialize();

    // Test data
    let recipient_pubkey = [2u8; 33];
    let amount = 1000u64;
    let timestamp = 1234567890u64;

    // Create and sign a note
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount,
        timestamp,
        &secret_key.secret_bytes(),
    )
    .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;

    // Verify the signature
    println!("Testing signature verification...");
    println!("Signature: {:?}", note.signature);
    println!("Issuer pubkey: {:?}", issuer_pubkey);

    note.verify_signature(&issuer_pubkey)
        .map_err(|e| format!("Signature verification failed: {:?}", e))?;

    // Verify note data is correct
    if note.recipient_pubkey != recipient_pubkey {
        return Err("recipient_pubkey mismatch".to_string());
    }
    if note.amount_collected != amount {
        return Err("amount_collected mismatch".to_string());
    }
    if note.timestamp != timestamp {
        return Err("timestamp mismatch".to_string());
    }

    println!("✓ test_roundtrip_signature passed");
    Ok(())
}

fn test_signature_tampering() -> Result<(), String> {
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();

    // Generate test key pair
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let issuer_pubkey = public_key.serialize();

    // Create and sign a valid note
    let mut note =
        IouNote::create_and_sign([2u8; 33], 1000, 1234567890, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;

    // Test 1: Tamper with signature
    note.signature[0] ^= 0x01; // Flip a bit in the signature
    if note.verify_signature(&issuer_pubkey).is_ok() {
        return Err("Tampered signature should fail verification".to_string());
    }

    // Test 2: Tamper with recipient
    let mut note2 =
        IouNote::create_and_sign([2u8; 33], 1000, 1234567890, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    note2.recipient_pubkey[0] ^= 0x01;
    if note2.verify_signature(&issuer_pubkey).is_ok() {
        return Err("Tampered recipient should fail verification".to_string());
    }

    // Test 3: Tamper with amount
    let mut note3 =
        IouNote::create_and_sign([2u8; 33], 1000, 1234567890, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    note3.amount_collected = 2000;
    if note3.verify_signature(&issuer_pubkey).is_ok() {
        return Err("Tampered amount should fail verification".to_string());
    }

    // Test 4: Wrong issuer public key
    let wrong_pubkey = [0u8; 33];
    let note4 = IouNote::create_and_sign([2u8; 33], 1000, 1234567890, &secret_key.secret_bytes())
        .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    if note4.verify_signature(&wrong_pubkey).is_ok() {
        return Err("Wrong issuer pubkey should fail verification".to_string());
    }

    println!("✓ test_signature_tampering passed");
    Ok(())
}

fn test_multiple_signatures() -> Result<(), String> {
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();

    // Generate multiple key pairs
    let secret_key1 = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key1 = secp256k1::PublicKey::from_secret_key(&secp, &secret_key1);
    let issuer_pubkey1 = public_key1.serialize();

    let secret_key2 = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key2 = secp256k1::PublicKey::from_secret_key(&secp, &secret_key2);
    let issuer_pubkey2 = public_key2.serialize();

    // Create notes with different issuers
    let note1 = IouNote::create_and_sign([2u8; 33], 1000, 1234567890, &secret_key1.secret_bytes())
        .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;

    let note2 = IouNote::create_and_sign([2u8; 33], 2000, 1234567891, &secret_key2.secret_bytes())
        .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;

    // Verify each note with its correct issuer
    note1
        .verify_signature(&issuer_pubkey1)
        .map_err(|e| format!("Note1 verification failed: {:?}", e))?;

    note2
        .verify_signature(&issuer_pubkey2)
        .map_err(|e| format!("Note2 verification failed: {:?}", e))?;

    // Verify that notes fail with wrong issuers
    if note1.verify_signature(&issuer_pubkey2).is_ok() {
        return Err("Note1 should fail with issuer2 pubkey".to_string());
    }

    if note2.verify_signature(&issuer_pubkey1).is_ok() {
        return Err("Note2 should fail with issuer1 pubkey".to_string());
    }

    println!("✓ test_multiple_signatures passed");
    Ok(())
}

fn test_timestamp_validation_future_timestamp() -> Result<(), String> {
    use crate::{IouNote, PubKey, TrackerStateManager};

    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let issuer_pubkey: PubKey = [1u8; 33];
    let recipient_pubkey: PubKey = [2u8; 33];

    // Create a note with a far future timestamp (in milliseconds)
    let note = IouNote::new(
        recipient_pubkey,
        1000,
        0,
        9999999999999, // Far future timestamp in milliseconds
        [0u8; 65],
    );

    // Should fail with FutureTimestamp error
    let result = tracker.add_note(&issuer_pubkey, &note);
    match result {
        Err(crate::NoteError::FutureTimestamp) => Ok(()),
        _ => Err("Expected FutureTimestamp error".to_string()),
    }
}

fn test_timestamp_validation_increasing_timestamps() -> Result<(), String> {
    use crate::{IouNote, PubKey, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let issuer_pubkey_bytes = secp256k1::PublicKey::from_secret_key(&secp, &secret_key).serialize();

    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let recipient_pubkey: PubKey = [2u8; 33];

    // Create first signed note
    let note1 =
        IouNote::create_and_sign(recipient_pubkey, 1000, 1000000, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create first note: {:?}", e))?;

    let result1 = tracker.add_note(&issuer_pubkey_bytes, &note1);
    if result1.is_err() {
        return Err(format!("First note should succeed: {:?}", result1.err()));
    }

    // Create second signed note with higher timestamp
    let note2 =
        IouNote::create_and_sign(recipient_pubkey, 2000, 1000001, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create second note: {:?}", e))?;

    let result2 = tracker.add_note(&issuer_pubkey_bytes, &note2);
    if result2.is_err() {
        return Err(format!(
            "Second note with higher timestamp should succeed: {:?}",
            result2.err()
        ));
    }

    Ok(())
}

fn test_timestamp_validation_non_increasing_timestamps() -> Result<(), String> {
    use crate::{IouNote, PubKey, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let issuer_pubkey_bytes = secp256k1::PublicKey::from_secret_key(&secp, &secret_key).serialize();

    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let recipient_pubkey: PubKey = [2u8; 33];

    // Add first signed note
    let note1 =
        IouNote::create_and_sign(recipient_pubkey, 1000, 1000000, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create first note: {:?}", e))?;

    let result1 = tracker.add_note(&issuer_pubkey_bytes, &note1);
    if result1.is_err() {
        return Err(format!("First note should succeed: {:?}", result1.err()));
    }

    // Try to add note with same timestamp - should fail
    let note2 =
        IouNote::create_and_sign(recipient_pubkey, 2000, 1000000, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create second note: {:?}", e))?;

    let result2 = tracker.add_note(&issuer_pubkey_bytes, &note2);
    match result2 {
        Err(crate::NoteError::PastTimestamp) => {} // Expected
        _ => {
            return Err(format!(
                "Expected PastTimestamp error for same timestamp, got: {:?}",
                result2.err()
            ))
        }
    }

    // Try to add note with lower timestamp - should fail
    let note3 =
        IouNote::create_and_sign(recipient_pubkey, 2000, 999999, &secret_key.secret_bytes())
            .map_err(|e| format!("Failed to create third note: {:?}", e))?;

    let result3 = tracker.add_note(&issuer_pubkey_bytes, &note3);
    match result3 {
        Err(crate::NoteError::PastTimestamp) => Ok(()), // Expected
        _ => Err(format!(
            "Expected PastTimestamp error for lower timestamp, got: {:?}",
            result3.err()
        )),
    }
}

fn test_different_issuer_recipient_pairs_allow_same_timestamps() -> Result<(), String> {
    use crate::{IouNote, PubKey, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();
    let secret_key1 = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let secret_key2 = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let issuer1_pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret_key1).serialize();
    let issuer2_pubkey = secp256k1::PublicKey::from_secret_key(&secp, &secret_key2).serialize();

    let mut tracker = TrackerStateManager::new_with_temp_storage();
    let recipient_pubkey: PubKey = [3u8; 33];

    // Add note for first issuer
    let note1 =
        IouNote::create_and_sign(recipient_pubkey, 1000, 1000000, &secret_key1.secret_bytes())
            .map_err(|e| format!("Failed to create first note: {:?}", e))?;

    let result1 = tracker.add_note(&issuer1_pubkey, &note1);
    if result1.is_err() {
        return Err(format!("First note should succeed: {:?}", result1.err()));
    }

    // Add note for different issuer with same timestamp - should succeed
    let note2 =
        IouNote::create_and_sign(recipient_pubkey, 1000, 1000000, &secret_key2.secret_bytes())
            .map_err(|e| format!("Failed to create second note: {:?}", e))?;

    let result2 = tracker.add_note(&issuer2_pubkey, &note2);
    if result2.is_err() {
        return Err("Note with same timestamp but different issuer should succeed".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod test_module {
    use crate::schnorr_tests;

    #[test]
    fn test_iou_note_creation() {
        super::test_iou_note_creation().unwrap();
    }

    #[test]
    fn test_signing_message() {
        super::test_signing_message().unwrap();
    }

    #[test]
    fn test_note_key_generation() {
        super::test_note_key_generation().unwrap();
    }

    #[test]
    fn test_signature_verification() {
        super::test_signature_verification().unwrap();
    }

    #[test]
    fn test_simple_hash_consistency() {
        super::test_simple_hash_consistency().unwrap();
    }

    #[test]
    fn test_schnorr_test_vectors() {
        schnorr_tests::run_schnorr_test_vectors().unwrap();
    }

    #[test]
    fn test_roundtrip_signature() {
        super::test_roundtrip_signature().unwrap();
    }

    #[test]
    fn test_signature_tampering() {
        super::test_signature_tampering().unwrap();
    }

    #[test]
    fn test_multiple_signatures() {
        super::test_multiple_signatures().unwrap();
    }

    #[test]
    fn test_timestamp_validation_future_timestamp() {
        super::test_timestamp_validation_future_timestamp().unwrap();
    }

    #[test]
    fn test_timestamp_validation_increasing_timestamps() {
        super::test_timestamp_validation_increasing_timestamps().unwrap();
    }

    #[test]
    fn test_timestamp_validation_non_increasing_timestamps() {
        super::test_timestamp_validation_non_increasing_timestamps().unwrap();
    }

    #[test]
    fn test_different_issuer_recipient_pairs_allow_same_timestamps() {
        super::test_different_issuer_recipient_pairs_allow_same_timestamps().unwrap();
    }
}

#[cfg(test)]
mod confirmation_state_tests {
    use crate::{
        FreshGenerationApproval, IouNote, NoteConfirmationStatus, TrackerGenerationConfig,
        TrackerStateManager,
    };
    use secp256k1::{Secp256k1, SecretKey};

    fn make_manager() -> TrackerStateManager {
        TrackerStateManager::new_with_temp_storage()
    }

    fn generation(fresh_generation: FreshGenerationApproval) -> TrackerGenerationConfig {
        TrackerGenerationConfig {
            tracker_nft_id: [0x42; 32],
            fresh_generation,
        }
    }

    fn issuer_pubkey(secret_key: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret_key).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn create_note(
        issuer_secret: &[u8; 32],
        recipient: &[u8; 33],
        amount: u64,
        timestamp: u64,
    ) -> IouNote {
        IouNote::create_and_sign(*recipient, amount, timestamp, issuer_secret).unwrap()
    }

    #[test]
    fn fresh_note_is_local_only() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::LocalOnly);
        assert_eq!(confirmation.confirmed_total_debt, None);
        assert_eq!(confirmation.pending_total_debt, None);
    }

    #[test]
    fn mark_notes_pending_transitions_to_pending() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();

        let digest = manager.get_state().avl_root_digest;
        let tx_id = "11".repeat(32);
        let count = manager.mark_notes_pending(digest, &tx_id, 100).unwrap();
        assert_eq!(count, 1);

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Pending);
        assert_eq!(confirmation.pending_total_debt, Some(1000));
        assert_eq!(confirmation.pending_tx_id, Some(tx_id));
    }

    #[test]
    fn invalid_publication_tx_id_is_rejected_without_quarantining_state() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();
        let digest = manager.validated_state().unwrap().avl_root_digest;

        assert!(matches!(
            manager.mark_notes_pending(digest, "not-a-transaction-id", 100),
            Err(crate::NoteError::InvalidTransactionId)
        ));
        assert!(manager.is_healthy());
        assert!(manager.pending_publication().unwrap().is_none());
        assert_eq!(
            manager
                .get_confirmation(&issuer, &recipient)
                .unwrap()
                .status,
            NoteConfirmationStatus::LocalOnly
        );
    }

    #[test]
    fn pending_publication_survives_restart_and_binds_confirmation_tx_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let tx_id = "11".repeat(32);
        let digest;

        {
            let mut manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
                .unwrap();
            digest = manager.validated_state().unwrap().avl_root_digest;
            manager.mark_notes_pending(digest, &tx_id, 100).unwrap();
            let key: [u8; 32] = crate::NoteKey::from_keys(&issuer, &recipient)
                .to_bytes()
                .try_into()
                .unwrap();
            // Model a crash after the durable external-effect receipt but
            // before this advisory confirmation row reached storage.
            manager.storage.remove_confirmation_for_test(&key).unwrap();
        }

        let mut reopened = TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Deny),
        )
        .unwrap();
        assert_eq!(
            reopened.pending_publication().unwrap(),
            Some(crate::PendingTrackerPublication {
                digest,
                tx_id: tx_id.clone(),
                submitted_height: 100,
            })
        );
        assert_eq!(
            reopened
                .get_confirmation(&issuer, &recipient)
                .unwrap()
                .status,
            NoteConfirmationStatus::LocalOnly
        );
        assert!(matches!(
            reopened.confirm_pending_publication(&"22".repeat(32), "box123", 200),
            Err(crate::NoteError::PublicationLeaseMismatch)
        ));
        reopened
            .confirm_pending_publication(&tx_id, "box123", 200)
            .unwrap();
        assert!(reopened.pending_publication().unwrap().is_none());
        let confirmed = reopened.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmed.status, NoteConfirmationStatus::Confirmed);
        assert_eq!(confirmed.confirmed_total_debt, Some(1000));
        assert_eq!(confirmed.confirmed_box_id.as_deref(), Some("box123"));
        assert_eq!(confirmed.confirmed_height, Some(200));
    }

    #[test]
    fn confirm_pending_notes_promotes_to_confirmed() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager
            .mark_notes_pending(digest, &"11".repeat(32), 100)
            .unwrap();
        manager.confirm_pending_notes("box123", 200).unwrap();

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Confirmed);
        assert_eq!(confirmation.confirmed_total_debt, Some(1000));
        assert_eq!(confirmation.confirmed_box_id, Some("box123".to_string()));
        assert_eq!(confirmation.confirmed_height, Some(200));
        assert_eq!(confirmation.pending_total_debt, None);
        assert_eq!(confirmation.pending_tx_id, None);
    }

    #[test]
    fn rollback_of_accepted_a_preserves_newer_pending_b_fail_closed() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1_000, 1))
            .unwrap();
        let root_a = manager.validated_state().unwrap().avl_root_digest;
        let tx_a = "11".repeat(32);
        manager.mark_notes_pending(root_a, &tx_a, 100).unwrap();
        let effect_a = crate::chain_reconciliation::validated_tracker_effect_for_test(
            "22".repeat(32),
            tx_a.clone(),
            "33".repeat(32),
            "44".repeat(32),
            101,
            6,
            root_a,
        );
        manager.confirm_validated_publication(&effect_a).unwrap();

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1_200, 2))
            .unwrap();
        let root_b = manager.validated_state().unwrap().avl_root_digest;
        let tx_b = "55".repeat(32);
        manager.mark_notes_pending(root_b, &tx_b, 108).unwrap();
        let rollback_a = crate::chain_reconciliation::validated_rollback_for_test(&effect_a);
        assert_eq!(
            manager.rollback_validated_publication(&rollback_a).unwrap(),
            1
        );
        assert_eq!(
            manager.rollback_validated_publication(&rollback_a).unwrap(),
            0
        );

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Pending);
        assert_eq!(confirmation.confirmed_total_debt, None);
        assert_eq!(confirmation.confirmed_tx_id, None);
        assert_eq!(confirmation.pending_total_debt, Some(1_200));
        assert_eq!(confirmation.pending_tx_id.as_deref(), Some(tx_b.as_str()));
        assert_eq!(
            manager.pending_publication().unwrap().unwrap().tx_id(),
            tx_b
        );
        assert_eq!(confirmation.redeemable_amount(0), 0);
    }

    #[test]
    fn restart_restores_historical_anchor_without_promoting_newer_local_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let unchanged_recipient = [2u8; 33];
        let changed_recipient = [3u8; 33];
        let later_recipient = [4u8; 33];
        let effect_a;
        let root_a;
        let root_b;

        {
            let mut manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            manager
                .add_note(
                    &issuer,
                    &create_note(&issuer_secret, &unchanged_recipient, 1_000, 1),
                )
                .unwrap();
            manager
                .add_note(
                    &issuer,
                    &create_note(&issuer_secret, &changed_recipient, 1_000, 1),
                )
                .unwrap();
            root_a = manager.validated_state().unwrap().avl_root_digest;
            let tx_a = "11".repeat(32);
            manager.mark_notes_pending(root_a, &tx_a, 100).unwrap();
            effect_a = crate::chain_reconciliation::validated_tracker_effect_for_test(
                "22".repeat(32),
                tx_a,
                "33".repeat(32),
                "44".repeat(32),
                101,
                6,
                root_a,
            );
            manager.confirm_validated_publication(&effect_a).unwrap();

            manager
                .add_note(
                    &issuer,
                    &create_note(&issuer_secret, &changed_recipient, 1_200, 2),
                )
                .unwrap();
            manager
                .add_note(
                    &issuer,
                    &create_note(&issuer_secret, &later_recipient, 500, 3),
                )
                .unwrap();
            root_b = manager.validated_state().unwrap().avl_root_digest;
            assert_ne!(root_a, root_b);
        }

        let mut reopened = TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Deny),
        )
        .unwrap();
        assert!(reopened
            .validated_confirmation_anchor()
            .unwrap()
            .unwrap()
            .matches_validated_effect(&effect_a));
        assert_eq!(reopened.validated_state().unwrap().avl_root_digest, root_b);

        reopened.confirm_validated_publication(&effect_a).unwrap();
        assert_eq!(reopened.validated_state().unwrap().avl_root_digest, root_b);
        assert_eq!(
            reopened
                .get_confirmation(&issuer, &unchanged_recipient)
                .unwrap()
                .status,
            NoteConfirmationStatus::Confirmed
        );
        assert_eq!(
            reopened
                .get_confirmation(&issuer, &changed_recipient)
                .unwrap()
                .status,
            NoteConfirmationStatus::LocalOnly
        );
        assert_eq!(
            reopened
                .get_confirmation(&issuer, &later_recipient)
                .unwrap()
                .status,
            NoteConfirmationStatus::LocalOnly
        );
    }

    #[test]
    fn historical_confirmation_value_and_root_tampering_fail_independently() {
        fn confirmed_manager() -> (TrackerStateManager, [u8; 33], [u8; 33], [u8; 33]) {
            let mut manager = make_manager();
            let issuer_secret = [1u8; 32];
            let issuer = issuer_pubkey(&issuer_secret);
            let recipient = [2u8; 33];
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1_000, 1))
                .unwrap();
            let root = manager.validated_state().unwrap().avl_root_digest;
            let tx = "11".repeat(32);
            manager.mark_notes_pending(root, &tx, 100).unwrap();
            let effect = crate::chain_reconciliation::validated_tracker_effect_for_test(
                "22".repeat(32),
                tx,
                "33".repeat(32),
                "44".repeat(32),
                101,
                6,
                root,
            );
            manager.confirm_validated_publication(&effect).unwrap();
            (manager, issuer, recipient, root)
        }

        let (mut wrong_value, issuer, recipient, _) = confirmed_manager();
        let key: [u8; 32] = crate::NoteKey::from_keys(&issuer, &recipient)
            .to_bytes()
            .try_into()
            .unwrap();
        let mut record = wrong_value.get_confirmation(&issuer, &recipient).unwrap();
        record.confirmed_total_debt = Some(999);
        wrong_value
            .storage
            .store_confirmation(&key, &record)
            .unwrap();
        wrong_value.confirmations.insert(key, record);
        assert!(matches!(
            wrong_value.validated_confirmation_anchor(),
            Err(crate::NoteError::StorageError(message))
                if message.contains("do not reproduce")
        ));

        let (mut wrong_root, issuer, recipient, root) = confirmed_manager();
        let key: [u8; 32] = crate::NoteKey::from_keys(&issuer, &recipient)
            .to_bytes()
            .try_into()
            .unwrap();
        let mut record = wrong_root.get_confirmation(&issuer, &recipient).unwrap();
        let mut mutated_root = root;
        mutated_root[0] ^= 0x01;
        record.confirmed_root = Some(mutated_root.to_vec());
        wrong_root
            .storage
            .store_confirmation(&key, &record)
            .unwrap();
        wrong_root.confirmations.insert(key, record);
        assert!(matches!(
            wrong_root.validated_confirmation_anchor(),
            Err(crate::NoteError::StorageError(message))
                if message.contains("does not match the durable projection")
        ));

        let (mut missing_row, issuer, recipient, _) = confirmed_manager();
        let key: [u8; 32] = crate::NoteKey::from_keys(&issuer, &recipient)
            .to_bytes()
            .try_into()
            .unwrap();
        missing_row
            .storage
            .remove_confirmation_for_test(&key)
            .unwrap();
        missing_row.confirmations.remove(&key);
        assert!(matches!(
            missing_row.validated_confirmation_anchor(),
            Err(crate::NoteError::StorageError(message))
                if message.contains("do not reproduce")
        ));

        let (orphan_rows, _, _, _) = confirmed_manager();
        orphan_rows.storage.clear_confirmed_projection().unwrap();
        assert!(orphan_rows.has_persisted_confirmation_history().unwrap());
        assert!(orphan_rows
            .validated_confirmation_anchor()
            .unwrap()
            .is_none());
    }

    #[test]
    fn revert_pending_notes_returns_to_local_only() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager
            .mark_notes_pending(digest, &"11".repeat(32), 100)
            .unwrap();
        manager.revert_pending_notes().unwrap();

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::LocalOnly);
        assert_eq!(confirmation.confirmed_total_debt, None);
        assert_eq!(confirmation.pending_total_debt, None);
        assert_eq!(confirmation.pending_tx_id, None);
    }

    #[test]
    fn reconcile_matching_digest_confirms_all_notes() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();
        let digest = manager.get_state().avl_root_digest;
        let count = manager
            .reconcile_with_confirmed_digest(&digest, "box456", 300)
            .unwrap();
        assert_eq!(count, 1);

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Confirmed);
        assert_eq!(confirmation.confirmed_total_debt, Some(1000));
        assert_eq!(confirmation.confirmed_box_id, Some("box456".to_string()));
        assert_eq!(confirmation.confirmed_height, Some(300));
    }

    #[test]
    fn non_matching_digest_does_not_confirm() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();
        let other_digest = [0u8; 33];
        let count = manager
            .reconcile_with_confirmed_digest(&other_digest, "box789", 300)
            .unwrap();
        assert_eq!(count, 0);

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::LocalOnly);
    }

    #[test]
    fn rebuild_confirmations_preserves_durable_pending_state() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager
            .mark_notes_pending(digest, &"11".repeat(32), 100)
            .unwrap();

        manager.rebuild_confirmations().unwrap();

        let confirmation = manager.get_confirmation(&issuer, &recipient).unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Pending);
        assert_eq!(confirmation.pending_total_debt, Some(1000));
        assert_eq!(confirmation.pending_tx_id, Some("11".repeat(32)));
    }

    #[test]
    fn note_confirmation_redeemable_amount() {
        let confirmation = crate::NoteConfirmation {
            status: NoteConfirmationStatus::Confirmed,
            confirmed_total_debt: Some(1000),
            pending_total_debt: None,
            confirmed_box_id: Some("11".repeat(32)),
            confirmed_height: Some(100),
            confirmed_tx_id: Some("22".repeat(32)),
            confirmed_block_id: Some("33".repeat(32)),
            confirmed_successor_depth: Some(6),
            confirmed_intent_id: Some("44".repeat(32)),
            confirmed_root: Some(vec![0x55; 33]),
            pending_tx_id: None,
        };

        assert!(confirmation.is_redeemable(0));
        assert_eq!(confirmation.redeemable_amount(0), 1000);
        assert!(confirmation.is_redeemable(500));
        assert_eq!(confirmation.redeemable_amount(500), 500);
        assert!(!confirmation.is_redeemable(1000));
        assert_eq!(confirmation.redeemable_amount(1000), 0);
        assert!(!confirmation.is_redeemable(1500));
        assert_eq!(confirmation.redeemable_amount(1500), 0);
    }

    #[test]
    fn projected_issuer_gross_debt_aggregates_and_replaces_once() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 40, 1))
            .unwrap();
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_c, 40, 1))
            .unwrap();

        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, Some(&recipient_b), 70)
                .unwrap(),
            110
        );
    }

    #[test]
    fn projected_issuer_gross_debt_counts_a_new_recipient() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 60, 1))
            .unwrap();

        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, Some(&recipient_c), 50)
                .unwrap(),
            110
        );
    }

    #[test]
    fn projected_issuer_gross_debt_never_drops_confirmed_value() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager
            .mark_notes_pending(digest, &"11".repeat(32), 100)
            .unwrap();
        manager.confirm_pending_notes("box123", 200).unwrap();

        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, Some(&recipient), 50)
                .unwrap(),
            100
        );
    }

    #[test]
    fn cumulative_debt_regression_is_rejected_after_confirmation() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager
            .mark_notes_pending(digest, &"11".repeat(32), 100)
            .unwrap();
        manager.confirm_pending_notes("box123", 200).unwrap();

        assert!(matches!(
            manager.add_note(&issuer, &create_note(&issuer_secret, &recipient, 40, 2)),
            Err(crate::NoteError::DebtRegression)
        ));
        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, Some(&recipient), 50)
                .unwrap(),
            100
        );
    }

    #[test]
    fn settlement_progress_preserves_signed_note_and_is_bounded() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();

        let before = manager.lookup_note(&issuer, &recipient).unwrap();
        let after = manager
            .record_redemption_progress(&issuer, &recipient, 40)
            .unwrap();
        assert_eq!(after.timestamp, before.timestamp);
        assert_eq!(after.signature, before.signature);
        assert_eq!(after.amount_collected, before.amount_collected);
        assert_eq!(after.amount_redeemed, 40);
        after.verify_signature(&issuer).unwrap();

        assert!(manager
            .record_redemption_progress(&issuer, &recipient, 61)
            .is_err());
        assert_eq!(
            manager
                .lookup_note(&issuer, &recipient)
                .unwrap()
                .amount_redeemed,
            40
        );
    }

    #[test]
    fn settlement_progress_quarantines_on_snapshot_avl_divergence() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        manager.avl_state = basis_trees::BasisAvlTree::new().unwrap();

        assert!(matches!(
            manager.record_redemption_progress(&issuer, &recipient, 1),
            Err(crate::NoteError::StorageError(message))
                if message.contains("live root")
        ));
        assert!(matches!(
            manager.lookup_note(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn settlement_progress_quarantines_on_tampered_snapshot() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        manager.storage.tamper_first_total_debt_for_test().unwrap();

        assert!(matches!(
            manager.record_redemption_progress(&issuer, &recipient, 1),
            Err(crate::NoteError::StorageError(message))
                if message.contains("checksum")
        ));
        assert!(matches!(
            manager.lookup_note(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn projected_issuer_gross_debt_reads_authoritative_snapshot() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 60, 1))
            .unwrap();
        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, Some(&recipient_c), 50)
                .unwrap(),
            110
        );
    }

    #[test]
    fn projected_issuer_gross_debt_rejects_missing_authoritative_state() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 60, 1))
            .unwrap();
        manager.storage.remove_state_for_test().unwrap();

        assert!(manager
            .projected_issuer_gross_debt(&issuer, Some(&recipient_c), 50)
            .is_err());
    }

    #[test]
    fn projected_issuer_gross_debt_rejects_corrupt_authoritative_state() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 60, 1))
            .unwrap();
        manager.storage.corrupt_state_for_test().unwrap();

        assert!(manager
            .projected_issuer_gross_debt(&issuer, Some(&recipient_c), 50)
            .is_err());
    }

    #[test]
    fn projected_issuer_gross_debt_without_recipient_is_conservative() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 60, 1))
            .unwrap();

        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, None, 50)
                .unwrap(),
            110
        );
    }

    #[test]
    fn projected_issuer_gross_debt_rejects_overflow() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient_b, u64::MAX, 1),
            )
            .unwrap();

        assert!(matches!(
            manager.projected_issuer_gross_debt(&issuer, Some(&recipient_c), 1),
            Err(crate::NoteError::AmountOverflow)
        ));
    }

    #[test]
    fn projected_issuer_gross_debt_survives_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        {
            let mut manager = TrackerStateManager::new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            );
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 60, 1))
                .unwrap();
        }

        let manager =
            TrackerStateManager::new(temp_dir.path(), generation(FreshGenerationApproval::Deny));
        assert_eq!(
            manager
                .projected_issuer_gross_debt(&issuer, Some(&recipient_c), 50)
                .unwrap(),
            110
        );
    }

    #[test]
    fn avl_root_survives_restart_when_arrival_order_differs_from_timestamps() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        let root_before_restart = {
            let mut manager = TrackerStateManager::new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            );
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 60, 2))
                .unwrap();
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient_c, 50, 1))
                .unwrap();
            manager.get_state().avl_root_digest
        };

        let manager =
            TrackerStateManager::new(temp_dir.path(), generation(FreshGenerationApproval::Deny));
        assert_eq!(manager.get_state().avl_root_digest, root_before_restart);
    }

    #[test]
    fn signed_note_successor_preserves_redeemed_progress() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        let initial = create_note(&issuer_secret, &recipient, 100, 1);
        manager.add_note(&issuer, &initial).unwrap();
        manager
            .record_redemption_progress(&issuer, &recipient, 40)
            .unwrap();

        let successor = create_note(&issuer_secret, &recipient, 120, 2);
        manager.add_note(&issuer, &successor).unwrap();

        let stored = manager.lookup_note(&issuer, &recipient).unwrap();
        assert_eq!(stored.amount_collected, 120);
        assert_eq!(stored.amount_redeemed, 40);
        assert_eq!(stored.outstanding_debt(), 80);
    }

    #[test]
    fn initial_note_cannot_inject_redeemed_progress() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        let mut note = create_note(&issuer_secret, &recipient, 100, 1);
        note.amount_redeemed = 99;
        manager.add_note(&issuer, &note).unwrap();

        let stored = manager.lookup_note(&issuer, &recipient).unwrap();
        assert_eq!(stored.amount_redeemed, 0);
        assert_eq!(stored.outstanding_debt(), 100);
    }

    #[test]
    fn missing_authoritative_state_fails_closed_without_mutating_live_tree() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let root_before = manager.get_state().avl_root_digest;
        manager.storage.remove_state_for_test().unwrap();

        assert!(manager.rebuild_avl_tree().is_err());
        assert_eq!(manager.avl_state.root_digest(), root_before);
        assert!(matches!(
            manager.get_total_debt(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn reordered_snapshot_fails_root_validation_without_mutating_live_tree() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 100, 1))
            .unwrap();
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_c, 50, 2))
            .unwrap();
        let root_before = manager.get_state().avl_root_digest;
        manager.storage.reverse_note_order_for_test().unwrap();

        assert!(matches!(
            manager.rebuild_avl_tree(),
            Err(crate::NoteError::StorageError(message))
                if message.contains("does not reproduce")
        ));
        assert_eq!(manager.avl_state.root_digest(), root_before);
        assert!(matches!(
            manager.get_total_debt(&issuer, &recipient_b),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn unexpected_partition_row_rejects_new_note_before_state_change() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 100, 1))
            .unwrap();
        let root_before = manager.get_state().avl_root_digest;
        manager
            .storage
            .insert_unexpected_note_row_for_test()
            .unwrap();

        assert!(manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_c, 50, 2))
            .is_err());
        assert_eq!(manager.avl_state.root_digest(), root_before);
        assert!(matches!(
            manager.get_total_debt(&issuer, &recipient_b),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn repeated_updates_keep_bounded_first_insertion_order() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        let root_before_restart = {
            let mut manager = TrackerStateManager::new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            );
            for version in 1..=25u64 {
                manager
                    .add_note(
                        &issuer,
                        &create_note(&issuer_secret, &recipient, 100 + version, version),
                    )
                    .unwrap();
            }
            assert_eq!(manager.storage.note_row_count_for_test().unwrap(), 1);
            manager.get_state().avl_root_digest
        };

        let manager =
            TrackerStateManager::new(temp_dir.path(), generation(FreshGenerationApproval::Deny));
        assert_eq!(manager.storage.note_row_count_for_test().unwrap(), 1);
        assert_eq!(manager.get_state().avl_root_digest, root_before_restart);
        assert_eq!(manager.get_total_debt(&issuer, &recipient).unwrap(), 125);
    }

    #[test]
    fn malformed_or_extra_authoritative_rows_fail_rebuild_without_clobbering_live_tree() {
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        let mut malformed = make_manager();
        malformed
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let root_before = malformed.get_state().avl_root_digest;
        malformed.storage.corrupt_state_for_test().unwrap();
        assert!(malformed.rebuild_avl_tree().is_err());
        assert_eq!(malformed.avl_state.root_digest(), root_before);
        assert!(matches!(
            malformed.lookup_note(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));

        let mut extra = make_manager();
        extra
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let root_before = extra.get_state().avl_root_digest;
        extra.storage.insert_unexpected_note_row_for_test().unwrap();
        assert!(extra.rebuild_avl_tree().is_err());
        assert_eq!(extra.avl_state.root_digest(), root_before);
        assert!(matches!(
            extra.lookup_note(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn same_length_debt_tampering_fails_snapshot_integrity_and_quarantines() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let root_before = manager.get_state().avl_root_digest;
        manager.storage.tamper_first_total_debt_for_test().unwrap();

        assert!(matches!(
            manager.rebuild_avl_tree(),
            Err(crate::NoteError::StorageError(message))
                if message.contains("checksum")
        ));
        assert_eq!(manager.avl_state.root_digest(), root_before);
        assert!(matches!(
            manager.lookup_note(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn liability_projection_revalidates_signatures_and_quarantines_tampering() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        manager.storage.tamper_first_total_debt_for_test().unwrap();

        assert!(matches!(
            manager.projected_issuer_gross_debt(&issuer, Some(&recipient), 100),
            Err(crate::NoteError::StorageError(message))
                if message.contains("checksum")
        ));
        assert!(matches!(
            manager.get_all_notes(),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn tampered_snapshot_cannot_be_laundered_by_a_valid_successor() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 101, 1))
            .unwrap();
        manager
            .storage
            .rewrite_first_total_debt_with_valid_checksum_for_test(100)
            .unwrap();

        assert!(matches!(
            manager.add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 2)),
            Err(crate::NoteError::InvalidSignature)
        ));
        assert!(!manager.is_healthy());
        assert!(matches!(
            manager.lookup_note(&issuer, &recipient),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn redeemed_progress_tampering_is_detected_even_when_in_range() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        manager
            .record_redemption_progress(&issuer, &recipient, 40)
            .unwrap();
        manager
            .storage
            .tamper_first_redeemed_amount_for_test()
            .unwrap();

        assert!(matches!(
            manager.add_note(&issuer, &create_note(&issuer_secret, &recipient, 120, 2)),
            Err(crate::NoteError::StorageError(message))
                if message.contains("checksum")
        ));
        assert!(!manager.is_healthy());
    }

    #[test]
    fn in_range_redeemed_tampering_is_rejected_after_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        {
            let mut manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
                .unwrap();
            manager
                .record_redemption_progress(&issuer, &recipient, 40)
                .unwrap();
            manager
                .storage
                .tamper_first_redeemed_amount_for_test()
                .unwrap();
        }

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Deny)
            ),
            Err(crate::NoteError::StorageError(message))
                if message.contains("snapshot checksum")
        ));
    }

    #[test]
    fn capacity_rejection_does_not_quarantine_or_change_the_root() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 100, 1))
            .unwrap();
        let root_before = manager.get_state().avl_root_digest;
        manager.storage.set_capacity_limit_for_test(1);

        assert!(matches!(
            manager.add_note(&issuer, &create_note(&issuer_secret, &recipient_c, 50, 2)),
            Err(crate::NoteError::CapacityExceeded { limit: 1 })
        ));
        assert!(manager.is_healthy());
        assert_eq!(manager.get_state().avl_root_digest, root_before);
        assert_eq!(manager.get_total_debt(&issuer, &recipient_b).unwrap(), 100);
    }

    #[test]
    fn generation_requires_explicit_bootstrap_and_binds_nft_and_first_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Deny)
            ),
            Err(crate::NoteError::GenerationBindingRequired(_))
        ));

        let manager = TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Approve),
        )
        .unwrap();
        let empty_root = manager.get_state().avl_root_digest;
        let (nft, bootstrap_root, anchor_root) = manager.storage.generation_for_test().unwrap();
        assert_eq!(nft, [0x42; 32]);
        assert_eq!(bootstrap_root, empty_root);
        assert_eq!(anchor_root, None);
        manager
            .validate_observed_generation(&[0x42; 32], empty_root)
            .unwrap();
        assert_eq!(
            manager.storage.generation_for_test().unwrap().2,
            Some(empty_root)
        );
        drop(manager);

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                TrackerGenerationConfig {
                    tracker_nft_id: [0x43; 32],
                    fresh_generation: FreshGenerationApproval::Deny,
                }
            ),
            Err(crate::NoteError::GenerationMismatch(_))
        ));
    }

    fn raw_note_layout(
        path: &std::path::Path,
    ) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>) {
        let keyspace = fjall::Config::new(path).open().unwrap();
        let notes = keyspace
            .open_partition("iou_notes", fjall::PartitionCreateOptions::default())
            .unwrap();
        let schema = keyspace
            .open_partition("note_schema", fjall::PartitionCreateOptions::default())
            .unwrap();
        (
            notes
                .get(b"note_state_v2")
                .unwrap()
                .map(|value| value.to_vec()),
            schema
                .get(b"note_schema_v2")
                .unwrap()
                .map(|value| value.to_vec()),
            schema
                .get(b"tracker_generation_v1")
                .unwrap()
                .map(|value| value.to_vec()),
        )
    }

    fn raw_pending_publication(path: &std::path::Path) -> Option<Vec<u8>> {
        let keyspace = fjall::Config::new(path).open().unwrap();
        let schema = keyspace
            .open_partition("note_schema", fjall::PartitionCreateOptions::default())
            .unwrap();
        schema
            .get(b"pending_publication_v1")
            .unwrap()
            .map(|value| value.to_vec())
    }

    fn replace_raw_pending_publication(path: &std::path::Path, bytes: &[u8]) {
        let keyspace = fjall::Config::new(path).open().unwrap();
        let schema = keyspace
            .open_partition("note_schema", fjall::PartitionCreateOptions::default())
            .unwrap();
        schema.insert(b"pending_publication_v1", bytes).unwrap();
        keyspace.persist(fjall::PersistMode::SyncData).unwrap();
    }

    #[test]
    fn corrupted_pending_publication_is_rejected_without_rewriting_state() {
        let mutation_names = [
            "magic",
            "digest",
            "transaction id",
            "height",
            "checksum",
            "truncation",
        ];

        for (case, mutation_name) in mutation_names.into_iter().enumerate() {
            let temp_dir = tempfile::tempdir().unwrap();
            {
                let mut manager = TrackerStateManager::try_new(
                    temp_dir.path(),
                    generation(FreshGenerationApproval::Approve),
                )
                .unwrap();
                let issuer_secret = [1u8; 32];
                let issuer = issuer_pubkey(&issuer_secret);
                let recipient = [2u8; 33];
                manager
                    .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
                    .unwrap();
                let digest = manager.validated_state().unwrap().avl_root_digest;
                manager
                    .mark_notes_pending(digest, &"11".repeat(32), 100)
                    .unwrap();
            }

            let storage_path = temp_dir.path().join("notes");
            let mut corrupted = raw_pending_publication(&storage_path).unwrap();
            assert_eq!(corrupted.len(), 109);
            match case {
                0 => corrupted[0] ^= 1,
                1 => corrupted[4] ^= 1,
                2 => corrupted[37] ^= 1,
                3 => corrupted[69] ^= 1,
                4 => corrupted[108] ^= 1,
                5 => {
                    corrupted.pop();
                }
                _ => unreachable!(),
            }
            replace_raw_pending_publication(&storage_path, &corrupted);
            let before_layout = raw_note_layout(&storage_path);
            let before_pending = raw_pending_publication(&storage_path);

            assert!(
                matches!(
                    TrackerStateManager::try_new(
                        temp_dir.path(),
                        generation(FreshGenerationApproval::Deny)
                    ),
                    Err(crate::NoteError::StorageError(_))
                ),
                "{mutation_name} corruption must fail closed"
            );
            assert_eq!(
                raw_note_layout(&storage_path),
                before_layout,
                "{mutation_name} corruption must not rewrite the state snapshot"
            );
            assert_eq!(
                raw_pending_publication(&storage_path),
                before_pending,
                "{mutation_name} corruption must not rewrite the publication receipt"
            );
        }
    }

    #[test]
    fn orphan_pending_publication_cannot_initialize_a_fresh_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let mut manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            let issuer_secret = [1u8; 32];
            let issuer = issuer_pubkey(&issuer_secret);
            let recipient = [2u8; 33];
            manager
                .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
                .unwrap();
            let digest = manager.validated_state().unwrap().avl_root_digest;
            manager
                .mark_notes_pending(digest, &"11".repeat(32), 100)
                .unwrap();
            manager.storage.remove_state_for_test().unwrap();
            manager.storage.remove_schema_for_test().unwrap();
            manager.storage.remove_generation_for_test().unwrap();
            manager.storage.persist_for_test().unwrap();
        }
        let storage_path = temp_dir.path().join("notes");
        let before_layout = raw_note_layout(&storage_path);
        let before_pending = raw_pending_publication(&storage_path);
        assert!(
            before_layout.0.is_none()
                && before_layout.1.is_none()
                && before_layout.2.is_none()
                && before_pending.is_some()
        );

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve)
            ),
            Err(crate::NoteError::GenerationMismatch(message))
                if message.contains("Pending tracker publication")
        ));
        assert_eq!(raw_note_layout(&storage_path), before_layout);
        assert_eq!(raw_pending_publication(&storage_path), before_pending);
    }

    #[test]
    fn orphan_generation_manifest_is_not_reset_or_completed() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            manager.storage.remove_state_for_test().unwrap();
            manager.storage.remove_schema_for_test().unwrap();
            manager.storage.persist_for_test().unwrap();
        }
        let storage_path = temp_dir.path().join("notes");
        let before = raw_note_layout(&storage_path);
        assert!(before.0.is_none() && before.1.is_none() && before.2.is_some());

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve)
            ),
            Err(crate::NoteError::GenerationMismatch(message))
                if message.contains("complete authoritative")
        ));
        assert_eq!(raw_note_layout(&storage_path), before);
    }

    #[test]
    fn state_without_schema_or_generation_is_not_completed() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            manager.storage.remove_schema_for_test().unwrap();
            manager.storage.remove_generation_for_test().unwrap();
            manager.storage.persist_for_test().unwrap();
        }
        let storage_path = temp_dir.path().join("notes");
        let before = raw_note_layout(&storage_path);
        assert!(before.0.is_some() && before.1.is_none() && before.2.is_none());

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve)
            ),
            Err(crate::NoteError::StorageError(message))
                if message.contains("without its schema and generation")
        ));
        assert_eq!(raw_note_layout(&storage_path), before);
    }

    #[test]
    fn wrong_generation_open_does_not_rewrite_any_authoritative_record() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let _manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
        }
        let storage_path = temp_dir.path().join("notes");
        let before = raw_note_layout(&storage_path);

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                TrackerGenerationConfig {
                    tracker_nft_id: [0x43; 32],
                    fresh_generation: FreshGenerationApproval::Approve,
                }
            ),
            Err(crate::NoteError::GenerationMismatch(_))
        ));
        assert_eq!(raw_note_layout(&storage_path), before);
    }

    #[test]
    fn first_observed_nonbootstrap_root_quarantines_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Approve),
        )
        .unwrap();
        let mut wrong_root = manager.get_state().avl_root_digest;
        wrong_root[0] ^= 1;

        assert!(matches!(
            manager.validate_observed_generation(&[0x42; 32], wrong_root),
            Err(crate::NoteError::GenerationMismatch(_))
        ));
        assert!(!manager.is_healthy());
    }

    #[test]
    fn corrupted_generation_manifest_is_rejected_after_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let manager = TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Approve),
            )
            .unwrap();
            manager
                .storage
                .tamper_generation_manifest_for_test()
                .unwrap();
        }

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Deny)
            ),
            Err(crate::NoteError::StorageError(message))
                if message.contains("generation manifest checksum")
        ));
    }

    #[test]
    fn publication_generation_gate_revalidates_the_complete_snapshot() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let bootstrap_root = manager.storage.generation_for_test().unwrap().1;
        manager.storage.tamper_first_total_debt_for_test().unwrap();

        assert!(matches!(
            manager.validate_observed_generation(&[0x42; 32], bootstrap_root),
            Err(crate::NoteError::StorageError(message)) if message.contains("snapshot checksum")
        ));
        assert!(!manager.is_healthy());
    }

    #[test]
    fn proof_exposure_revalidates_the_complete_snapshot() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        manager.storage.tamper_first_total_debt_for_test().unwrap();

        assert!(matches!(
            manager.generate_proof(&issuer, &recipient),
            Err(crate::NoteError::StorageError(message)) if message.contains("snapshot checksum")
        ));
        assert!(!manager.is_healthy());
    }

    #[test]
    fn reserve_root_exposure_revalidates_the_complete_snapshot() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        manager.storage.tamper_first_total_debt_for_test().unwrap();

        assert!(matches!(
            manager.reserve_state_digest(),
            Err(crate::NoteError::StorageError(message)) if message.contains("snapshot checksum")
        ));
        assert!(!manager.is_healthy());
    }

    #[test]
    fn declared_note_count_above_bound_fails_closed() {
        let mut manager = make_manager();
        manager
            .storage
            .set_declared_note_count_for_test(50_001)
            .unwrap();

        assert!(matches!(
            manager.rebuild_avl_tree(),
            Err(crate::NoteError::StorageError(message))
                if message.contains("exceeds configured bound")
        ));
        assert!(matches!(
            manager.get_all_notes(),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn unknown_durable_outcome_quarantines_manager_until_validated_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient_b = [2u8; 33];
        let recipient_c = [3u8; 33];

        let publication_health = crate::PublicationHealth::new();
        let mut manager = TrackerStateManager::try_new_with_publication_health(
            temp_dir.path(),
            generation(FreshGenerationApproval::Approve),
            publication_health.clone(),
        )
        .unwrap();
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient_b, 100, 1))
            .unwrap();
        manager.storage.fail_next_persist_for_test();

        assert!(matches!(
            manager.add_note(&issuer, &create_note(&issuer_secret, &recipient_c, 50, 2)),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
        assert!(matches!(
            manager.lookup_note(&issuer, &recipient_b),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
        assert!(!publication_health.is_healthy());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { manager.get_state() }))
                .is_err()
        );

        drop(manager);

        let reopened = TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Deny),
        )
        .unwrap();
        let persisted = reopened.storage.read_state_strict().unwrap();
        assert_eq!(
            reopened.get_state().avl_root_digest,
            persisted.avl_root_digest
        );
        assert_eq!(reopened.get_total_debt(&issuer, &recipient_b).unwrap(), 100);
    }

    #[test]
    fn confirmation_durability_uncertainty_blocks_publication() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 100, 1))
            .unwrap();
        let digest = manager.validated_state().unwrap().avl_root_digest;
        manager.storage.fail_next_persist_for_test();

        assert!(matches!(
            manager.reconcile_with_confirmed_digest(&digest, "box-1", 100),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
        assert!(!manager.is_healthy());
        assert!(matches!(
            manager.validated_state(),
            Err(crate::NoteError::StorageOutcomeUnknown(_))
        ));
    }

    #[test]
    fn storage_allows_only_one_writer_per_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let first = TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Approve),
        )
        .unwrap();
        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Deny)
            ),
            Err(crate::NoteError::StorageError(message))
                if message.contains("active writer")
        ));
        drop(first);
        assert!(TrackerStateManager::try_new(
            temp_dir.path(),
            generation(FreshGenerationApproval::Deny)
        )
        .is_ok());
    }

    #[test]
    fn legacy_note_rows_require_explicit_migration_without_rewrite() {
        let temp_dir = tempfile::tempdir().unwrap();
        let notes_path = temp_dir.path().join("notes");
        let legacy_key = [7u8; 32];
        let legacy_value = [9u8; 155];

        {
            let keyspace = fjall::Config::new(&notes_path).open().unwrap();
            let partition = keyspace
                .open_partition("iou_notes", fjall::PartitionCreateOptions::default())
                .unwrap();
            partition.insert(legacy_key, legacy_value).unwrap();
            keyspace.persist(fjall::PersistMode::SyncData).unwrap();
        }

        assert!(matches!(
            TrackerStateManager::try_new(
                temp_dir.path(),
                generation(FreshGenerationApproval::Deny)
            ),
            Err(crate::NoteError::MigrationRequired(message))
                if message.contains("explicit")
        ));

        let keyspace = fjall::Config::new(&notes_path).open().unwrap();
        let partition = keyspace
            .open_partition("iou_notes", fjall::PartitionCreateOptions::default())
            .unwrap();
        assert_eq!(
            partition.get(legacy_key).unwrap().unwrap().as_ref(),
            legacy_value
        );
        assert_eq!(partition.len().unwrap(), 1);
    }
}
