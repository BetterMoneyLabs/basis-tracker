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

    let note = IouNote::new(
        recipient_pubkey,
        1000,
        0,
        1234567890,
        signature,
        [0x42; 32],
        crate::DenomKey::Erg,
    );

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
    let note = IouNote::new(
        receiver_pubkey,
        1000,
        0,
        timestamp,
        [3u8; 65],
        [0x42; 32],
        crate::DenomKey::Erg,
    );

    // Format: key (32) || totalDebt (8) || timestamp (8) || trackerNftId (32) = 80 bytes
    let message = note.signing_message(&owner_pubkey);
    if message.is_empty() {
        return Err("signing message is empty".to_string());
    }

    if message.len() != 80 {
        return Err(format!(
            "signing message should be 80 bytes, got {}",
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
    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount,
        timestamp,
        &[1u8; 32],
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .expect("Failed to create valid signature");

    // Test valid signature (basic format validation)
    if note.verify_signature(&issuer_pubkey).is_err() {
        return Err("should pass with valid signature format".to_string());
    }

    // Test invalid signature (all zeros)
    let invalid_note = IouNote::new(
        recipient_pubkey,
        amount,
        0,
        timestamp,
        [0u8; 65],
        [0x42; 32],
        crate::DenomKey::Erg,
    );

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
        &[0x42; 32],
        crate::DenomKey::Erg,
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
    let mut note = IouNote::create_and_sign(
        [2u8; 33],
        1000,
        1234567890,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;

    // Test 1: Tamper with signature
    note.signature[0] ^= 0x01; // Flip a bit in the signature
    if note.verify_signature(&issuer_pubkey).is_ok() {
        return Err("Tampered signature should fail verification".to_string());
    }

    // Test 2: Tamper with recipient
    let mut note2 = IouNote::create_and_sign(
        [2u8; 33],
        1000,
        1234567890,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    note2.recipient_pubkey[0] ^= 0x01;
    if note2.verify_signature(&issuer_pubkey).is_ok() {
        return Err("Tampered recipient should fail verification".to_string());
    }

    // Test 3: Tamper with amount
    let mut note3 = IouNote::create_and_sign(
        [2u8; 33],
        1000,
        1234567890,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    note3.amount_collected = 2000;
    if note3.verify_signature(&issuer_pubkey).is_ok() {
        return Err("Tampered amount should fail verification".to_string());
    }

    // Test 4: Wrong issuer public key
    let wrong_pubkey = [0u8; 33];
    let note4 = IouNote::create_and_sign(
        [2u8; 33],
        1000,
        1234567890,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
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
    let note1 = IouNote::create_and_sign(
        [2u8; 33],
        1000,
        1234567890,
        &secret_key1.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create and sign note: {:?}", e))?;

    let note2 = IouNote::create_and_sign(
        [2u8; 33],
        2000,
        1234567891,
        &secret_key2.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
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
        [0x42; 32],
        crate::DenomKey::Erg,
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
    let note1 = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1000000,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create first note: {:?}", e))?;

    let result1 = tracker.add_note(&issuer_pubkey_bytes, &note1);
    if result1.is_err() {
        return Err(format!("First note should succeed: {:?}", result1.err()));
    }

    // Create second signed note with higher timestamp
    let note2 = IouNote::create_and_sign(
        recipient_pubkey,
        2000,
        1000001,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
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
    let note1 = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1000000,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create first note: {:?}", e))?;

    let result1 = tracker.add_note(&issuer_pubkey_bytes, &note1);
    if result1.is_err() {
        return Err(format!("First note should succeed: {:?}", result1.err()));
    }

    // Try to add note with same timestamp - should fail
    let note2 = IouNote::create_and_sign(
        recipient_pubkey,
        2000,
        1000000,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
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
    let note3 = IouNote::create_and_sign(
        recipient_pubkey,
        2000,
        999999,
        &secret_key.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
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
    let note1 = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1000000,
        &secret_key1.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
    .map_err(|e| format!("Failed to create first note: {:?}", e))?;

    let result1 = tracker.add_note(&issuer1_pubkey, &note1);
    if result1.is_err() {
        return Err(format!("First note should succeed: {:?}", result1.err()));
    }

    // Add note for different issuer with same timestamp - should succeed
    let note2 = IouNote::create_and_sign(
        recipient_pubkey,
        1000,
        1000000,
        &secret_key2.secret_bytes(),
        &[0x42; 32],
        crate::DenomKey::Erg,
    )
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
    use crate::{IouNote, NoteConfirmationStatus, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    fn make_manager() -> TrackerStateManager {
        TrackerStateManager::new_with_temp_storage()
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
        IouNote::create_and_sign(
            *recipient,
            amount,
            timestamp,
            issuer_secret,
            &[0x42; 32],
            crate::DenomKey::Erg,
        )
        .unwrap()
    }

    #[test]
    fn fresh_note_is_local_only() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
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
        let count = manager.mark_notes_pending(digest, "tx123", 100).unwrap();
        assert_eq!(count, 1);

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Pending);
        assert_eq!(confirmation.pending_total_debt, Some(1000));
        assert_eq!(confirmation.pending_tx_id, Some("tx123".to_string()));
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
        manager.mark_notes_pending(digest, "tx123", 100).unwrap();
        manager
            .confirm_pending_notes(digest, "box123", 200)
            .unwrap();

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::Confirmed);
        assert_eq!(confirmation.confirmed_total_debt, Some(1000));
        assert_eq!(confirmation.confirmed_box_id, Some("box123".to_string()));
        assert_eq!(confirmation.confirmed_height, Some(200));
        assert_eq!(confirmation.pending_total_debt, None);
        assert_eq!(confirmation.pending_tx_id, None);
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
        manager.mark_notes_pending(digest, "tx123", 100).unwrap();
        manager.revert_pending_notes().unwrap();

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
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

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
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

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::LocalOnly);
    }

    #[test]
    fn rebuild_confirmations_clears_pending_state() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx123", 100).unwrap();

        manager.rebuild_confirmations();

        let confirmation = manager
            .get_confirmation(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(confirmation.status, NoteConfirmationStatus::LocalOnly);
        assert_eq!(confirmation.pending_total_debt, None);
        assert_eq!(confirmation.pending_tx_id, None);
    }

    #[test]
    fn note_confirmation_redeemable_amount() {
        let confirmation = crate::NoteConfirmation {
            status: NoteConfirmationStatus::Confirmed,
            confirmed_total_debt: Some(1000),
            pending_total_debt: None,
            confirmed_box_id: None,
            confirmed_height: None,
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
    fn committed_proof_rejects_unconfirmed_note() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];
        let note = create_note(&issuer_secret, &recipient, 1000, 1);

        manager.add_note(&issuer, &note).unwrap();

        let result = manager.generate_committed_tracker_lookup_proof(
            &issuer,
            &recipient,
            &crate::DenomKey::Erg,
        );
        assert!(
            matches!(result, Err(crate::NoteError::NoteNotConfirmed)),
            "expected NoteNotConfirmed, got {:?}",
            result
        );
    }

    /// Issue #15 S6: redemption proofs must be generated against the committed
    /// on-chain state. A note posted after the last commitment changes the live
    /// tree digest but must not affect committed proofs for already-confirmed
    /// notes, and must itself be rejected until confirmed.
    #[test]
    fn committed_proof_unaffected_by_later_unconfirmed_notes() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient1 = [2u8; 33];
        let recipient2 = [3u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient1, 1000, 1))
            .unwrap();
        let digest1 = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest1, "tx1", 100).unwrap();
        manager.confirm_pending_notes(digest1, "box1", 200).unwrap();
        assert_eq!(manager.confirmed_digest(), Some(digest1));

        // Committed proof for the confirmed note works and carries the confirmed value.
        let proof1 = manager
            .generate_committed_tracker_lookup_proof(&issuer, &recipient1, &crate::DenomKey::Erg)
            .expect("committed proof for confirmed note");
        assert_eq!(proof1.value, 1000u64.to_be_bytes().to_vec());

        // A second note (possibly a self-note from an attacker) changes the live tree.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient2, 5000, 2))
            .unwrap();
        assert_ne!(manager.get_state().avl_root_digest, digest1);

        // The committed proof for the first note still verifies against the
        // confirmed digest — redemption liveness is preserved.
        let proof1_after = manager
            .generate_committed_tracker_lookup_proof(&issuer, &recipient1, &crate::DenomKey::Erg)
            .expect("committed proof must survive later unconfirmed notes");
        assert_eq!(proof1_after.value, 1000u64.to_be_bytes().to_vec());
        assert!(
            basis_trees::BasisAvlTree::verify_proof(
                &digest1,
                &proof1_after.proof,
                &proof1_after.key,
                &proof1_after.value,
            ),
            "committed proof must verify against the confirmed on-chain digest"
        );

        // The unconfirmed note itself cannot be redeemed yet.
        let result = manager.generate_committed_tracker_lookup_proof(
            &issuer,
            &recipient2,
            &crate::DenomKey::Erg,
        );
        assert!(
            matches!(result, Err(crate::NoteError::NoteNotConfirmed)),
            "expected NoteNotConfirmed, got {:?}",
            result
        );
    }

    #[test]
    fn committed_proof_errors_when_rebuilt_digest_mismatches() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 100).unwrap();
        manager.confirm_pending_notes(digest, "box1", 200).unwrap();

        // Reconcile against a digest that does not match the committed tree. The
        // note keeps its confirmed value but the recorded confirmed digest moves,
        // so the rebuilt committed tree no longer reproduces it.
        let other_digest = [9u8; 33];
        manager
            .reconcile_with_confirmed_digest(&other_digest, "box2", 300)
            .unwrap();

        let result = manager.generate_committed_tracker_lookup_proof(
            &issuer,
            &recipient,
            &crate::DenomKey::Erg,
        );
        match result {
            Err(crate::NoteError::StorageError(msg)) => {
                assert!(
                    msg.contains("digest mismatch"),
                    "expected digest mismatch error, got: {}",
                    msg
                );
            }
            other => panic!("expected digest mismatch StorageError, got {:?}", other),
        }
    }
}

#[cfg(test)]
mod debt_monotonicity_tests {
    use crate::{IouNote, NoteError, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    fn make_manager() -> TrackerStateManager {
        TrackerStateManager::new_with_temp_storage()
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
        IouNote::create_and_sign(
            *recipient,
            amount,
            timestamp,
            issuer_secret,
            &[0x42; 32],
            crate::DenomKey::Erg,
        )
        .unwrap()
    }

    #[test]
    fn debt_decrease_rejected_on_add_note() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();

        // A debtor restating a lower cumulative total must be rejected.
        let lower = create_note(&issuer_secret, &recipient, 500, 2);
        let result = manager.add_note(&issuer, &lower);
        assert!(
            matches!(result, Err(NoteError::DebtDecreaseNotPermitted { .. })),
            "expected DebtDecreaseNotPermitted, got {:?}",
            result
        );

        // The stored note keeps the original (higher) total.
        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 1000);
    }

    #[test]
    fn debt_decrease_rejected_on_update_note() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();

        let lower = create_note(&issuer_secret, &recipient, 0, 2);
        let result = manager.update_note(&issuer, &lower);
        assert!(
            matches!(result, Err(NoteError::DebtDecreaseNotPermitted { .. })),
            "expected DebtDecreaseNotPermitted, got {:?}",
            result
        );

        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 1000);
    }

    #[test]
    fn equal_or_higher_debt_accepted() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();

        // Equal total (e.g. a re-signed note) is accepted.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 2))
            .unwrap();
        // Higher total is accepted.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1500, 3))
            .unwrap();
        manager
            .update_note(&issuer, &create_note(&issuer_secret, &recipient, 2000, 4))
            .unwrap();

        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 2000);
    }

    #[test]
    fn amount_redeemed_preserved_across_replacement() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        let mut note = create_note(&issuer_secret, &recipient, 1000, 1);
        note.amount_redeemed = 400;
        manager.add_note(&issuer, &note).unwrap();

        // A replacement note from create_and_sign carries amount_redeemed = 0;
        // the tracked redemption progress must survive the update.
        let replacement = create_note(&issuer_secret, &recipient, 1500, 2);
        assert_eq!(replacement.amount_redeemed, 0);
        manager.add_note(&issuer, &replacement).unwrap();

        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 1500);
        assert_eq!(stored.amount_redeemed, 400);

        // A later redemption (higher amount_redeemed) is recorded via record_redemption;
        // update_note always carries the existing amount_redeemed forward.
        manager
            .record_redemption(&issuer, &recipient, &crate::DenomKey::Erg, 600)
            .unwrap();

        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 1500);
        assert_eq!(stored.amount_redeemed, 600);
    }
}

#[cfg(test)]
mod contract_range_tests {
    use crate::{IouNote, NoteError, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    fn make_manager() -> TrackerStateManager {
        TrackerStateManager::new_with_temp_storage()
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
        IouNote::create_and_sign(
            *recipient,
            amount,
            timestamp,
            issuer_secret,
            &[0x42; 32],
            crate::DenomKey::Erg,
        )
        .unwrap()
    }

    #[test]
    fn debt_above_contract_range_rejected_on_add_note() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        // The contract reads totalDebt with the signed byteArrayToLong, so
        // amount_collected >= 2^63 would come out negative on-chain.
        for amount in [u64::MAX, i64::MAX as u64 + 1] {
            let note = create_note(&issuer_secret, &recipient, amount, 1);
            let result = manager.add_note(&issuer, &note);
            assert!(
                matches!(
                    result,
                    Err(NoteError::AmountExceedsContractRange {
                        field: "amount_collected",
                        value,
                    }) if value == amount
                ),
                "expected AmountExceedsContractRange for amount {}, got {:?}",
                amount,
                result
            );
        }

        // The rejected postings must not poison state: a later sane note is accepted.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 2))
            .unwrap();
        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 1000);
    }

    #[test]
    fn debt_above_contract_range_rejected_on_update_note() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();

        for amount in [u64::MAX, i64::MAX as u64 + 1] {
            let note = create_note(&issuer_secret, &recipient, amount, 2);
            let result = manager.update_note(&issuer, &note);
            assert!(
                matches!(
                    result,
                    Err(NoteError::AmountExceedsContractRange {
                        field: "amount_collected",
                        value,
                    }) if value == amount
                ),
                "expected AmountExceedsContractRange for amount {}, got {:?}",
                amount,
                result
            );
        }

        // A later sane note is still accepted.
        manager
            .update_note(&issuer, &create_note(&issuer_secret, &recipient, 2000, 3))
            .unwrap();
        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, 2000);
    }

    #[test]
    fn debt_at_contract_range_max_accepted() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        // i64::MAX exactly still fits the contract's signed long.
        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient, i64::MAX as u64, 1),
            )
            .unwrap();
        let stored = manager
            .lookup_note(&issuer, &recipient, &crate::DenomKey::Erg)
            .unwrap();
        assert_eq!(stored.amount_collected, i64::MAX as u64);
    }

    #[test]
    fn timestamp_above_contract_range_rejected() {
        let mut manager = make_manager();
        let issuer_secret = [1u8; 32];
        let issuer = issuer_pubkey(&issuer_secret);
        let recipient = [2u8; 33];

        // u64::MAX is also a future timestamp, but the contract-range check must
        // fire first: FutureTimestamp alone would not explain the on-chain bricking.
        for timestamp in [u64::MAX, i64::MAX as u64 + 1] {
            let note = create_note(&issuer_secret, &recipient, 1000, timestamp);
            let result = manager.add_note(&issuer, &note);
            assert!(
                matches!(
                    result,
                    Err(NoteError::AmountExceedsContractRange {
                        field: "timestamp",
                        value,
                    }) if value == timestamp
                ),
                "expected AmountExceedsContractRange for timestamp {}, got {:?}",
                timestamp,
                result
            );
            let result = manager.update_note(&issuer, &note);
            assert!(
                matches!(
                    result,
                    Err(NoteError::AmountExceedsContractRange {
                        field: "timestamp",
                        value,
                    }) if value == timestamp
                ),
                "expected AmountExceedsContractRange for timestamp {}, got {:?}",
                timestamp,
                result
            );
        }

        // A later sane note is accepted.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1))
            .unwrap();
    }
}

/// Multi-denomination acceptance tests for specs/ergo/multi_collateral_reserves.md:
/// independent debt edges per (issuer, recipient, denomination), per-pair reserve
/// trees, and journal-based restart rebuilds reproducing digests exactly.
#[cfg(test)]
mod multi_denomination_tests {
    use crate::{DenomKey, IouNote, NoteError, TrackerStateManager};
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const TOKEN_A: [u8; 32] = [0xa5; 32];
    const TOKEN_B: [u8; 32] = [0xe8; 32];

    fn pubkey(secret_key: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret_key).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn create_note(
        issuer_secret: &[u8; 32],
        recipient: &[u8; 33],
        amount: u64,
        timestamp: u64,
        denomination: DenomKey,
    ) -> IouNote {
        IouNote::create_and_sign(
            *recipient,
            amount,
            timestamp,
            issuer_secret,
            &NFT,
            denomination,
        )
        .unwrap()
    }

    #[test]
    fn same_pair_independent_debt_per_denomination() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer_secret = [1u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&[2u8; 32]);

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
            )
            .unwrap();
        manager
            .add_note(
                &issuer,
                &create_note(
                    &issuer_secret,
                    &recipient,
                    250,
                    1001,
                    DenomKey::Token(TOKEN_A),
                ),
            )
            .unwrap();
        manager
            .add_note(
                &issuer,
                &create_note(
                    &issuer_secret,
                    &recipient,
                    2500,
                    1002,
                    DenomKey::Token(TOKEN_B),
                ),
            )
            .unwrap();

        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            100
        );
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            250
        );
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_B))
                .unwrap(),
            2500
        );

        // The three edges are distinct tree keys.
        let k_erg = crate::NoteKey::from_assets(&issuer, &recipient, &DenomKey::Erg);
        let k_a = crate::NoteKey::from_assets(&issuer, &recipient, &DenomKey::Token(TOKEN_A));
        let k_b = crate::NoteKey::from_assets(&issuer, &recipient, &DenomKey::Token(TOKEN_B));
        assert_ne!(k_erg, k_a);
        assert_ne!(k_a, k_b);

        // A note signed for one denomination is rejected under another: the
        // denomination is part of the signed message's key.
        let erg_note = create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg);
        let mut wrong = erg_note.clone();
        wrong.denomination = DenomKey::Token(TOKEN_A);
        assert!(matches!(
            wrong.verify_signature(&issuer),
            Err(NoteError::InvalidSignature)
        ));
    }

    #[test]
    fn reserve_trees_are_per_issuer_and_denomination() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer = pubkey(&[1u8; 32]);
        let recipient = pubkey(&[2u8; 32]);

        manager
            .update_already_redeemed(&issuer, &recipient, &DenomKey::Erg, 1000, 10)
            .unwrap();
        manager
            .update_already_redeemed(&issuer, &recipient, &DenomKey::Token(TOKEN_A), 1001, 40)
            .unwrap();

        assert_eq!(
            manager
                .get_already_redeemed(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            10
        );
        assert_eq!(
            manager
                .get_already_redeemed(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            40
        );
        // A denomination with no history is the empty tree / zero redeemed.
        assert_eq!(
            manager
                .get_already_redeemed(&issuer, &recipient, &DenomKey::Token(TOKEN_B))
                .unwrap(),
            0
        );
        assert_ne!(
            manager.reserve_state_digest(&issuer, &DenomKey::Erg),
            manager.reserve_state_digest(&issuer, &DenomKey::Token(TOKEN_A)),
        );
    }

    /// §10 exchange: consented cross-denomination debt movement is atomic and
    /// signature-checked; non-consented or malformed exchanges are rejected.
    #[test]
    fn exchange_moves_debt_between_denominations() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer_secret = [1u8; 32];
        let recipient_secret = [3u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&recipient_secret);

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
            )
            .unwrap();
        manager
            .add_note(
                &issuer,
                &create_note(
                    &issuer_secret,
                    &recipient,
                    50,
                    1001,
                    DenomKey::Token(TOKEN_A),
                ),
            )
            .unwrap();

        // Exchange 30 ERG for 40 TOKEN_A at timestamp 2000.
        let sign = |secret: &[u8; 32], denom: &DenomKey, total: u64, ts: u64| {
            let pk = pubkey(secret);
            let msg = crate::schnorr::signing_message(&issuer, &recipient, denom, total, ts, &NFT);
            let _ = pk;
            crate::schnorr::schnorr_sign(&msg, secret, &issuer).unwrap()
        };
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        let ts = 2000u64;
        let msg_from =
            crate::schnorr::signing_message(&issuer, &recipient, &DenomKey::Erg, 70, ts, &NFT);
        let msg_to = crate::schnorr::signing_message(
            &issuer,
            &recipient,
            &DenomKey::Token(TOKEN_A),
            90,
            ts,
            &NFT,
        );
        let _ = sign;
        let issuer_from = sign_as(&issuer_secret, &msg_from, &issuer);
        let recipient_from = sign_as(&recipient_secret, &msg_from, &recipient);
        let issuer_to = sign_as(&issuer_secret, &msg_to, &issuer);
        let recipient_to = sign_as(&recipient_secret, &msg_to, &recipient);

        manager
            .exchange_debt(
                &issuer,
                &recipient,
                &DenomKey::Erg,
                30,
                &DenomKey::Token(TOKEN_A),
                40,
                ts,
                &issuer_from,
                &recipient_from,
                &issuer_to,
                &recipient_to,
            )
            .unwrap();

        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            90
        );
        // The exchanged notes still verify under the new signatures.
        let erg_note = manager
            .lookup_note(&issuer, &recipient, &DenomKey::Erg)
            .unwrap();
        assert!(erg_note.verify_signature(&issuer).is_ok());
        assert_eq!(erg_note.timestamp, ts);

        // Creditor consent is required: a bad recipient signature is rejected and
        // nothing moves.
        let ts2 = 2001u64;
        let msg_from2 =
            crate::schnorr::signing_message(&issuer, &recipient, &DenomKey::Erg, 60, ts2, &NFT);
        let msg_to2 = crate::schnorr::signing_message(
            &issuer,
            &recipient,
            &DenomKey::Token(TOKEN_A),
            100,
            ts2,
            &NFT,
        );
        let bad_recipient_from = sign_as(&issuer_secret, &msg_from2, &issuer); // signed by the wrong party
        let result = manager.exchange_debt(
            &issuer,
            &recipient,
            &DenomKey::Erg,
            10,
            &DenomKey::Token(TOKEN_A),
            10,
            ts2,
            &sign_as(&issuer_secret, &msg_from2, &issuer),
            &bad_recipient_from,
            &sign_as(&issuer_secret, &msg_to2, &issuer),
            &sign_as(&recipient_secret, &msg_to2, &recipient),
        );
        assert!(matches!(result, Err(NoteError::InvalidSignature)));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );

        // Same-denomination exchange is rejected.
        assert!(matches!(
            manager.exchange_debt(
                &issuer,
                &recipient,
                &DenomKey::Erg,
                10,
                &DenomKey::Erg,
                10,
                ts2,
                &issuer_from,
                &recipient_from,
                &issuer_to,
                &recipient_to,
            ),
            Err(NoteError::InvalidExchange(_))
        ));
    }

    /// §7.4 restart-rebuild equality: notes and reserve trees survive a restart
    /// bit-identically (journal replay + note store reload).
    #[test]
    fn restart_rebuild_reproduces_digests() {
        let dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&[2u8; 32]);

        let (root_before, erg_digest_before, tok_digest_before) = {
            let mut manager = TrackerStateManager::new(dir.path());
            manager
                .add_note(
                    &issuer,
                    &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
                )
                .unwrap();
            manager
                .add_note(
                    &issuer,
                    &create_note(
                        &issuer_secret,
                        &recipient,
                        250,
                        1001,
                        DenomKey::Token(TOKEN_A),
                    ),
                )
                .unwrap();
            manager
                .update_already_redeemed(&issuer, &recipient, &DenomKey::Token(TOKEN_A), 1001, 40)
                .unwrap();
            (
                manager.get_state().avl_root_digest,
                manager.reserve_state_digest(&issuer, &DenomKey::Erg),
                manager.reserve_state_digest(&issuer, &DenomKey::Token(TOKEN_A)),
            )
        };

        let mut manager2 = TrackerStateManager::new(dir.path());
        assert_eq!(manager2.get_state().avl_root_digest, root_before);
        assert_eq!(
            manager2.reserve_state_digest(&issuer, &DenomKey::Erg),
            erg_digest_before
        );
        assert_eq!(
            manager2.reserve_state_digest(&issuer, &DenomKey::Token(TOKEN_A)),
            tok_digest_before
        );
        assert_eq!(
            manager2
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            250
        );
        assert_eq!(
            manager2
                .get_already_redeemed(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            40
        );
        // Wrong-denomination lookups find nothing.
        assert!(manager2
            .lookup_note(&issuer, &recipient, &DenomKey::Token(TOKEN_B))
            .is_err());
    }
}
