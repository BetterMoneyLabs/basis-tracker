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
    let result = tracker.add_note(&issuer_pubkey, &note, None);
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

    let result1 = tracker.add_note(&issuer_pubkey_bytes, &note1, None);
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

    let result2 = tracker.add_note(&issuer_pubkey_bytes, &note2, None);
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

    let result1 = tracker.add_note(&issuer_pubkey_bytes, &note1, None);
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

    let result2 = tracker.add_note(&issuer_pubkey_bytes, &note2, None);
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

    let result3 = tracker.add_note(&issuer_pubkey_bytes, &note3, None);
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

    let result1 = tracker.add_note(&issuer1_pubkey, &note1, None);
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

    let result2 = tracker.add_note(&issuer2_pubkey, &note2, None);
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

        manager.add_note(&issuer, &note, None).unwrap();

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

        manager.add_note(&issuer, &note, None).unwrap();

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

        manager.add_note(&issuer, &note, None).unwrap();
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

        manager.add_note(&issuer, &note, None).unwrap();
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

        manager.add_note(&issuer, &note, None).unwrap();
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

        manager.add_note(&issuer, &note, None).unwrap();
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

        manager.add_note(&issuer, &note, None).unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx123", 100).unwrap();

        assert_eq!(manager.rebuild_confirmations().unwrap(), 1);

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

        manager.add_note(&issuer, &note, None).unwrap();

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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient1, 1000, 1), None)
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient2, 5000, 2), None)
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1), None)
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1), None)
            .unwrap();

        // A debtor restating a lower cumulative total must be rejected.
        let lower = create_note(&issuer_secret, &recipient, 500, 2);
        let result = manager.add_note(&issuer, &lower, None);
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1), None)
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1), None)
            .unwrap();

        // Equal total (e.g. a re-signed note) is accepted.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 2), None)
            .unwrap();
        // Higher total is accepted.
        manager
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1500, 3), None)
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
        manager.add_note(&issuer, &note, None).unwrap();

        // A replacement note from create_and_sign carries amount_redeemed = 0;
        // the tracked redemption progress must survive the update.
        let replacement = create_note(&issuer_secret, &recipient, 1500, 2);
        assert_eq!(replacement.amount_redeemed, 0);
        manager.add_note(&issuer, &replacement, None).unwrap();

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
            let result = manager.add_note(&issuer, &note, None);
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 2), None)
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1), None)
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
                None,
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
            let result = manager.add_note(&issuer, &note, None);
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
            .add_note(&issuer, &create_note(&issuer_secret, &recipient, 1000, 1), None)
            .unwrap();
    }
}

/// Multi-denomination acceptance tests for specs/ergo/multi_collateral_reserves.md:
/// independent debt edges per (issuer, recipient, denomination), per-pair reserve
/// trees, and journal-based restart rebuilds reproducing digests exactly.
#[cfg(test)]
mod multi_denomination_tests {
    use crate::{DenomKey, ExchangeAcceptanceView, IouNote, NoteError, TrackerStateManager};
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
                None,
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
                None,
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
                None,
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

    /// §10 exchange (two-phase, issue #17): the from-edge decrease applies
    /// immediately; the to-edge increase is staged and applied only after the
    /// decrease is confirmed and the reserve is verified unchanged.
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
                None,
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
                None,
            )
            .unwrap();

        // Exchange 30 ERG for 40 TOKEN_A at timestamp 2000.
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
                "reserve_box",
            )
            .unwrap();

        // Phase 1: the from-edge decrease is applied immediately...
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );
        let erg_note = manager
            .lookup_note(&issuer, &recipient, &DenomKey::Erg)
            .unwrap();
        assert!(erg_note.verify_signature(&issuer).is_ok());
        assert_eq!(erg_note.timestamp, ts);

        // ...but the to-edge increase is only staged, not applied.
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            50
        );
        let staged = manager.list_pending_exchanges(None, None).unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].1.from_new_total, 70);
        assert_eq!(staged[0].1.from_old_total, 100);
        assert_eq!(staged[0].1.to_note.amount_collected, 90);
        assert_eq!(staged[0].1.reserve_box_id, "reserve_box");
        assert!(matches!(
            staged[0].1.status,
            crate::PendingExchangeStatus::Staged
        ));

        // Processing before the decrease is confirmed waits: nothing happens.
        let processed = manager.process_staged_exchanges(&[]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (0, 0));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            50
        );

        // Simulate the tracker box update being confirmed: the from-edge
        // decrease is now the committed on-chain state.
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();

        // Phase 2 with an intact reserve: the to-edge increase is applied.
        let observation = crate::ExchangeObservation {
            key: staged[0].0,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (1, 0));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            90
        );
        let tok_note = manager
            .lookup_note(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
            .unwrap();
        assert!(tok_note.verify_signature(&issuer).is_ok());
        assert_eq!(tok_note.timestamp, ts);
        // The applied record is removed; the to-edge note is the completion signal.
        assert!(manager
            .list_pending_exchanges(None, None)
            .unwrap()
            .is_empty());

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
            "reserve_box",
        );
        assert!(matches!(result, Err(NoteError::InvalidSignature)));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );
        assert!(manager
            .list_pending_exchanges(None, None)
            .unwrap()
            .is_empty());

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
                "reserve_box",
            ),
            Err(NoteError::InvalidExchange(_))
        ));
    }

    /// Finding #8: the exchange acceptance check sees the exact pre-mutation
    /// state and a rejection aborts the exchange without mutating anything.
    #[test]
    fn exchange_acceptance_check_sees_premutation_state_and_rejection_aborts() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer_secret = [1u8; 32];
        let recipient_secret = [3u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&recipient_secret);

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
                None,
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
                None,
            )
            .unwrap();

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
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        let sigs = (
            sign_as(&issuer_secret, &msg_from, &issuer),
            sign_as(&recipient_secret, &msg_from, &recipient),
            sign_as(&issuer_secret, &msg_to, &issuer),
            sign_as(&recipient_secret, &msg_to, &recipient),
        );

        // The check must observe the exact pre-mutation state: the to-edge note
        // as it exists now (50 TOKEN_A), the issuer's full note set, and the
        // (empty) confirmation records.
        type ObservedState = Option<(u64, u64, usize, usize)>;
        let observed: std::sync::Arc<std::sync::Mutex<ObservedState>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        #[allow(clippy::type_complexity)]
        let check = {
            let observed = std::sync::Arc::clone(&observed);
            Box::new(move |view: ExchangeAcceptanceView| {
                let to = view
                    .to_note
                    .as_ref()
                    .map(|n| (n.amount_collected, n.amount_redeemed));
                *observed.lock().unwrap() = Some((
                    to.map(|(c, _)| c).unwrap_or(0),
                    to.map(|(_, r)| r).unwrap_or(0),
                    view.issuer_notes.len(),
                    view.confirmations.len(),
                ));
                Err("test rejection".to_string())
            }) as Box<dyn FnOnce(ExchangeAcceptanceView) -> Result<(), String> + Send>
        };

        let result = manager.exchange_debt_checked(
            &issuer,
            &recipient,
            &DenomKey::Erg,
            30,
            &DenomKey::Token(TOKEN_A),
            40,
            ts,
            &sigs.0,
            &sigs.1,
            &sigs.2,
            &sigs.3,
            "reserve_box",
            Some(check),
        );
        assert!(
            matches!(&result, Err(NoteError::AcceptancePolicy(msg)) if msg.contains("test rejection")),
            "expected a policy rejection, got {result:?}"
        );

        // Nothing moved.
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
            50
        );

        let observed = observed.lock().unwrap().take().unwrap();
        assert_eq!(
            (observed.0, observed.1),
            (50, 0),
            "to-edge pre-mutation state"
        );
        assert_eq!(observed.2, 2, "issuer notes visible to the check");
        assert_eq!(
            observed.3, 2,
            "one confirmation record per note is visible to the check"
        );

        // An accepting check lets the same (still validly signed) exchange
        // through: the from-edge decrease applies, the to-edge increase is
        // staged (issue #17) and applies once the decrease is confirmed.
        let result = manager.exchange_debt_checked(
            &issuer,
            &recipient,
            &DenomKey::Erg,
            30,
            &DenomKey::Token(TOKEN_A),
            40,
            ts,
            &sigs.0,
            &sigs.1,
            &sigs.2,
            &sigs.3,
            "reserve_box",
            Some(Box::new(|_| Ok(()))),
        );
        assert!(result.is_ok(), "{result:?}");
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
            50,
            "to-edge increase must stay staged until the decrease is confirmed"
        );
        let staged = manager.list_pending_exchanges(None, None).unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].1.to_note.amount_collected, 90);

        // Simulate the confirmation of the from-edge decrease, then apply.
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();
        let observation = crate::ExchangeObservation {
            key: staged[0].0,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (1, 0));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            90
        );
    }

    /// Shared setup for the issue #17 phase-2 tests: add an ERG note (100) and
    /// a TOKEN_A note (50), then stage an exchange of 30 ERG for 40 TOKEN_A at
    /// timestamp 2000. Returns the manager, both pubkeys, and the staged
    /// record's from-edge note key.
    fn stage_exchange() -> (TrackerStateManager, [u8; 33], [u8; 33], [u8; 32]) {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer_secret = [1u8; 32];
        let recipient_secret = [3u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&recipient_secret);

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
                None,
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
                None,
            )
            .unwrap();

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
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        manager
            .exchange_debt(
                &issuer,
                &recipient,
                &DenomKey::Erg,
                30,
                &DenomKey::Token(TOKEN_A),
                40,
                ts,
                &sign_as(&issuer_secret, &msg_from, &issuer),
                &sign_as(&recipient_secret, &msg_from, &recipient),
                &sign_as(&issuer_secret, &msg_to, &issuer),
                &sign_as(&recipient_secret, &msg_to, &recipient),
                "reserve_box",
            )
            .unwrap();

        let staged_key = manager.list_staged_exchanges().unwrap()[0].key;
        (manager, issuer, recipient, staged_key)
    }

    fn issuer_secret() -> [u8; 32] {
        [1u8; 32]
    }

    fn recipient_secret() -> [u8; 32] {
        [3u8; 32]
    }

    /// Issue #17: a staged exchange is cancelled when the from-denomination
    /// reserve box is no longer unspent — an on-chain reserve action (such as
    /// the creditor's pre-built redemption of the old note) happened during the
    /// staging window. The to-edge increase must NOT be applied then.
    #[test]
    fn exchange_to_edge_cancelled_when_reserve_box_spent() {
        let (mut manager, issuer, recipient, staged_key) = stage_exchange();
        let from_digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(from_digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(from_digest, "tracker_box", 5)
            .unwrap();

        // The reserve box was spent on-chain: no R5 digest available.
        let observation = crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: None,
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (0, 1));

        // The to-edge increase was NOT applied; the from-edge decrease stays.
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            50
        );
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );
        let pending = manager.list_pending_exchanges(None, None).unwrap();
        assert_eq!(pending.len(), 1);
        match &pending[0].1.status {
            crate::PendingExchangeStatus::Cancelled { reason } => {
                assert!(
                    reason.contains("no longer unspent"),
                    "unexpected cancel reason: {reason}"
                );
            }
            other => panic!("expected cancelled status, got {other:?}"),
        }

        // Re-processing does nothing: the record is no longer staged.
        let processed = manager
            .process_staged_exchanges(&[crate::ExchangeObservation {
                key: staged_key,
                reserve_r5_digest: None,
            }])
            .unwrap();
        assert_eq!((processed.applied, processed.cancelled), (0, 0));
    }

    /// Issue #17: a staged exchange is cancelled when the reserve box's on-chain
    /// R5 digest diverges from the tracker's reserve-tree digest — the signature
    /// of an unreported on-chain redemption during the staging window.
    #[test]
    fn exchange_to_edge_cancelled_on_reserve_digest_mismatch() {
        let (mut manager, issuer, recipient, staged_key) = stage_exchange();
        let from_digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(from_digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(from_digest, "tracker_box", 5)
            .unwrap();

        // The box is still unspent but its R5 no longer matches the tracker's
        // reserve tree: someone redeemed without telling the tracker.
        let observation = crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: Some([0x9au8; 33]),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (0, 1));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            50
        );
        let pending = manager.list_pending_exchanges(None, None).unwrap();
        match &pending[0].1.status {
            crate::PendingExchangeStatus::Cancelled { reason } => {
                assert!(
                    reason.contains("R5 digest"),
                    "unexpected cancel reason: {reason}"
                );
            }
            other => panic!("expected cancelled status, got {other:?}"),
        }
    }

    /// Issue #20: if the phase-1 atomic commit fails, NOTHING changes — the
    /// from-edge note, the staged records, the live tree digest and the
    /// confirmations are all exactly as before, and the exchange can simply be
    /// retried once storage is healthy again.
    #[test]
    fn exchange_phase1_commit_failure_leaves_state_unchanged() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer = pubkey(&issuer_secret());
        let recipient = pubkey(&recipient_secret());

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret(), &recipient, 100, 1000, DenomKey::Erg),
                None,
            )
            .unwrap();
        manager
            .add_note(
                &issuer,
                &create_note(
                    &issuer_secret(),
                    &recipient,
                    50,
                    1001,
                    DenomKey::Token(TOKEN_A),
                ),
                None,
            )
            .unwrap();

        let digest_before = manager.get_state().avl_root_digest;
        let confirmations_before = manager.all_confirmations();

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
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        let do_exchange = |manager: &mut TrackerStateManager| {
            manager.exchange_debt(
                &issuer,
                &recipient,
                &DenomKey::Erg,
                30,
                &DenomKey::Token(TOKEN_A),
                40,
                ts,
                &sign_as(&issuer_secret(), &msg_from, &issuer),
                &sign_as(&recipient_secret(), &msg_from, &recipient),
                &sign_as(&issuer_secret(), &msg_to, &issuer),
                &sign_as(&recipient_secret(), &msg_to, &recipient),
                "reserve_box",
            )
        };

        manager
            .storage
            .fail_exchange_commits
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = do_exchange(&mut manager);
        assert!(
            matches!(result, Err(NoteError::StorageError(_))),
            "expected injected storage failure, got {result:?}"
        );

        // Everything is unchanged: notes, staged records, tree, confirmations.
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
            50
        );
        assert!(manager
            .list_pending_exchanges(None, None)
            .unwrap()
            .is_empty());
        assert_eq!(manager.get_state().avl_root_digest, digest_before);
        assert_eq!(manager.all_confirmations(), confirmations_before);

        // After the storage failure clears, the same exchange goes through.
        manager
            .storage
            .fail_exchange_commits
            .store(false, std::sync::atomic::Ordering::SeqCst);
        do_exchange(&mut manager).unwrap();
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );
        assert_eq!(manager.list_pending_exchanges(None, None).unwrap().len(), 1);
    }

    /// Issue #20: if the phase-2 atomic commit fails, the to-edge note is not
    /// written, the staged record survives as Staged (previously a failed
    /// staged-record delete led to a spurious cancellation of an exchange that
    /// had actually applied), and the next confirmation cycle applies the
    /// exchange exactly once.
    #[test]
    fn exchange_phase2_commit_failure_keeps_staged_and_retries() {
        let (mut manager, issuer, recipient, staged_key) = stage_exchange();
        let from_digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(from_digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(from_digest, "tracker_box", 5)
            .unwrap();

        let digest_before = manager.get_state().avl_root_digest;
        let make_observation = |manager: &TrackerStateManager| crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };

        manager
            .storage
            .fail_exchange_commits
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = manager.process_staged_exchanges(&[make_observation(&manager)]);
        assert!(
            matches!(result, Err(NoteError::StorageError(_))),
            "expected injected storage failure, got {result:?}"
        );

        // Nothing changed: to-edge untouched, record still staged, digest kept.
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            50
        );
        assert_eq!(manager.get_state().avl_root_digest, digest_before);
        let staged = manager.list_pending_exchanges(None, None).unwrap();
        assert_eq!(staged.len(), 1);
        assert!(
            matches!(staged[0].1.status, crate::PendingExchangeStatus::Staged),
            "record must stay staged, got {:?}",
            staged[0].1.status
        );

        // Next cycle, with storage healthy again: applied exactly once.
        manager
            .storage
            .fail_exchange_commits
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let processed = manager
            .process_staged_exchanges(&[make_observation(&manager)])
            .unwrap();
        assert_eq!((processed.applied, processed.cancelled), (1, 0));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            90
        );
        assert!(manager
            .list_pending_exchanges(None, None)
            .unwrap()
            .is_empty());
    }

    /// Issue #20: if the canonical tree rebuild fails AFTER the atomic storage
    /// commit, the previous (valid) tree stays in place, its digest is kept as
    /// the committed state, and `update_state` retries the rebuild until it
    /// succeeds — a half-rebuilt tree is never published.
    #[test]
    fn exchange_rebuild_failure_keeps_old_tree_and_recovers() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer = pubkey(&issuer_secret());
        let recipient = pubkey(&recipient_secret());

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret(), &recipient, 100, 1000, DenomKey::Erg),
                None,
            )
            .unwrap();
        manager
            .add_note(
                &issuer,
                &create_note(
                    &issuer_secret(),
                    &recipient,
                    50,
                    1001,
                    DenomKey::Token(TOKEN_A),
                ),
                None,
            )
            .unwrap();

        let digest_before = manager.get_state().avl_root_digest;
        manager.fail_tree_rebuild = true;

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
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        // The storage commit succeeds; only the in-memory rebuild fails.
        manager
            .exchange_debt(
                &issuer,
                &recipient,
                &DenomKey::Erg,
                30,
                &DenomKey::Token(TOKEN_A),
                40,
                ts,
                &sign_as(&issuer_secret(), &msg_from, &issuer),
                &sign_as(&recipient_secret(), &msg_from, &recipient),
                &sign_as(&issuer_secret(), &msg_to, &issuer),
                &sign_as(&recipient_secret(), &msg_to, &recipient),
                "reserve_box",
            )
            .unwrap();

        // Storage has the decrease; the live tree is still the old, valid one.
        assert!(manager.tree_dirty);
        assert_eq!(manager.get_state().avl_root_digest, digest_before);
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            100
        );
        assert_eq!(
            manager
                .lookup_note(&issuer, &recipient, &DenomKey::Erg)
                .unwrap()
                .amount_collected,
            70
        );
        assert_eq!(manager.list_pending_exchanges(None, None).unwrap().len(), 1);

        // Once the rebuild works again, the next state update heals the tree.
        manager.fail_tree_rebuild = false;
        manager.update_state();
        assert!(!manager.tree_dirty);
        assert_ne!(manager.get_state().avl_root_digest, digest_before);
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            70
        );
    }

    /// Issue #20 follow-up: `add_note` stores the note (atomically, with its
    /// index entries) BEFORE updating the in-memory tree, so a storage failure
    /// leaves the tree digest, the confirmations and the stored note all
    /// unchanged — and the same note can simply be re-submitted.
    #[test]
    fn add_note_store_failure_leaves_state_unchanged() {
        let mut manager = TrackerStateManager::new_with_temp_storage();
        let issuer = pubkey(&issuer_secret());
        let recipient = pubkey(&recipient_secret());

        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret(), &recipient, 100, 1000, DenomKey::Erg),
                None,
            )
            .unwrap();

        let digest_before = manager.get_state().avl_root_digest;
        let confirmations_before = manager.all_confirmations();

        manager
            .storage
            .fail_note_store
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = manager.add_note(
            &issuer,
            &create_note(&issuer_secret(), &recipient, 150, 1002, DenomKey::Erg),
            None,
        );
        assert!(
            matches!(result, Err(NoteError::StorageError(_))),
            "expected injected storage failure, got {result:?}"
        );

        // Nothing changed: not in the tree, not in storage, not in confirmations.
        assert_eq!(manager.get_state().avl_root_digest, digest_before);
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            100
        );
        assert_eq!(
            manager
                .lookup_note(&issuer, &recipient, &DenomKey::Erg)
                .unwrap()
                .amount_collected,
            100
        );
        assert_eq!(manager.all_confirmations(), confirmations_before);

        // After the failure clears, the same note goes through.
        manager
            .storage
            .fail_note_store
            .store(false, std::sync::atomic::Ordering::SeqCst);
        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret(), &recipient, 150, 1002, DenomKey::Erg),
                None,
            )
            .unwrap();
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Erg)
                .unwrap(),
            150
        );
    }

    /// Issue #17: a staged exchange is cancelled when the from-edge commits to
    /// a different value than the exchange's decrease — the tracker state moved
    /// past the exchange (e.g. the issuer issued new debt on the edge).
    #[test]
    fn exchange_cancelled_when_from_edge_commits_different_value() {
        let (mut manager, issuer, recipient, staged_key) = stage_exchange();

        // The issuer issues new debt on the from-edge before the decrease
        // commits: the committed value will not be the exchange's 70.
        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret(), &recipient, 80, 2001, DenomKey::Erg),
                None,
            )
            .unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();

        let observation = crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (0, 1));
        // The to-edge increase was not applied.
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            50
        );
        let pending = manager.list_pending_exchanges(None, None).unwrap();
        match &pending[0].1.status {
            crate::PendingExchangeStatus::Cancelled { reason } => {
                assert!(
                    reason.contains("committed total is 80"),
                    "unexpected cancel reason: {reason}"
                );
            }
            other => panic!("expected cancelled status, got {other:?}"),
        }
    }

    /// Issue #17: a staged exchange is cancelled when the to-edge changed while
    /// staged (a new note was issued on it) — applying the staged note would
    /// lower that edge.
    #[test]
    fn exchange_cancelled_when_to_edge_changes_during_staging() {
        let (mut manager, issuer, recipient, staged_key) = stage_exchange();

        // New debt on the to-edge before the from-edge decrease commits.
        manager
            .add_note(
                &issuer,
                &create_note(
                    &issuer_secret(),
                    &recipient,
                    60,
                    2001,
                    DenomKey::Token(TOKEN_A),
                ),
                None,
            )
            .unwrap();
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();

        let observation = crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (0, 1));
        // The to-edge kept its new (higher) value; the staged 90 was not applied.
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            60
        );
        let pending = manager.list_pending_exchanges(None, None).unwrap();
        match &pending[0].1.status {
            crate::PendingExchangeStatus::Cancelled { reason } => {
                assert!(
                    reason.contains("to-edge changed"),
                    "unexpected cancel reason: {reason}"
                );
            }
            other => panic!("expected cancelled status, got {other:?}"),
        }
    }

    /// Issue #17: a redemption recorded on the to-edge while the exchange is
    /// staged is carried forward when the staged increase is applied.
    #[test]
    fn exchange_applies_with_to_edge_redeemed_carry_forward() {
        let (mut manager, issuer, recipient, staged_key) = stage_exchange();

        // The creditor redeems 10 TOKEN_A (reported) while the exchange is staged.
        manager
            .record_redemption(&issuer, &recipient, &DenomKey::Token(TOKEN_A), 10)
            .unwrap();

        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();

        let observation = crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (1, 0));
        let tok_note = manager
            .lookup_note(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
            .unwrap();
        assert_eq!(tok_note.amount_collected, 90);
        assert_eq!(
            tok_note.amount_redeemed, 10,
            "redeemed amount carried forward"
        );
    }

    /// Issue #17: only one staged exchange may be in flight per (issuer,
    /// recipient) pair; after it resolves, a new exchange is accepted.
    #[test]
    fn only_one_staged_exchange_per_pair() {
        let (mut manager, issuer, recipient, _staged_key) = stage_exchange();

        // A second exchange for the same pair is rejected while one is staged.
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
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        let issuer_secret = issuer_secret();
        let recipient_secret = recipient_secret();
        let result = manager.exchange_debt(
            &issuer,
            &recipient,
            &DenomKey::Erg,
            10,
            &DenomKey::Token(TOKEN_A),
            10,
            ts2,
            &sign_as(&issuer_secret, &msg_from2, &issuer),
            &sign_as(&recipient_secret, &msg_from2, &recipient),
            &sign_as(&issuer_secret, &msg_to2, &issuer),
            &sign_as(&recipient_secret, &msg_to2, &recipient),
            "reserve_box",
        );
        assert!(
            matches!(&result, Err(NoteError::InvalidExchange(msg)) if msg.contains("already staged")),
            "expected an already-staged rejection, got {result:?}"
        );

        // Resolve the staged exchange, then a new exchange is accepted.
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();
        let staged = manager.list_staged_exchanges().unwrap();
        let observation = crate::ExchangeObservation {
            key: staged[0].key,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        manager.process_staged_exchanges(&[observation]).unwrap();

        let result = manager.exchange_debt(
            &issuer,
            &recipient,
            &DenomKey::Erg,
            10,
            &DenomKey::Token(TOKEN_A),
            10,
            ts2,
            &sign_as(&issuer_secret, &msg_from2, &issuer),
            &sign_as(&recipient_secret, &msg_from2, &recipient),
            &sign_as(&issuer_secret, &msg_to2, &issuer),
            &sign_as(&recipient_secret, &msg_to2, &recipient),
            "reserve_box",
        );
        assert!(result.is_ok(), "{result:?}");
    }

    /// Issue #17: staged exchange records survive a restart (binary round-trip
    /// through the pending_exchanges partition) and apply after reopening.
    #[test]
    fn exchange_staged_record_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let recipient_secret = [3u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&recipient_secret);

        let staged_key = {
            let mut manager = TrackerStateManager::new(dir.path());
            manager
                .add_note(
                    &issuer,
                    &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
                    None,
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
                    None,
                )
                .unwrap();
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
            let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
                crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
            };
            manager
                .exchange_debt(
                    &issuer,
                    &recipient,
                    &DenomKey::Erg,
                    30,
                    &DenomKey::Token(TOKEN_A),
                    40,
                    ts,
                    &sign_as(&issuer_secret, &msg_from, &issuer),
                    &sign_as(&recipient_secret, &msg_from, &recipient),
                    &sign_as(&issuer_secret, &msg_to, &issuer),
                    &sign_as(&recipient_secret, &msg_to, &recipient),
                    "box99",
                )
                .unwrap();
            manager.list_staged_exchanges().unwrap()[0].key
        };

        // Reopen: the staged record survives with all fields intact.
        let mut manager = TrackerStateManager::new(dir.path());
        let pending = manager.list_pending_exchanges(None, None).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, staged_key);
        assert_eq!(pending[0].1.from_new_total, 70);
        assert_eq!(pending[0].1.from_old_total, 100);
        assert_eq!(pending[0].1.to_note.amount_collected, 90);
        assert_eq!(pending[0].1.reserve_box_id, "box99");
        assert!(matches!(
            pending[0].1.status,
            crate::PendingExchangeStatus::Staged
        ));

        // The decrease is still committed-state pending; confirm and apply.
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx1", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();
        let observation = crate::ExchangeObservation {
            key: staged_key,
            reserve_r5_digest: Some(
                manager
                    .reserve_state_digest(&issuer, &DenomKey::Erg)
                    .try_into()
                    .unwrap(),
            ),
        };
        let processed = manager.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (1, 0));
        assert_eq!(
            manager
                .get_total_debt(&issuer, &recipient, &DenomKey::Token(TOKEN_A))
                .unwrap(),
            90
        );
    }

    /// Issue #17 defense in depth: while an exchange decrease is uncommitted,
    /// committed-state lookup proofs are refused — a redemption built from the
    /// stale committed value would over-redeem the edge.
    #[test]
    fn stale_committed_proof_rejected_during_exchange_window() {
        let dir = tempfile::tempdir().unwrap();
        let issuer_secret = [1u8; 32];
        let recipient_secret = [3u8; 32];
        let issuer = pubkey(&issuer_secret);
        let recipient = pubkey(&recipient_secret);

        let mut manager = TrackerStateManager::new(dir.path());
        manager
            .add_note(
                &issuer,
                &create_note(&issuer_secret, &recipient, 100, 1000, DenomKey::Erg),
                None,
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
                None,
            )
            .unwrap();

        // Commit the pre-exchange state (from-edge confirmed at 100).
        let digest = manager.get_state().avl_root_digest;
        manager.mark_notes_pending(digest, "tx0", 1).unwrap();
        manager
            .confirm_pending_notes(digest, "tracker_box", 5)
            .unwrap();

        // Stage the exchange: the live from-edge drops to 70 while the
        // committed value stays 100.
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
        let sign_as = |secret: &[u8; 32], msg: &[u8], pk: &[u8; 33]| {
            crate::schnorr::schnorr_sign(msg, secret, pk).unwrap()
        };
        manager
            .exchange_debt(
                &issuer,
                &recipient,
                &DenomKey::Erg,
                30,
                &DenomKey::Token(TOKEN_A),
                40,
                ts,
                &sign_as(&issuer_secret, &msg_from, &issuer),
                &sign_as(&recipient_secret, &msg_from, &recipient),
                &sign_as(&issuer_secret, &msg_to, &issuer),
                &sign_as(&recipient_secret, &msg_to, &recipient),
                "reserve_box",
            )
            .unwrap();

        // The pre-exchange committed proof (100) is now stale: the live edge is 70.
        let result =
            manager.generate_committed_tracker_lookup_proof(&issuer, &recipient, &DenomKey::Erg);
        assert!(
            matches!(
                &result,
                Err(NoteError::StaleCommittedDebt {
                    committed: 100,
                    local: 70
                })
            ),
            "expected StaleCommittedDebt, got {result:?}"
        );

        // Once the decrease commits, proofs work again (at the new value).
        let digest = manager.get_state().avl_root_digest;
        manager
            .reconcile_with_confirmed_digest(&digest, "tracker_box", 6)
            .unwrap();
        let proof = manager
            .generate_committed_tracker_lookup_proof(&issuer, &recipient, &DenomKey::Erg)
            .unwrap();
        assert_eq!(proof.value, 70u64.to_be_bytes().to_vec());
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
                    None,
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
                    None,
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

/// Debt transfer (triad novation, specs/spec.md "Debt Transfer"), pure-credit
/// edges: A owes B; B is paid by moving part of A's debt to C. Version 3: seven
/// signatures, including both creditors' prior acknowledgments of their edges'
/// current totals, and a one-hour maximum age, so every timestamp in this module
/// is relative to a recent base.
#[cfg(test)]
mod debt_transfer_tests {
    use crate::{
        is_bound_note_timestamp, is_reserved_timestamp, is_setoff_timestamp, is_transfer_timestamp,
        DenomKey, ExchangeAcceptanceView, IouNote, NoteError, SetOffEdge, TrackerStateManager,
        TransferSignatures, TransferTotals, NOVATION_MAX_AGE_MS,
    };
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const OTHER_NFT: [u8; 32] = [0x43; 32];
    const TOKEN: [u8; 32] = [0xa5; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    const E: [u8; 32] = [4u8; 32];
    const Z: [u8; 32] = [5u8; 32];

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A whole second ten minutes before this module's first test started. Transfers older than
    /// NOVATION_MAX_AGE_MS are refused, so the timestamps here are offsets from it; the largest offset is a few
    /// seconds, so every one of them is in the past and well inside the maximum age.
    fn base() -> u64 {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| (now_ms() - 600_000) / 1000 * 1000)
    }

    // Transfer timestamps end in 999 ms.
    fn t1() -> u64 {
        base() + 1999
    }

    fn t2() -> u64 {
        base() + 2999
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

    fn pubkey(secret_key: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret_key).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn create_note(issuer_secret: &[u8; 32], recipient: &[u8; 33], amount: u64, timestamp: u64) -> IouNote {
        IouNote::create_and_sign(*recipient, amount, timestamp, issuer_secret, &NFT, DenomKey::Token(TOKEN))
            .unwrap()
    }

    fn sign(secret: &[u8; 32], issuer: &[u8; 33], recipient: &[u8; 33], total: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::signing_message(issuer, recipient, &DenomKey::Token(TOKEN), total, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, secret, &pubkey(secret)).unwrap()
    }

    /// `secret`'s prior acknowledgment PA(issuer, recipient, prior_total, prior_ts, ts) (tracker v3.2): the edge
    /// is at `prior_total` with stored timestamp `prior_ts` (0 and 0 for an absent edge), for the operation at `ts`.
    fn ack(secret: &[u8; 32], issuer: &[u8; 33], recipient: &[u8; 33], prior_total: u64, prior_ts: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::prior_ack_message(issuer, recipient, &DenomKey::Token(TOKEN), prior_total, prior_ts, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, secret, &pubkey(secret)).unwrap()
    }

    /// The seven signatures of a transfer (version 4) of `amount` from A->old (stored total and timestamp `from`)
    /// to A->new (stored total and timestamp `to`, (0, 0) when absent), by A, `old` and `new` at `ts`.
    fn sigs(old: &[u8; 32], new: &[u8; 32], from: (u64, u64), to: (u64, u64), amount: u64, ts: u64) -> TransferSignatures {
        let (a, o, n) = (pubkey(&A), pubkey(old), pubkey(new));
        let ((from_before, from_ts), (to_before, to_ts)) = (from, to);
        let from_total = from_before.saturating_sub(amount);
        let to_total = to_before + amount;
        TransferSignatures {
            issuer_from: sign(&A, &a, &o, from_total, ts),
            issuer_to: sign(&A, &a, &n, to_total, ts),
            from_recipient_prior: ack(old, &a, &o, from_before, from_ts, ts),
            from_recipient: sign(old, &a, &o, from_total, ts),
            from_recipient_to: sign(old, &a, &n, to_total, ts),
            to_recipient_prior: ack(new, &a, &n, to_before, to_ts, ts),
            to_recipient: sign(new, &a, &n, to_total, ts),
        }
    }

    fn transfer(m: &mut TrackerStateManager, old: &[u8; 32], new: &[u8; 32], amount: u64, ts: u64, s: &TransferSignatures)
        -> Result<TransferTotals, NoteError> {
        transfer_with_priors(m, old, new, amount, ts, s, None, None)
    }

    /// A transfer that also sends the optional, diagnostic prior totals.
    #[allow(clippy::too_many_arguments)]
    fn transfer_with_priors(
        m: &mut TrackerStateManager, old: &[u8; 32], new: &[u8; 32], amount: u64, ts: u64, s: &TransferSignatures,
        from_prior: Option<u64>, to_prior: Option<u64>,
    ) -> Result<TransferTotals, NoteError> {
        transfer_with_diagnostics(m, old, new, amount, ts, s, (from_prior, to_prior), (None, None))
    }

    /// A transfer that sends the diagnostic prior totals (from, to) and prior timestamps (from, to).
    #[allow(clippy::too_many_arguments)]
    fn transfer_with_diagnostics(
        m: &mut TrackerStateManager, old: &[u8; 32], new: &[u8; 32], amount: u64, ts: u64, s: &TransferSignatures,
        totals: (Option<u64>, Option<u64>), stamps: (Option<u64>, Option<u64>),
    ) -> Result<TransferTotals, NoteError> {
        m.transfer_debt(
            &pubkey(&A), &pubkey(old), &pubkey(new), &DenomKey::Token(TOKEN), amount, ts, s, totals.0, totals.1,
            stamps.0, stamps.1,
        )
    }

    fn total(m: &TrackerStateManager, recipient: &[u8; 32]) -> Option<u64> {
        m.get_note_optional(&pubkey(&A), &pubkey(recipient), &DenomKey::Token(TOKEN))
            .unwrap()
            .map(|n| n.amount_collected)
    }

    fn bound_only(m: &TrackerStateManager, recipient: &[u8; 32]) -> bool {
        m.is_bound_only(&pubkey(&A), &pubkey(recipient), &DenomKey::Token(TOKEN)).unwrap()
    }

    /// The timestamp of A's note to B in [`setup`].
    fn tb() -> u64 {
        base() + 1000
    }

    /// The timestamp of A's note to C in [`setup`] (when it has one).
    fn tc() -> u64 {
        base() + 1001
    }

    /// A owes B 100, noted at base + 1000 ms ([`tb`]) (and C `to_before`, if any, at base + 1001 ms, [`tc`]).
    fn setup(to_before: u64) -> TrackerStateManager {
        let mut m = TrackerStateManager::new_with_temp_storage();
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 100, base() + 1000), None).unwrap();
        if to_before > 0 {
            m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), to_before, base() + 1001), None).unwrap();
        }
        m
    }

    fn assert_unchanged(m: &TrackerStateManager, digest_before: &[u8; 33], b: u64, c: Option<u64>) {
        assert_eq!(total(m, &B), Some(b));
        assert_eq!(total(m, &C), c);
        assert_eq!(&m.get_state().avl_root_digest, digest_before);
    }

    #[test]
    fn transfer_timestamps_end_in_999_and_each_operation_has_its_own_residue() {
        // Transfers keep 999; set-offs moved to 998 and bound notes use 997 (each residue belongs to one operation).
        assert!(is_transfer_timestamp(1999) && is_transfer_timestamp(1_730_000_000_999));
        assert!(!is_transfer_timestamp(1998) && !is_transfer_timestamp(2000) && !is_transfer_timestamp(0));
        assert!(is_setoff_timestamp(1998) && !is_setoff_timestamp(1999) && !is_setoff_timestamp(1997));
        assert!(is_bound_note_timestamp(1997) && !is_bound_note_timestamp(1998) && !is_bound_note_timestamp(1996));
        for ms in 995..=999u64 {
            assert!(is_reserved_timestamp(1_730_000_000_000 + ms), "{} ms is reserved", ms);
        }
        for ms in [0u64, 1, 500, 994] {
            assert!(!is_reserved_timestamp(1_730_000_000_000 + ms), "{} ms is ordinary", ms);
        }
        assert_eq!(
            (crate::TRANSFER_TS_MS, crate::SETOFF_TS_MS, crate::BOUND_NOTE_TS_MS, crate::COMPRESS_TS_MS, crate::EXCHANGE_TS_MS, crate::RESERVED_FROM_MS),
            (999, 998, 997, 996, 995, 995)
        );
        // The maximum novation age is one hour.
        assert_eq!(NOVATION_MAX_AGE_MS, 3_600_000);
    }

    #[test]
    fn transfer_moves_debt_to_a_new_creditor() {
        let mut m = setup(0);
        let digest_before = m.get_state().avl_root_digest;
        let totals = transfer(&mut m, &B, &C, 30, t1(), &sigs(&B, &C, (100, tb()), (0, 0), 30, t1())).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 70, to_new_total: 30, from_prior_total: 100, to_prior_total: 0 });
        assert_eq!(total(&m, &B), Some(70));
        assert_eq!(total(&m, &C), Some(30));
        assert_ne!(m.get_state().avl_root_digest, digest_before);
        let c_notes = m.get_recipient_notes(&pubkey(&C)).unwrap();
        assert_eq!(c_notes.len(), 1);
        assert_eq!(c_notes[0].amount_collected, 30);
        let b_note = m.lookup_note(&pubkey(&A), &pubkey(&B), &DenomKey::Token(TOKEN)).unwrap();
        assert_eq!((b_note.amount_collected, b_note.timestamp), (70, t1()));
        assert!(b_note.verify_signature(&pubkey(&A)).is_ok());
        let c_note = m.lookup_note(&pubkey(&A), &pubkey(&C), &DenomKey::Token(TOKEN)).unwrap();
        assert!(c_note.verify_signature(&pubkey(&A)).is_ok());
    }

    #[test]
    fn transfer_adds_to_an_existing_edge_and_can_move_the_whole_debt() {
        let mut m = setup(10);
        let totals = transfer(&mut m, &B, &C, 100, t1(), &sigs(&B, &C, (100, tb()), (10, tc()), 100, t1())).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 0, to_new_total: 110, from_prior_total: 100, to_prior_total: 10 });
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(0), Some(110)));
    }

    #[test]
    fn transferred_debt_can_move_again() {
        let mut m = setup(0);
        transfer(&mut m, &B, &C, 30, t1(), &sigs(&B, &C, (100, tb()), (0, 0), 30, t1())).unwrap();
        transfer(&mut m, &C, &E, 20, t2(), &sigs(&C, &E, (30, t1()), (0, 0), 20, t2())).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C), total(&m, &E)), (Some(70), Some(10), Some(20)));
    }

    #[test]
    fn the_old_creditors_consent_is_required() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let mut s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        s.from_recipient = sign(&C, &pubkey(&A), &pubkey(&B), 70, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::InvalidSignature)));
        s.from_recipient = sign(&A, &pubkey(&A), &pubkey(&B), 70, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn the_old_creditors_consent_binds_the_destination() {
        // A (with a key Z it controls) takes B's from-edge consent and tries to send the debt to Z instead of C.
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let honest = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        let puppet = sigs(&B, &Z, (100, tb()), (0, 0), 30, t1()); // A and Z can produce every signature except B's
        let forged = TransferSignatures {
            issuer_from: puppet.issuer_from,
            issuer_to: puppet.issuer_to,
            from_recipient_prior: honest.from_recipient_prior, // B's real acknowledgment of A->B = 100
            from_recipient: honest.from_recipient,             // B's real consent (from-edge)
            from_recipient_to: honest.from_recipient_to,       // B's real consent (bound to A->C)
            to_recipient_prior: puppet.to_recipient_prior,
            to_recipient: puppet.to_recipient,
        };
        assert!(matches!(transfer(&mut m, &B, &Z, 30, t1(), &forged), Err(NoteError::InvalidSignature)));
        // Without B's to-edge signature at all, it fails too.
        let mut s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        s.from_recipient_to = sign(&A, &pubkey(&A), &pubkey(&C), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn the_new_creditors_acceptance_and_the_issuers_signatures_are_required() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let mut s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        s.to_recipient = sign(&B, &pubkey(&A), &pubkey(&C), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        s.issuer_to = sign(&B, &pubkey(&A), &pubkey(&C), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        s.issuer_from = sign(&C, &pubkey(&A), &pubkey(&B), 70, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::InvalidSignature)));
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 31, t1(), &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn prior_acknowledgments_must_be_signed_by_the_creditors() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        let t = t1();
        let (a, b, c) = (pubkey(&A), pubkey(&B), pubkey(&C));
        // The old creditor's acknowledgment of A->B = 100, signed by the issuer, the new creditor or an outsider.
        for signer in [&A, &C, &Z] {
            let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
            s.from_recipient_prior = ack(signer, &a, &b, 100, tb(), t);
            assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        }
        // The new creditor's acknowledgment of A->C = 10, signed by the issuer, the old creditor or an outsider.
        for signer in [&A, &B, &Z] {
            let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
            s.to_recipient_prior = ack(signer, &a, &c, 10, tc(), t);
            assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        }
        // The right creditor's acknowledgment at another timestamp, of another prior timestamp, or of another
        // edge, does not verify either.
        let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        s.from_recipient_prior = ack(&B, &a, &b, 100, tb(), t - 1000);
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        s.from_recipient_prior = ack(&B, &a, &b, 100, tb() - 1, t);
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        s.to_recipient_prior = ack(&C, &a, &b, 10, tc(), t);
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        // A creditor's acknowledgment and its new-total signature are not interchangeable.
        let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        std::mem::swap(&mut s.from_recipient_prior, &mut s.from_recipient);
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        std::mem::swap(&mut s.to_recipient_prior, &mut s.to_recipient);
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, Some(10));
        transfer(&mut m, &B, &C, 30, t, &sigs(&B, &C, (100, tb()), (10, tc()), 30, t)).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
    }

    #[test]
    fn a_stale_prior_amplification_is_refused() {
        // Finding F-A. A->B = 100 and A->C = 10, and everyone signs a transfer of 30 at T. Before it is submitted,
        // A raises A->B by 5 with a back-dated note (timestamp before T) and a set-off with C->A lowers A->C by 5.
        // Submitted with amount 35, both new totals still match what was signed (105 - 35 = 70, 5 + 35 = 40), so
        // the five version-2 signatures would have moved 35 of B's claim. The prior acknowledgments refuse it.
        let mut m = setup(10);
        m.add_note(&pubkey(&C), &create_note(&C, &pubkey(&A), 5, base() + 1002), None).unwrap();
        let t = t1();
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 105, base() + 1500), None).unwrap();
        let setoff_ts = base() + 1998;
        let edge = |issuer: &[u8; 32], recipient: &[u8; 32], new_total: u64| SetOffEdge {
            issuer_pubkey: pubkey(issuer),
            recipient_pubkey: pubkey(recipient),
            decrease: 5,
            timestamp: setoff_ts,
            issuer_signature: sign(issuer, &pubkey(issuer), &pubkey(recipient), new_total, setoff_ts),
            recipient_signature: sign(recipient, &pubkey(issuer), &pubkey(recipient), new_total, setoff_ts),
        };
        m.set_off(&DenomKey::Token(TOKEN), &[edge(&A, &C, 5), edge(&C, &A, 0)]).unwrap();
        assert!(setoff_ts < t);
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(105), Some(5)));
        let digest = m.get_state().avl_root_digest;
        // The amplified request: B acknowledged 100 and C acknowledged 10, neither of which is stored any more.
        assert!(matches!(transfer(&mut m, &B, &C, 35, t, &s), Err(NoteError::InvalidSignature)));
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 35, t, &s, Some(100), Some(10)), Err(NoteError::StalePrior)));
        // The signed amount is refused too: the edges are no longer at the acknowledged totals.
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 105, Some(5));
        // Control: only the acknowledgments stopped it. Had B and C acknowledged the moved totals (they did not),
        // the same new-total signatures would apply with amount 35.
        let informed = TransferSignatures {
            from_recipient_prior: ack(&B, &pubkey(&A), &pubkey(&B), 105, base() + 1500, t),
            to_recipient_prior: ack(&C, &pubkey(&A), &pubkey(&C), 5, setoff_ts, t),
            ..s
        };
        let totals = transfer(&mut m, &B, &C, 35, t, &informed).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 70, to_new_total: 40, from_prior_total: 105, to_prior_total: 5 });
    }

    #[test]
    fn a_colluding_payer_cannot_underpay_the_new_creditor() {
        // C expects 30. Seeing A->C = 10, it acknowledges 10 and accepts 40. A note C does not know of then raises
        // A->C to 20, and B, colluding, signs a decrease of only 20: 100 - 20 = 80 and 20 + 20 = 40, so every
        // signature except C's acknowledgment matches the request. C's acknowledgment of 10 refuses it.
        let mut m = setup(10);
        let t = t1();
        let honest = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), 20, base() + 1500), None).unwrap(); // the hidden note
        let digest = m.get_state().avl_root_digest;
        let s = TransferSignatures {
            to_recipient_prior: honest.to_recipient_prior, // C acknowledged A->C = 10
            to_recipient: honest.to_recipient,             // C accepted A->C = 40
            ..sigs(&B, &C, (100, tb()), (20, base() + 1500), 20, t) // A and B sign the smaller decrease
        };
        assert!(matches!(transfer(&mut m, &B, &C, 20, t, &s), Err(NoteError::InvalidSignature)));
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 20, t, &s, Some(100), Some(10)), Err(NoteError::StalePrior)));
        assert_unchanged(&m, &digest, 100, Some(20));
        // Control: only C's acknowledgment stopped it. Had C acknowledged the real total 20, it would apply.
        let informed = TransferSignatures { to_recipient_prior: ack(&C, &pubkey(&A), &pubkey(&C), 20, base() + 1500, t), ..s };
        let totals = transfer(&mut m, &B, &C, 20, t, &informed).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 80, to_new_total: 40, from_prior_total: 100, to_prior_total: 20 });
    }

    #[test]
    fn an_absent_to_edge_is_acknowledged_at_zero_and_created() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let t = t1();
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t);
        // The new creditor acknowledges (0, 0); any other prior for the absent edge is refused.
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 30, t, &s, Some(100), Some(5)), Err(NoteError::StalePrior)));
        let wrong = TransferSignatures { to_recipient_prior: ack(&C, &pubkey(&A), &pubkey(&C), 5, 0, t), ..s };
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &wrong), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
        let totals = transfer_with_priors(&mut m, &B, &C, 30, t, &s, Some(100), Some(0)).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 70, to_new_total: 30, from_prior_total: 100, to_prior_total: 0 });
        let c_note = m.lookup_note(&pubkey(&A), &pubkey(&C), &DenomKey::Token(TOKEN)).unwrap();
        assert_eq!((c_note.amount_collected, c_note.amount_redeemed, c_note.timestamp), (30, 0, t));
        assert_eq!(c_note.signature, s.issuer_to);
        assert!(c_note.verify_signature(&pubkey(&A)).is_ok());
        assert_eq!(m.get_recipient_notes(&pubkey(&C)).unwrap().len(), 1);
        assert_eq!(m.get_issuer_notes(&pubkey(&A)).unwrap().len(), 2);
        assert!(bound_only(&m, &B) && bound_only(&m, &C));
    }

    #[test]
    fn diagnostic_prior_totals_must_equal_the_stored_totals() {
        let t = t1();
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        for (from_prior, to_prior) in [(Some(99), None), (None, Some(11)), (Some(100), Some(0)), (Some(10), Some(100))] {
            let r = transfer_with_priors(&mut m, &B, &C, 30, t, &s, from_prior, to_prior);
            assert!(matches!(r, Err(NoteError::StalePrior)), "{:?} {:?}: {:?}", from_prior, to_prior, r);
        }
        assert_unchanged(&m, &digest, 100, Some(10));
        // They are optional: neither, either one or both may be sent.
        for (from_prior, to_prior) in [(None, None), (Some(100), None), (None, Some(10)), (Some(100), Some(10))] {
            let mut m = setup(10);
            let totals = transfer_with_priors(&mut m, &B, &C, 30, t, &s, from_prior, to_prior).unwrap();
            assert_eq!(totals, TransferTotals { from_new_total: 70, to_new_total: 40, from_prior_total: 100, to_prior_total: 10 });
        }
    }

    #[test]
    fn diagnostic_prior_timestamps_must_equal_the_stored_timestamps() {
        // Tracker v3.2: from_prior_timestamp and to_prior_timestamp are diagnostic like the totals: when sent they
        // must equal the stored timestamps (0 for an absent to-edge), else StalePrior; the signatures bind.
        let t = t1();
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        for stamps in [(Some(tb() - 1), None), (None, Some(tc() + 1)), (Some(tc()), Some(tb())), (Some(tb()), Some(0))] {
            let r = transfer_with_diagnostics(&mut m, &B, &C, 30, t, &s, (None, None), stamps);
            assert!(matches!(r, Err(NoteError::StalePrior)), "{:?}: {:?}", stamps, r);
        }
        // With the right totals too.
        let r = transfer_with_diagnostics(&mut m, &B, &C, 30, t, &s, (Some(100), Some(10)), (Some(tb()), Some(tc() - 1)));
        assert!(matches!(r, Err(NoteError::StalePrior)), "{:?}", r);
        assert_unchanged(&m, &digest, 100, Some(10));
        // They are optional: neither, either one or both may be sent.
        for stamps in [(None, None), (Some(tb()), None), (None, Some(tc())), (Some(tb()), Some(tc()))] {
            let mut m = setup(10);
            let totals = transfer_with_diagnostics(&mut m, &B, &C, 30, t, &s, (Some(100), Some(10)), stamps).unwrap();
            assert_eq!(totals, TransferTotals { from_new_total: 70, to_new_total: 40, from_prior_total: 100, to_prior_total: 10 });
        }
        // An absent to-edge has the prior timestamp 0.
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t);
        let mut m = setup(0);
        let r = transfer_with_diagnostics(&mut m, &B, &C, 30, t, &s, (None, None), (None, Some(1)));
        assert!(matches!(r, Err(NoteError::StalePrior)), "{:?}", r);
        transfer_with_diagnostics(&mut m, &B, &C, 30, t, &s, (Some(100), Some(0)), (Some(tb()), Some(0))).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(30)));
    }

    #[test]
    fn a_transfer_older_than_the_maximum_age_is_refused() {
        // A note an hour and a half old; then a transfer just past the maximum age and one just inside it.
        let old = (now_ms() - NOVATION_MAX_AGE_MS - 1_800_000) / 1000 * 1000;
        let mut m = TrackerStateManager::new_with_temp_storage();
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 100, old), None).unwrap();
        let digest = m.get_state().avl_root_digest;
        let expired = at_residue(now_ms() - NOVATION_MAX_AGE_MS - 1_000, 999);
        let r = transfer(&mut m, &B, &C, 30, expired, &sigs(&B, &C, (100, old), (0, 0), 30, expired));
        assert!(matches!(r, Err(NoteError::NovationExpired)), "{:?}", r);
        assert_unchanged(&m, &digest, 100, None);
        let inside = at_residue(now_ms() - NOVATION_MAX_AGE_MS + 10_000, 999);
        transfer(&mut m, &B, &C, 30, inside, &sigs(&B, &C, (100, old), (0, 0), 30, inside)).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(30)));
    }

    #[test]
    fn two_attempts_of_the_same_payment_exclude_each_other_in_either_order() {
        // The same payment (30 from A->B = 100 to A->C = 10), signed at T1 and again at a later T2, both
        // acknowledging the same states of A->B and A->C. Whichever applies first, the other is refused, so 30 moves
        // exactly once (and for good, whatever totals the edges take later: prior_ack_regression_tests).
        let (first, second) = (sigs(&B, &C, (100, tb()), (10, tc()), 30, t1()), sigs(&B, &C, (100, tb()), (10, tc()), 30, t2()));
        assert!(t1() < t2());
        // T1 applies first: A->B is no longer at the acknowledged 100.
        let mut m = setup(10);
        transfer_with_priors(&mut m, &B, &C, 30, t1(), &first, Some(100), Some(10)).unwrap();
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 30, t2(), &second, Some(100), Some(10)), Err(NoteError::StalePrior)));
        assert!(matches!(transfer(&mut m, &B, &C, 30, t2(), &second), Err(NoteError::InvalidSignature)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
        // T2 applies first: both edges are now newer than T1.
        let mut m = setup(10);
        transfer_with_priors(&mut m, &B, &C, 30, t2(), &second, Some(100), Some(10)).unwrap();
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 30, t1(), &first, Some(100), Some(10)), Err(NoteError::StalePrior)));
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &first), Err(NoteError::PastTimestamp)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
    }

    #[test]
    fn the_raised_edges_issuer_signature_cannot_be_used_as_an_ordinary_note() {
        // The issuer's to-edge signature is over a valid note message; its novation timestamp keeps it out of
        // POST /notes, so C cannot collect it without B's edge going down.
        let mut m = setup(0);
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        let standalone = IouNote {
            recipient_pubkey: pubkey(&C),
            amount_collected: 30,
            amount_redeemed: 0,
            timestamp: t1(),
            signature: s.issuer_to,
            tracker_nft_id: NFT,
            denomination: DenomKey::Token(TOKEN),
        };
        assert!(standalone.verify_signature(&pubkey(&A)).is_ok());
        assert!(matches!(m.add_note(&pubkey(&A), &standalone, None), Err(NoteError::ReservedTimestamp)));
        assert_eq!(total(&m, &C), None);
        // Ordinary notes at other milliseconds are unaffected (995-998 are now reserved too, so the latest
        // ordinary millisecond before T1 is 994).
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), 5, base() + 1994), None).unwrap();
    }

    #[test]
    fn a_transfer_needs_a_novation_timestamp() {
        let mut m = setup(0);
        let ts = base() + 2000;
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, ts);
        assert!(matches!(transfer(&mut m, &B, &C, 30, ts, &s), Err(NoteError::InvalidTransfer(_))));
        assert_eq!(total(&m, &B), Some(100));
    }

    #[test]
    fn cannot_transfer_more_than_is_owed_or_nothing() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 101, t1(), &s), Err(NoteError::InvalidTransfer(_))));
        assert!(matches!(transfer(&mut m, &B, &C, 0, t1(), &s), Err(NoteError::InvalidTransfer(_))));
        assert!(matches!(transfer(&mut m, &E, &C, 10, t1(), &s), Err(NoteError::NotFound)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn creditors_must_differ_and_cannot_be_the_issuer() {
        let mut m = setup(0);
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &B, 30, t1(), &s), Err(NoteError::InvalidTransfer(_))));
        assert!(matches!(transfer(&mut m, &B, &A, 30, t1(), &s), Err(NoteError::InvalidTransfer(_))));
    }

    #[test]
    fn timestamps_must_move_forward_so_a_transfer_cannot_be_replayed() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, base() + 999);
        assert!(matches!(transfer(&mut m, &B, &C, 30, base() + 999, &s), Err(NoteError::PastTimestamp)));
        assert_unchanged(&m, &digest, 100, Some(10));
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t1());
        transfer(&mut m, &B, &C, 30, t1(), &s).unwrap();
        assert!(transfer(&mut m, &B, &C, 30, t1(), &s).is_err());
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
        assert!(m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 100, base() + 1000), None).is_err());
    }

    #[test]
    fn refused_while_the_old_edge_has_a_pending_or_committed_on_chain_total() {
        // Tracker v3 reports the scope refusal as OnchainScope (it was InvalidTransfer).
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 100).unwrap();
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::OnchainScope(_))));
        // The scope check runs before the diagnostic prior totals are compared.
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 30, t1(), &s, Some(1), Some(1)), Err(NoteError::OnchainScope(_))));
        m.confirm_pending_notes(digest, "box1", 101).unwrap();
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::OnchainScope(_))));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn only_the_edge_being_lowered_must_be_free_of_on_chain_state() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 100).unwrap();
        m.confirm_pending_notes(digest, "box1", 101).unwrap();
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&E), 50, base() + 1500), None).unwrap();
        let totals = transfer(&mut m, &E, &C, 20, t1(), &sigs(&E, &C, (50, base() + 1500), (10, tc()), 20, t1())).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 30, to_new_total: 30, from_prior_total: 50, to_prior_total: 10 });
    }

    #[test]
    fn a_staged_exchange_on_either_pair_refuses_the_transfer() {
        // An ERG -> TOKEN exchange staged on A's pair with the old or the new creditor (its ERG decrease applies,
        // its TOKEN increase waits for a tracker box) blocks the TOKEN transfer from A->B to A->C.
        let token = DenomKey::Token(TOKEN);
        for (creditor, token_before) in [(B, 100u64), (C, 10)] {
            let mut m = setup(10);
            let (a, r) = (pubkey(&A), pubkey(&creditor));
            m.add_note(&a, &IouNote::create_and_sign(r, 50, base() + 1002, &A, &NFT, DenomKey::Erg).unwrap(), None)
                .unwrap();
            let ts = base() + 1500;
            let sig = |secret: &[u8; 32], d: &DenomKey, total: u64| {
                let msg = crate::schnorr::signing_message(&a, &r, d, total, ts, &NFT);
                crate::schnorr::schnorr_sign(&msg, secret, &pubkey(secret)).unwrap()
            };
            m.exchange_debt(
                &a, &r, &DenomKey::Erg, 10, &token, 10, ts,
                &sig(&A, &DenomKey::Erg, 40), &sig(&creditor, &DenomKey::Erg, 40),
                &sig(&A, &token, token_before + 10), &sig(&creditor, &token, token_before + 10),
                "reserve_box",
            )
            .unwrap();
            assert_eq!(m.list_staged_exchanges().unwrap().len(), 1);
            let digest = m.get_state().avl_root_digest;
            let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t1());
            let r = transfer(&mut m, &B, &C, 30, t1(), &s);
            assert!(matches!(r, Err(NoteError::StagedExchange(_))), "{:?}", r);
            // The staged-exchange guard runs before the diagnostic prior totals are compared.
            let r = transfer_with_priors(&mut m, &B, &C, 30, t1(), &s, Some(1), Some(1));
            assert!(matches!(r, Err(NoteError::StagedExchange(_))), "{:?}", r);
            assert_unchanged(&m, &digest, 100, Some(10));
        }
    }

    #[test]
    fn the_checks_run_in_the_documented_order() {
        // Each request fails two checks; the earlier one (numbered as in transfer_debt's documentation) decides.
        let t = t1();
        let good = sigs(&B, &C, (100, tb()), (10, tc()), 30, t);
        let bad = TransferSignatures { issuer_to: good.issuer_from, ..good }; // one signature does not verify
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        // 2 before 3: an ordinary residue, in the future.
        let future = at_residue(now_ms() + 3_600_000, 0);
        assert!(matches!(transfer(&mut m, &B, &C, 30, future, &good), Err(NoteError::InvalidTransfer(_))));
        // 3 alone: the transfer residue, in the future.
        let future = at_residue(now_ms() + 3_600_000, 999);
        assert!(matches!(transfer(&mut m, &B, &C, 30, future, &good), Err(NoteError::FutureTimestamp)));
        // 4 before 6: expired, from an edge that does not exist.
        let expired = at_residue(now_ms() - NOVATION_MAX_AGE_MS - 1_000, 999);
        assert!(matches!(transfer(&mut m, &E, &C, 30, expired, &good), Err(NoteError::NovationExpired)));
        // 6 before 8: no from-edge, and a wrong diagnostic prior.
        assert!(matches!(transfer_with_priors(&mut m, &E, &C, 30, t, &good, Some(1), None), Err(NoteError::NotFound)));
        // 8 before 9: a wrong diagnostic prior, and more than is owed.
        assert!(matches!(transfer_with_priors(&mut m, &B, &C, 101, t, &good, Some(1), None), Err(NoteError::StalePrior)));
        // 9 before 10: more than is owed, at a timestamp older than the edges.
        assert!(matches!(transfer(&mut m, &B, &C, 101, base() + 999, &good), Err(NoteError::InvalidTransfer(_))));
        // 10 before 13: older than the edges, with a signature that does not verify.
        assert!(matches!(transfer(&mut m, &B, &C, 30, base() + 999, &bad), Err(NoteError::PastTimestamp)));
        // 13 before the acceptance policy: with a signature that does not verify the policy is never asked.
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = asked.clone();
        let refuse: crate::ExchangeAcceptanceCheckFn = Box::new(move |_view: ExchangeAcceptanceView| {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            Err("refused".to_string())
        });
        let r = m.transfer_debt_checked(
            &pubkey(&A), &pubkey(&B), &pubkey(&C), &DenomKey::Token(TOKEN), 30, t, &bad, None, None, None, None,
            Some(refuse),
        );
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "{:?}", r);
        assert!(!asked.load(std::sync::atomic::Ordering::SeqCst));
        assert_unchanged(&m, &digest, 100, Some(10));
    }

    #[test]
    fn the_nft_and_range_checks_run_before_the_signatures() {
        let t = t1();
        // 10 before 11, and 11 before 13: A->C is bound to another tracker NFT.
        let mut m = setup(0);
        let foreign = IouNote::create_and_sign(pubkey(&C), 10, base() + 1001, &A, &OTHER_NFT, DenomKey::Token(TOKEN)).unwrap();
        m.add_note(&pubkey(&A), &foreign, None).unwrap();
        let bad = TransferSignatures { issuer_to: [0u8; 65], ..sigs(&B, &C, (100, tb()), (10, tc()), 30, t) };
        assert!(matches!(transfer(&mut m, &B, &C, 30, base() + 999, &bad), Err(NoteError::PastTimestamp)));
        assert!(matches!(transfer(&mut m, &B, &C, 30, t, &bad), Err(NoteError::TrackerIdMismatch)));
        // 12 before 13: the new to-edge total leaves the contract's signed 64-bit range.
        let mut m = setup(0);
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), i64::MAX as u64, base() + 1001), None).unwrap();
        let r = transfer(&mut m, &B, &C, 30, t, &bad);
        assert!(matches!(r, Err(NoteError::AmountExceedsContractRange { field: "to_new_total", .. })), "{:?}", r);
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(100), Some(i64::MAX as u64)));
    }

    #[test]
    fn a_storage_failure_leaves_state_unchanged() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let confirmations = m.all_confirmations();
        m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::StorageError(_))));
        assert_unchanged(&m, &digest, 100, None);
        assert_eq!(m.all_confirmations(), confirmations);
        m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
        transfer(&mut m, &B, &C, 30, t1(), &s).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(30)));
    }

    #[test]
    fn the_new_creditors_acceptance_policy_sees_the_pre_transfer_state_and_can_refuse() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t1());
        let refuse: crate::ExchangeAcceptanceCheckFn = Box::new(|view: ExchangeAcceptanceView| {
            assert_eq!(view.to_note.map(|n| n.amount_collected), Some(10));
            Err("C accepts at most 20 from A".to_string())
        });
        let r = m.transfer_debt_checked(
            &pubkey(&A), &pubkey(&B), &pubkey(&C), &DenomKey::Token(TOKEN), 30, t1(), &s, None, None, None, None,
            Some(refuse),
        );
        assert!(matches!(r, Err(NoteError::AcceptancePolicy(_))));
        assert_unchanged(&m, &digest, 100, Some(10));
        let accept: crate::ExchangeAcceptanceCheckFn = Box::new(|_view: ExchangeAcceptanceView| Ok(()));
        m.transfer_debt_checked(
            &pubkey(&A), &pubkey(&B), &pubkey(&C), &DenomKey::Token(TOKEN), 30, t1(), &s, Some(100), Some(10),
            Some(tb()), Some(tc()), Some(accept),
        )
        .unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
    }

    #[test]
    fn the_tree_stays_reproducible_after_a_transfer() {
        let mut m = setup(10);
        transfer(&mut m, &B, &C, 30, t1(), &sigs(&B, &C, (100, tb()), (10, tc()), 30, t1())).unwrap();
        let live = m.get_state().avl_root_digest;
        m.rebuild_tree_canonically().unwrap();
        m.update_state();
        assert_eq!(m.get_state().avl_root_digest, live);
    }

    #[test]
    fn a_transfer_marks_both_edges_bound_only() {
        // Tracker v3: both edges a transfer changed take only bound notes afterwards. The marks are written in the
        // transfer's own storage batch, so a transfer that fails to commit leaves none.
        let mut m = setup(10);
        let s = sigs(&B, &C, (100, tb()), (10, tc()), 30, t1());
        m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(transfer(&mut m, &B, &C, 30, t1(), &s), Err(NoteError::StorageError(_))));
        assert!(!bound_only(&m, &B) && !bound_only(&m, &C));
        m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
        transfer(&mut m, &B, &C, 30, t1(), &s).unwrap();
        assert!(bound_only(&m, &B) && bound_only(&m, &C));
        // An edge the transfer did not touch is not marked.
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&E), 5, base() + 1500), None).unwrap();
        assert!(!bound_only(&m, &E));
        // On a marked edge an ordinary note is refused and a bound note (residue 997, the issuer's acknowledgment
        // of the current state: total 40 at T1) applies. Timestamps are relative to T1, which ends in 999.
        let ordinary = create_note(&A, &pubkey(&C), 50, t1() + 1001);
        assert!(matches!(m.add_note(&pubkey(&A), &ordinary, None), Err(NoteError::BoundNoteRequired)));
        let ts = t1() + 1998;
        let prior = crate::PriorAck {
            signature: ack(&A, &pubkey(&A), &pubkey(&C), 40, t1(), ts),
            prior_total: Some(40),
            prior_timestamp: Some(t1()),
        };
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), 50, ts), Some(prior)).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(50)));
    }

    #[test]
    fn a_transfer_and_its_bound_only_marks_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let s = sigs(&B, &C, (100, tb()), (0, 0), 30, t1());
        {
            let mut m = TrackerStateManager::new(dir.path());
            m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 100, base() + 1000), None).unwrap();
            transfer(&mut m, &B, &C, 30, t1(), &s).unwrap();
        }
        let m = TrackerStateManager::new(dir.path());
        for (recipient, total, signature) in [(&B, 70, s.issuer_from), (&C, 30, s.issuer_to)] {
            let n = m.lookup_note(&pubkey(&A), &pubkey(recipient), &DenomKey::Token(TOKEN)).unwrap();
            assert_eq!((n.amount_collected, n.timestamp, n.signature), (total, t1(), signature));
            assert!(bound_only(&m, recipient));
        }
    }
}

/// Multilateral set-off (obligation clearing), version 3, pure-credit edges:
/// lowered edges, two signatures each at the edge's own timestamp (residue 998,
/// at most an hour old, newer than the edge), balanced for every participant.
#[cfg(test)]
mod setoff_tests {
    use crate::{
        DenomKey, IouNote, NoteError, SetOffEdge, SetOffEdgeTotal, TrackerStateManager, TransferSignatures,
        MAX_SETOFF_EDGES, NOVATION_MAX_AGE_MS,
    };
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const OTHER_NFT: [u8; 32] = [0x43; 32];
    const TOKEN: [u8; 32] = [0xa5; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    const D: [u8; 32] = [4u8; 32];

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A whole second ten minutes before this module's first test started. Tracker v3 refuses set-off
    /// timestamps older than NOVATION_MAX_AGE_MS, so the timestamps here are offsets from it (they were fixed
    /// 1970-era values before): the largest offset is a few seconds, so every one of them is in the past and
    /// well inside the maximum age.
    fn base() -> u64 {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| (now_ms() - 600_000) / 1000 * 1000)
    }

    /// The set-off timestamp most tests sign at. Set-off timestamps end in 998 ms (the set-off residue; 999
    /// belongs to transfers).
    fn ts0() -> u64 {
        base() + 5998
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

    fn pk(secret: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn note(m: &mut TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32], amount: u64, ts: u64) {
        let n = IouNote::create_and_sign(pk(recipient), amount, ts, issuer, &NFT, DenomKey::Token(TOKEN)).unwrap();
        m.add_note(&pk(issuer), &n, None).unwrap();
    }

    fn total(m: &TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32]) -> u64 {
        m.get_note_optional(&pk(issuer), &pk(recipient), &DenomKey::Token(TOKEN)).unwrap().map(|n| n.amount_collected).unwrap()
    }

    /// The stored timestamp of the edge issuer -> recipient.
    fn stamp(m: &TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32]) -> u64 {
        m.lookup_note(&pk(issuer), &pk(recipient), &DenomKey::Token(TOKEN)).unwrap().timestamp
    }

    fn sign(secret: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &DenomKey::Token(TOKEN), total, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, secret, &pk(secret)).unwrap()
    }

    /// An edge lowered from `before` by `decrease`, signed by both parties at its own timestamp `ts`.
    fn edge(issuer: &[u8; 32], recipient: &[u8; 32], before: u64, decrease: u64, ts: u64) -> SetOffEdge {
        let new_total = before.saturating_sub(decrease);
        SetOffEdge {
            issuer_pubkey: pk(issuer),
            recipient_pubkey: pk(recipient),
            decrease,
            timestamp: ts,
            issuer_signature: sign(issuer, issuer, recipient, new_total, ts),
            recipient_signature: sign(recipient, issuer, recipient, new_total, ts),
        }
    }

    /// `secret`'s prior acknowledgment PA(issuer, recipient, prior_total, prior_ts, ts) (tracker v3.2).
    fn ack(secret: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], prior_total: u64, prior_ts: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::prior_ack_message(&pk(issuer), &pk(recipient), &DenomKey::Token(TOKEN), prior_total, prior_ts, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, secret, &pk(secret)).unwrap()
    }

    /// The seven signatures of a transfer (version 4) of `amount` of A's debt from A->B (stored total and
    /// timestamp `from`) to A->C (stored total and timestamp `to`, (0, 0) when absent), by A, B and C at `ts`.
    fn transfer_sigs(from: (u64, u64), to: (u64, u64), amount: u64, ts: u64) -> TransferSignatures {
        let ((from_before, from_ts), (to_before, to_ts)) = (from, to);
        let (from_total, to_total) = (from_before - amount, to_before + amount);
        TransferSignatures {
            issuer_from: sign(&A, &A, &B, from_total, ts),
            issuer_to: sign(&A, &A, &C, to_total, ts),
            from_recipient_prior: ack(&B, &A, &B, from_before, from_ts, ts),
            from_recipient: sign(&B, &A, &B, from_total, ts),
            from_recipient_to: sign(&B, &A, &C, to_total, ts),
            to_recipient_prior: ack(&C, &A, &C, to_before, to_ts, ts),
            to_recipient: sign(&C, &A, &C, to_total, ts),
        }
    }

    fn net(m: &TrackerStateManager, who: &[u8; 32], all: &[[u8; 32]]) -> i64 {
        let mut n = 0i64;
        for other in all {
            if other == who { continue; }
            let owed_to_me = m.get_note_optional(&pk(other), &pk(who), &DenomKey::Token(TOKEN)).unwrap().map(|x| x.amount_collected).unwrap_or(0);
            let i_owe = m.get_note_optional(&pk(who), &pk(other), &DenomKey::Token(TOKEN)).unwrap().map(|x| x.amount_collected).unwrap_or(0);
            n += owed_to_me as i64 - i_owe as i64;
        }
        n
    }

    /// A owes B 10, B owes C 7, C owes A 5.
    fn triangle() -> TrackerStateManager {
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 10, base() + 1000);
        note(&mut m, &B, &C, 7, base() + 1001);
        note(&mut m, &C, &A, 5, base() + 1002);
        m
    }

    #[test]
    fn a_cycle_is_cleared_and_every_net_position_is_unchanged() {
        let mut m = triangle();
        let t = ts0();
        let all = [A, B, C];
        let nets: Vec<i64> = all.iter().map(|w| net(&m, w, &all)).collect();
        let r = m.set_off(&DenomKey::Token(TOKEN), &[edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)]).unwrap();
        assert_eq!(r.total_cleared, 15);
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (5, 2, 0));
        let after: Vec<i64> = all.iter().map(|w| net(&m, w, &all)).collect();
        assert_eq!(nets, after);
        // The lowered notes still carry their issuers' signatures over the new totals.
        let n = m.lookup_note(&pk(&A), &pk(&B), &DenomKey::Token(TOKEN)).unwrap();
        assert!(n.verify_signature(&pk(&A)).is_ok());
        assert_eq!(n.timestamp, t);
    }

    #[test]
    fn a_bilateral_pair_and_a_figure_eight_clear() {
        let t = ts0();
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 8, base() + 1000);
        note(&mut m, &B, &A, 3, base() + 1001);
        m.set_off(&DenomKey::Token(TOKEN), &[edge(&A, &B, 8, 3, t), edge(&B, &A, 3, 3, t)]).unwrap();
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (5, 0));
        // Two cycles through B: A<->B and B<->C, cleared together.
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 4, base() + 1000);
        note(&mut m, &B, &A, 4, base() + 1001);
        note(&mut m, &B, &C, 2, base() + 1002);
        note(&mut m, &C, &B, 2, base() + 1003);
        let r = m.set_off(&DenomKey::Token(TOKEN), &[edge(&A, &B, 4, 4, t), edge(&B, &A, 4, 4, t), edge(&B, &C, 2, 2, t), edge(&C, &B, 2, 2, t)]).unwrap();
        assert_eq!(r.total_cleared, 12);
    }

    fn unchanged(m: &TrackerStateManager, digest: &[u8; 33]) {
        assert_eq!((total(m, &A, &B), total(m, &B, &C), total(m, &C, &A)), (10, 7, 5));
        assert_eq!(&m.get_state().avl_root_digest, digest);
    }

    #[test]
    fn unbalanced_decreases_are_refused() {
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        // B would lose 5 of its claim on A while its own debt to C falls by only 4.
        let r = m.set_off(&DenomKey::Token(TOKEN), &[edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 4, t), edge(&C, &A, 5, 5, t)]);
        assert!(matches!(r, Err(NoteError::InvalidSetOff(_))));
        unchanged(&m, &digest);
    }

    #[test]
    fn signatures_from_a_valid_set_off_cannot_be_used_in_an_unbalanced_subset() {
        // Everyone signed the full cycle. Submitting only two of its edges (dropping C->A) would leave A paying
        // B without being paid by C: the balance check refuses it.
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        let full = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        let r = m.set_off(&DenomKey::Token(TOKEN), &full[..2]);
        assert!(matches!(r, Err(NoteError::InvalidSetOff(_))));
        unchanged(&m, &digest);
        m.set_off(&DenomKey::Token(TOKEN), &full).unwrap();
    }

    #[test]
    fn both_parties_of_every_edge_must_sign() {
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        let mut e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        e[0].recipient_signature = sign(&A, &A, &B, 5, t); // the issuer signs in the creditor's place
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), &e), Err(NoteError::InvalidSignature)));
        let mut e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        e[1].issuer_signature = sign(&C, &B, &C, 2, t);
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), &e), Err(NoteError::InvalidSignature)));
        // Signatures over other totals do not verify.
        let mut e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        e[2].decrease = 4;
        e[0].decrease = 4;
        e[1].decrease = 4;
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), &e), Err(NoteError::InvalidSignature)));
        unchanged(&m, &digest);
    }

    #[test]
    fn malformed_set_offs_are_refused() {
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        let d = DenomKey::Token(TOKEN);
        let invalid = |r: Result<crate::SetOffResult, NoteError>| matches!(r, Err(NoteError::InvalidSetOff(_)));
        assert!(invalid(m.set_off(&d, &[edge(&A, &B, 10, 5, t)])));
        assert!(invalid(m.set_off(&d, &[edge(&A, &B, 10, 5, t), edge(&A, &B, 10, 5, t)])));
        assert!(invalid(m.set_off(&d, &[edge(&A, &A, 10, 5, t), edge(&B, &C, 7, 5, t)])));
        assert!(invalid(m.set_off(&d, &[edge(&A, &B, 10, 0, t), edge(&B, &A, 0, 0, t)])));
        // A decrease larger than the edge.
        assert!(invalid(m.set_off(&d, &[edge(&A, &B, 10, 6, t), edge(&B, &C, 7, 6, t), edge(&C, &A, 5, 6, t)])));
        // Not the set-off residue (998 ms).
        let t6 = base() + 6000;
        assert!(invalid(m.set_off(&d, &[edge(&A, &B, 10, 5, t6), edge(&B, &C, 7, 5, t6), edge(&C, &A, 5, 5, t6)])));
        // Too many edges.
        let many: Vec<SetOffEdge> = (0..=MAX_SETOFF_EDGES).map(|_| edge(&A, &B, 10, 1, t)).collect();
        assert!(invalid(m.set_off(&d, &many)));
        // A missing edge.
        assert!(matches!(m.set_off(&d, &[edge(&A, &D, 10, 5, t), edge(&D, &A, 10, 5, t)]), Err(NoteError::NotFound)));
        // Not newer than the edges.
        let early = base() + 998;
        assert!(matches!(m.set_off(&d, &[edge(&A, &B, 10, 5, early), edge(&B, &C, 7, 5, early), edge(&C, &A, 5, 5, early)]), Err(NoteError::PastTimestamp)));
        unchanged(&m, &digest);
    }

    #[test]
    fn the_same_pair_twice_at_different_edge_timestamps_is_refused() {
        // Per-edge timestamps must not let one pair count as two edges. A owes B 10 and B owes A 10. The request
        // lowers A->B by 5 twice, at two different timestamps (each copy validly signed by both parties from the
        // stored total 10), and B->A by 10 once. On its own numbers it balances: A and B each lose 10 as creditor
        // and 10 as issuer. Applied, both copies would be written from the same stored total, so A->B would fall by
        // only 5 while B->A falls by 10: A's net position would change. The distinct-pair check refuses it.
        let mut m = TrackerStateManager::new_with_temp_storage();
        let d = DenomKey::Token(TOKEN);
        note(&mut m, &A, &B, 10, base() + 1000);
        note(&mut m, &B, &A, 10, base() + 1001);
        let digest = m.get_state().avl_root_digest;
        let t = ts0();
        let (first, second, back) = (edge(&A, &B, 10, 5, t), edge(&A, &B, 10, 5, t + 1000), edge(&B, &A, 10, 10, t));
        assert_ne!(first.timestamp, second.timestamp);
        for request in [
            vec![first.clone(), second.clone(), back.clone()],
            vec![first.clone(), back.clone(), second.clone()],
            vec![back.clone(), second.clone(), first.clone()],
        ] {
            // Not vacuous: the request balances for every key, so only the pair check can refuse it.
            let mut net = std::collections::HashMap::new();
            for e in &request {
                *net.entry(e.recipient_pubkey).or_insert(0i64) += e.decrease as i64;
                *net.entry(e.issuer_pubkey).or_insert(0i64) -= e.decrease as i64;
            }
            assert!(net.values().all(|v| *v == 0), "{:?}", net);
            match m.set_off(&d, &request) {
                Err(NoteError::InvalidSetOff(msg)) => assert!(msg.contains("more than once"), "{}", msg),
                other => panic!("expected InvalidSetOff, got {:?}", other),
            }
            assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (10, 10));
            assert_eq!(m.get_state().avl_root_digest, digest);
        }
        // The signatures are genuine: one copy with its matching decrease of B->A is an honest set-off and applies.
        m.set_off(&d, &[second, edge(&B, &A, 10, 5, t + 1000)]).unwrap();
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (5, 5));
    }

    #[test]
    fn a_set_off_cannot_be_replayed() {
        let mut m = triangle();
        let t = ts0();
        let e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        m.set_off(&DenomKey::Token(TOKEN), &e).unwrap();
        assert!(m.set_off(&DenomKey::Token(TOKEN), &e).is_err());
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (5, 2, 0));
    }

    #[test]
    fn edges_with_on_chain_state_are_refused() {
        // Tracker v3 reports the scope refusal as OnchainScope (it was InvalidSetOff): 409 onchain_scope.
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 100).unwrap();
        let e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), &e), Err(NoteError::OnchainScope(_))));
        unchanged(&m, &digest);
    }

    #[test]
    fn a_storage_failure_leaves_state_unchanged_and_the_tree_stays_reproducible() {
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        let e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), &e), Err(NoteError::StorageError(_))));
        unchanged(&m, &digest);
        m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
        m.set_off(&DenomKey::Token(TOKEN), &e).unwrap();
        let live = m.get_state().avl_root_digest;
        m.rebuild_tree_canonically().unwrap();
        m.update_state();
        assert_eq!(m.get_state().avl_root_digest, live);
    }

    #[test]
    fn set_off_edges_take_only_bound_notes_and_other_edges_take_ordinary_notes() {
        // (Named ordinary_notes_still_work_after_a_set_off before tracker v3.1; renamed for what it checks.)
        // Tracker v3: every edge a set-off lowered is bound-only afterwards, so a new deal on it is a bound note
        // (residue 997, with the issuer's acknowledgment of the current total and timestamp) and an ordinary note
        // there is refused. Edges the set-off did not touch take ordinary notes as before.
        let mut m = triangle();
        let t = ts0();
        m.set_off(&DenomKey::Token(TOKEN), &[edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)]).unwrap();
        // A new deal: A owes B 3 more (cumulative 8), at a later timestamp.
        let ordinary = IouNote::create_and_sign(pk(&B), 8, base() + 7000, &A, &NFT, DenomKey::Token(TOKEN)).unwrap();
        assert!(matches!(m.add_note(&pk(&A), &ordinary, None), Err(NoteError::BoundNoteRequired)));
        let bound = IouNote::create_and_sign(pk(&B), 8, base() + 7997, &A, &NFT, DenomKey::Token(TOKEN)).unwrap();
        let prior = crate::PriorAck {
            signature: ack(&A, &A, &B, 5, t, base() + 7997),
            prior_total: Some(5),
            prior_timestamp: Some(t),
        };
        m.add_note(&pk(&A), &bound, Some(prior)).unwrap();
        assert_eq!(total(&m, &A, &B), 8);
        // An edge outside the set-off still takes ordinary notes.
        note(&mut m, &A, &C, 4, base() + 7000);
        assert_eq!(total(&m, &A, &C), 4);
    }

    #[test]
    fn a_set_off_marks_every_edge_bound_only_and_the_marks_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let d = DenomKey::Token(TOKEN);
        let t = ts0();
        let cycle = [(A, B), (B, C), (C, A)];
        let e = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        {
            let mut m = TrackerStateManager::new(dir.path());
            note(&mut m, &A, &B, 10, base() + 1000);
            note(&mut m, &B, &C, 7, base() + 1001);
            note(&mut m, &C, &A, 5, base() + 1002);
            note(&mut m, &A, &C, 3, base() + 1003); // outside the cycle
            // A set-off that fails to commit marks nothing (the marks are in its storage batch).
            m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(matches!(m.set_off(&d, &e), Err(NoteError::StorageError(_))));
            for (i, r) in cycle {
                assert!(!m.is_bound_only(&pk(&i), &pk(&r), &d).unwrap());
            }
            m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
            m.set_off(&d, &e).unwrap();
            for (i, r) in cycle {
                assert!(m.is_bound_only(&pk(&i), &pk(&r), &d).unwrap());
            }
            assert!(!m.is_bound_only(&pk(&A), &pk(&C), &d).unwrap());
        }
        let mut m = TrackerStateManager::new(dir.path());
        for (i, r) in cycle {
            assert!(m.is_bound_only(&pk(&i), &pk(&r), &d).unwrap());
        }
        assert!(!m.is_bound_only(&pk(&A), &pk(&C), &d).unwrap());
        // After the restart an ordinary note on a set-off edge is still refused (t ends in 998, so t + 1002 is an
        // ordinary millisecond), and the untouched edge still takes ordinary notes.
        let ordinary = IouNote::create_and_sign(pk(&B), 8, t + 1002, &A, &NFT, DenomKey::Token(TOKEN)).unwrap();
        assert!(matches!(m.add_note(&pk(&A), &ordinary, None), Err(NoteError::BoundNoteRequired)));
        note(&mut m, &A, &C, 6, t + 1002);
        assert_eq!((total(&m, &A, &B), total(&m, &A, &C)), (5, 6));
    }

    #[test]
    fn set_offs_use_their_own_residue() {
        // 999 belongs to transfers and 997 to bound notes: a set-off signed at either is refused.
        let mut m = triangle();
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        for ts in [t + 1, t - 1] {
            let e = [edge(&A, &B, 10, 5, ts), edge(&B, &C, 7, 5, ts), edge(&C, &A, 5, 5, ts)];
            assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), &e), Err(NoteError::InvalidSetOff(_))), "ts {}", ts);
        }
        unchanged(&m, &digest);
    }

    #[test]
    fn a_moved_edge_is_re_signed_on_its_own_while_the_other_edges_keep_their_timestamp() {
        // Everyone signs the cycle A->B->C->A, 5 each, at t. Before it is submitted, a new deal raises B->C from 7
        // to 9 (after t). B->C's signatures are stale now (the edge is newer than t and its total moved), so the
        // signed set-off is refused and changes nothing. Only B and C re-sign B->C, at a newer timestamp t2 and the
        // new total 9 - 5 = 4; A->B and C->A keep the signatures they made at t, and the set-off applies.
        let mut m = triangle();
        let d = DenomKey::Token(TOKEN);
        let t = ts0();
        let signed = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
        note(&mut m, &B, &C, 9, t + 500); // ends in 498 ms: an ordinary note
        let digest = m.get_state().avl_root_digest;
        assert!(matches!(m.set_off(&d, &signed), Err(NoteError::PastTimestamp)));
        // Re-signed at t, B->C is still not newer than its stored note.
        let r = m.set_off(&d, &[signed[0].clone(), edge(&B, &C, 9, 5, t), signed[2].clone()]);
        assert!(matches!(r, Err(NoteError::PastTimestamp)), "{:?}", r);
        // At the newer timestamp, signatures over the old new total (7 - 5 = 2) do not verify.
        let t2 = t + 1000;
        let r = m.set_off(&d, &[signed[0].clone(), edge(&B, &C, 7, 5, t2), signed[2].clone()]);
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "{:?}", r);
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (10, 9, 5));
        assert_eq!(m.get_state().avl_root_digest, digest);

        let all = [A, B, C];
        let nets: Vec<i64> = all.iter().map(|w| net(&m, w, &all)).collect();
        let r = m.set_off(&d, &[signed[0].clone(), edge(&B, &C, 9, 5, t2), signed[2].clone()]).unwrap();
        assert_eq!(r.total_cleared, 15);
        // The result reports every edge's new total and its own timestamp, in request order.
        assert_eq!(
            r.new_totals,
            vec![
                SetOffEdgeTotal { issuer_pubkey: pk(&A), recipient_pubkey: pk(&B), new_total: 5, timestamp: t },
                SetOffEdgeTotal { issuer_pubkey: pk(&B), recipient_pubkey: pk(&C), new_total: 4, timestamp: t2 },
                SetOffEdgeTotal { issuer_pubkey: pk(&C), recipient_pubkey: pk(&A), new_total: 0, timestamp: t },
            ]
        );
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (5, 4, 0));
        let after: Vec<i64> = all.iter().map(|w| net(&m, w, &all)).collect();
        assert_eq!(nets, after);
        // Each lowered note is stored at its own edge's timestamp with its issuer's signature, and is bound-only.
        assert_eq!((stamp(&m, &A, &B), stamp(&m, &B, &C), stamp(&m, &C, &A)), (t, t2, t));
        for (i, r) in [(A, B), (B, C), (C, A)] {
            assert!(m.lookup_note(&pk(&i), &pk(&r), &d).unwrap().verify_signature(&pk(&i)).is_ok());
            assert!(m.is_bound_only(&pk(&i), &pk(&r), &d).unwrap());
        }
        // The live tree is the canonical one.
        let live = m.get_state().avl_root_digest;
        m.rebuild_tree_canonically().unwrap();
        m.update_state();
        assert_eq!(m.get_state().avl_root_digest, live);
    }

    #[test]
    fn no_two_edges_of_a_signed_triangle_can_be_assembled_into_a_set_off() {
        // Every participant signed the cycle A->B->C->A (B->C at its own, later timestamp). Any two of its three
        // edges leave a participant paying without being paid: the balance check refuses every pair, in either
        // order, and nothing changes. Only the whole cycle applies.
        let mut m = triangle();
        let d = DenomKey::Token(TOKEN);
        let t = ts0();
        let full = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t + 1000), edge(&C, &A, 5, 5, t)];
        let digest = m.get_state().avl_root_digest;
        for (i, j) in [(0, 1), (1, 2), (0, 2), (1, 0), (2, 1), (2, 0)] {
            match m.set_off(&d, &[full[i].clone(), full[j].clone()]) {
                Err(NoteError::InvalidSetOff(msg)) => assert!(msg.contains("do not balance"), "edges {} and {}: {}", i, j, msg),
                other => panic!("edges {} and {}: {:?}", i, j, other),
            }
        }
        unchanged(&m, &digest);
        m.set_off(&d, &full).unwrap();
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (5, 2, 0));
    }

    #[test]
    fn transfer_signatures_are_not_set_off_edges_and_set_off_signatures_are_not_a_transfer() {
        // A owes B 100 and B owes A 30. In a transfer of 30 from A->B to A->C at T (999 ms), A and B sign
        // M(A,B,70,T): exactly the pair a set-off lowering A->B by 30 at T needs. In a set-off lowering A->B and
        // B->A by 30 at S (998 ms), A and B sign M(A,B,70,S): exactly a transfer's issuer_from and from_recipient
        // signatures at S. The residues keep each kind out of the other operation.
        let d = DenomKey::Token(TOKEN);
        let fresh = || {
            let mut m = TrackerStateManager::new_with_temp_storage();
            note(&mut m, &A, &B, 100, base() + 1000);
            note(&mut m, &B, &A, 30, base() + 1001);
            m
        };
        let mut m = fresh();
        let digest = m.get_state().avl_root_digest;
        let is_unchanged = |m: &TrackerStateManager| {
            (total(m, &A, &B), total(m, &B, &A)) == (100, 30)
                && m.get_state().avl_root_digest == digest
                && m.get_note_optional(&pk(&A), &pk(&C), &d).unwrap().is_none()
        };
        // The transfer's A->B signatures as a set-off edge, balanced by B->A signed at T as well or at 998 ms.
        let t = base() + 2999;
        let transfer = transfer_sigs((100, base() + 1000), (0, 0), 30, t);
        let from_edge = SetOffEdge {
            issuer_pubkey: pk(&A),
            recipient_pubkey: pk(&B),
            decrease: 30,
            timestamp: t,
            issuer_signature: transfer.issuer_from,
            recipient_signature: transfer.from_recipient,
        };
        for back_ts in [t, base() + 2998] {
            let r = m.set_off(&d, &[from_edge.clone(), edge(&B, &A, 30, 30, back_ts)]);
            assert!(matches!(r, Err(NoteError::InvalidSetOff(_))), "{:?}", r);
        }
        assert!(is_unchanged(&m));
        // The set-off's A->B signatures as a transfer's from-edge signatures, every other slot validly signed at S.
        let s = base() + 3998;
        let setoff = [edge(&A, &B, 100, 30, s), edge(&B, &A, 30, 30, s)];
        let at_s = TransferSignatures {
            issuer_from: setoff[0].issuer_signature,
            from_recipient: setoff[0].recipient_signature,
            ..transfer_sigs((100, base() + 1000), (0, 0), 30, s)
        };
        let r = m.transfer_debt(&pk(&A), &pk(&B), &pk(&C), &d, 30, s, &at_s, None, None, None, None);
        assert!(matches!(r, Err(NoteError::InvalidTransfer(_))), "{:?}", r);
        assert!(is_unchanged(&m));
        // Both sets of signatures are genuine on their own routes.
        m.set_off(&d, &setoff).unwrap();
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (70, 0));
        let mut other = fresh();
        other.transfer_debt(&pk(&A), &pk(&B), &pk(&C), &d, 30, t, &transfer, None, None, None, None).unwrap();
        assert_eq!((total(&other, &A, &B), total(&other, &A, &C)), (70, 30));
    }

    #[test]
    fn a_set_off_issuer_signature_cannot_be_submitted_as_a_note() {
        // A and B sign a set-off lowering A->B from 100 to 70 at S. Before it applies, a transfer at an earlier T
        // lowers A->B to 40. A's set-off signature is a valid note message that would now RAISE A->B from 40 to
        // 70 at a newer timestamp: its residue (998) keeps it out of POST /notes. The set-off itself no longer
        // verifies (A->B would fall to 10, not to the signed 70).
        let d = DenomKey::Token(TOKEN);
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 100, base() + 1000);
        note(&mut m, &B, &A, 30, base() + 1001);
        let s = base() + 3998;
        let setoff = [edge(&A, &B, 100, 30, s), edge(&B, &A, 30, 30, s)];
        let t = base() + 2999;
        m.transfer_debt(&pk(&A), &pk(&B), &pk(&C), &d, 60, t, &transfer_sigs((100, base() + 1000), (0, 0), 60, t), None, None, None, None)
            .unwrap();
        assert_eq!(total(&m, &A, &B), 40);
        let as_note = IouNote {
            recipient_pubkey: pk(&B),
            amount_collected: 70,
            amount_redeemed: 0,
            timestamp: s,
            signature: setoff[0].issuer_signature,
            tracker_nft_id: NFT,
            denomination: d,
        };
        assert!(as_note.verify_signature(&pk(&A)).is_ok());
        assert!(matches!(m.add_note(&pk(&A), &as_note, None), Err(NoteError::ReservedTimestamp)));
        assert!(matches!(m.set_off(&d, &setoff), Err(NoteError::InvalidSignature)));
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A), total(&m, &A, &C)), (40, 30, 60));
    }

    #[test]
    fn every_edge_needs_the_set_off_residue() {
        // One edge at another operation's residue (999 transfer, 997 bound note, 996 and 995 reserved) or at an
        // ordinary millisecond refuses the whole set-off, wherever it is and even when every other edge is at 998.
        let mut m = triangle();
        let d = DenomKey::Token(TOKEN);
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        for wrong in [t + 1, t - 1, t - 2, t - 3, t + 2, t - 498] {
            for position in 0..3 {
                let mut stamps = [t, t, t];
                stamps[position] = wrong;
                let e = [edge(&A, &B, 10, 5, stamps[0]), edge(&B, &C, 7, 5, stamps[1]), edge(&C, &A, 5, 5, stamps[2])];
                let r = m.set_off(&d, &e);
                assert!(matches!(r, Err(NoteError::InvalidSetOff(_))), "{} at edge {}: {:?}", wrong, position, r);
            }
        }
        unchanged(&m, &digest);
    }

    #[test]
    fn every_edges_timestamp_must_be_inside_the_maximum_age_and_not_in_the_future() {
        // The edges were noted two hours ago. Each edge's own timestamp is checked: one edge older than the
        // maximum age, or in the future, refuses the set-off whichever edge it is.
        let old = (now_ms() - 2 * NOVATION_MAX_AGE_MS) / 1000 * 1000;
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 10, old);
        note(&mut m, &B, &A, 4, old + 1);
        let d = DenomKey::Token(TOKEN);
        let digest = m.get_state().avl_root_digest;
        let fresh = at_residue(now_ms() - 60_000, 998);
        let expired = at_residue(now_ms() - NOVATION_MAX_AGE_MS - 1_000, 998);
        let future = at_residue(now_ms() + NOVATION_MAX_AGE_MS, 998);
        for (ab, ba, expected) in [(expired, fresh, "expired"), (fresh, expired, "expired"), (fresh, future, "future"), (future, fresh, "future")] {
            let r = m.set_off(&d, &[edge(&A, &B, 10, 4, ab), edge(&B, &A, 4, 4, ba)]);
            match (expected, &r) {
                ("expired", Err(NoteError::NovationExpired)) | ("future", Err(NoteError::FutureTimestamp)) => {}
                _ => panic!("{} ({}, {}): {:?}", expected, ab, ba, r),
            }
        }
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (10, 4));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Just inside the maximum age, next to a fresh edge: the set-off applies, each edge at its own timestamp.
        let inside = at_residue(now_ms() - NOVATION_MAX_AGE_MS + 10_000, 998);
        let r = m.set_off(&d, &[edge(&A, &B, 10, 4, inside), edge(&B, &A, 4, 4, fresh)]).unwrap();
        assert_eq!((r.new_totals[0].timestamp, r.new_totals[1].timestamp), (inside, fresh));
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (6, 0));
        assert_eq!((stamp(&m, &A, &B), stamp(&m, &B, &A)), (inside, fresh));
    }

    #[test]
    fn a_staged_exchange_on_any_pair_refuses_the_set_off() {
        // An ERG -> TOKEN exchange staged on one of the cycle's pairs (its ERG decrease applies, its TOKEN increase
        // waits for a tracker box) blocks the TOKEN set-off with StagedExchange (it was InvalidSetOff): 409
        // staged_exchange. The guard runs after the balance check and before any edge is read.
        let token = DenomKey::Token(TOKEN);
        for (issuer, creditor, token_before) in [(A, B, 10u64), (B, C, 7), (C, A, 5)] {
            let mut m = triangle();
            let t = ts0();
            let (i, r) = (pk(&issuer), pk(&creditor));
            m.add_note(&i, &IouNote::create_and_sign(r, 50, base() + 1003, &issuer, &NFT, DenomKey::Erg).unwrap(), None)
                .unwrap();
            let xts = base() + 1500;
            let sig = |secret: &[u8; 32], denomination: &DenomKey, total: u64| {
                let msg = crate::schnorr::signing_message(&i, &r, denomination, total, xts, &NFT);
                crate::schnorr::schnorr_sign(&msg, secret, &pk(secret)).unwrap()
            };
            m.exchange_debt(
                &i, &r, &DenomKey::Erg, 10, &token, 10, xts,
                &sig(&issuer, &DenomKey::Erg, 40), &sig(&creditor, &DenomKey::Erg, 40),
                &sig(&issuer, &token, token_before + 10), &sig(&creditor, &token, token_before + 10),
                "reserve_box",
            )
            .unwrap();
            assert_eq!(m.list_staged_exchanges().unwrap().len(), 1);
            let digest = m.get_state().avl_root_digest;
            let cycle = [edge(&A, &B, 10, 5, t), edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)];
            let result = m.set_off(&token, &cycle);
            assert!(matches!(result, Err(NoteError::StagedExchange(_))), "{:?}", result);
            // Balance first: an unbalanced set-off is InvalidSetOff even with the exchange staged.
            let result = m.set_off(&token, &cycle[..2]);
            assert!(matches!(result, Err(NoteError::InvalidSetOff(_))), "{:?}", result);
            // Before the edges are read: with an edge that does not exist it is still StagedExchange.
            let back = [edge(&issuer, &creditor, token_before, 5, t), edge(&creditor, &issuer, 0, 5, t)];
            let result = m.set_off(&token, &back);
            assert!(matches!(result, Err(NoteError::StagedExchange(_))), "{:?}", result);
            unchanged(&m, &digest);
        }
    }

    #[test]
    fn the_checks_run_in_the_documented_order() {
        // Each set-off fails two checks; the earlier one (numbered as in set_off's documentation) decides.
        let mut m = triangle();
        let d = DenomKey::Token(TOKEN);
        let t = ts0();
        let digest = m.get_state().avl_root_digest;
        let invalid_set_off = |r: Result<crate::SetOffResult, NoteError>| match r {
            Err(NoteError::InvalidSetOff(msg)) => msg,
            other => panic!("expected InvalidSetOff, got {:?}", other),
        };
        let future = at_residue(now_ms() + NOVATION_MAX_AGE_MS, 998);
        let expired = at_residue(now_ms() - NOVATION_MAX_AGE_MS - 1_000, 998);
        // 1 before 2: a single edge, at an ordinary millisecond.
        assert!(invalid_set_off(m.set_off(&d, &[edge(&A, &B, 10, 5, t + 2)])).contains("at least two edges"));
        // 2, for one edge: the residue before the time; an ordinary millisecond in the future.
        let r = m.set_off(&d, &[edge(&A, &B, 10, 5, t), edge(&B, &A, 0, 5, future + 2)]);
        assert!(invalid_set_off(r).contains("998 ms"));
        // 2 before 3: an expired edge next to a self-edge.
        let r = m.set_off(&d, &[edge(&A, &A, 10, 5, t), edge(&B, &C, 7, 5, expired)]);
        assert!(matches!(r, Err(NoteError::NovationExpired)), "{:?}", r);
        // 2 before 4: an edge in the future, unbalanced.
        let r = m.set_off(&d, &[edge(&A, &B, 10, 5, future), edge(&B, &C, 7, 4, t)]);
        assert!(matches!(r, Err(NoteError::FutureTimestamp)), "{:?}", r);
        // 3 before 4: a repeated edge, unbalanced.
        assert!(invalid_set_off(m.set_off(&d, &[edge(&A, &B, 10, 5, t), edge(&A, &B, 10, 4, t)])).contains("more than once"));
        // 4 before 6: unbalanced, with an edge that does not exist.
        assert!(invalid_set_off(m.set_off(&d, &[edge(&A, &D, 10, 5, t), edge(&B, &C, 7, 5, t)])).contains("do not balance"));
        // 6 runs edge by edge: the first edge's signature is checked before the second edge is read.
        let mut bad_first = edge(&A, &B, 10, 5, t);
        bad_first.issuer_signature = [0u8; 65];
        let r = m.set_off(&d, &[bad_first, edge(&B, &A, 0, 5, t)]);
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "{:?}", r);
        // 6, for one edge: the amounts before the time; a decrease above the total at a timestamp older than the edge.
        let early = base() + 998;
        let r = m.set_off(&d, &[edge(&A, &B, 10, 11, early), edge(&B, &A, 0, 11, early)]);
        assert!(invalid_set_off(r).contains("exceeds"));
        // 6, for one edge: the time before the signatures.
        let mut bad_early = edge(&A, &B, 10, 5, early);
        bad_early.recipient_signature = [0u8; 65];
        let r = m.set_off(&d, &[bad_early, edge(&B, &C, 7, 5, t), edge(&C, &A, 5, 5, t)]);
        assert!(matches!(r, Err(NoteError::PastTimestamp)), "{:?}", r);
        unchanged(&m, &digest);
    }

    #[test]
    fn the_edges_must_share_one_tracker_nft_and_it_is_checked_before_their_signatures() {
        let mut m = TrackerStateManager::new_with_temp_storage();
        let d = DenomKey::Token(TOKEN);
        let t = ts0();
        note(&mut m, &A, &B, 10, base() + 1000);
        // B -> A is bound to another tracker NFT (stored directly; POST /notes would refuse it).
        let foreign = IouNote::create_and_sign(pk(&A), 10, base() + 1001, &B, &OTHER_NFT, d).unwrap();
        m.add_note(&pk(&B), &foreign, None).unwrap();
        let digest = m.get_state().avl_root_digest;
        let mut back = edge(&B, &A, 10, 4, t);
        back.recipient_signature = [0u8; 65];
        let r = m.set_off(&d, &[edge(&A, &B, 10, 4, t), back]);
        assert!(matches!(r, Err(NoteError::TrackerIdMismatch)), "{:?}", r);
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (10, 10));
        assert_eq!(m.get_state().avl_root_digest, digest);
    }
}

/// Tracker v3 timestamp residues, bound notes and bound-only edges, for add_note
/// (POST /notes) and update_note: residues 995-999 are reserved; a note at
/// residue 997 is a bound note, applied only with the issuer's acknowledgment of
/// the edge's stored total and, since v3.1, only within the maximum age; an
/// edge a novation changed takes only bound notes.
#[cfg(test)]
mod bound_note_tests {
    use crate::persistence::NoteStorage;
    use crate::{DenomKey, IouNote, NoteError, NoteKey, PriorAck, TrackerStateManager, NOVATION_MAX_AGE_MS};
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const TOKEN: [u8; 32] = [0xa5; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];

    fn pk(secret: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn denom() -> DenomKey {
        DenomKey::Token(TOKEN)
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A whole second ten minutes before this module's first test started. Tracker v3.1 refuses bound notes older
    /// than NOVATION_MAX_AGE_MS (design decision D-1), so the timestamps here are offsets from it (they were fixed
    /// 1970-era values before): the largest offset is a few seconds, so every one of them is in the past and well
    /// inside the maximum age, and each keeps its residue (`at(1997)` ends in 997 ms).
    fn base() -> u64 {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| (now_ms() - 600_000) / 1000 * 1000)
    }

    /// `offset` milliseconds after [`base`].
    fn at(offset: u64) -> u64 {
        base() + offset
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

    /// `signer`'s signature over the (issuer -> recipient) note message at `total` and `ts`.
    fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &denom(), total, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
    }

    /// A's note to `recipient` at the cumulative `total` and `ts`.
    fn note(recipient: &[u8; 32], total: u64, ts: u64) -> IouNote {
        IouNote::create_and_sign(pk(recipient), total, ts, &A, &NFT, denom()).unwrap()
    }

    /// `signer`'s signature over the prior acknowledgment PA(A, recipient, prior, prior_ts, ts) (tracker v3.2).
    fn sign_ack(signer: &[u8; 32], recipient: &[u8; 32], (prior, prior_ts): (u64, u64), ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::prior_ack_message(&pk(&A), &pk(recipient), &denom(), prior, prior_ts, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
    }

    /// A prior acknowledgment (bound notes version 3) of the A -> recipient state `prior` (total, timestamp; (0, 0)
    /// for an absent edge) for a bound note at `ts`, signed by `signer`, with the optional diagnostic `prior_total`.
    fn ack(signer: &[u8; 32], recipient: &[u8; 32], prior: (u64, u64), ts: u64, prior_total: Option<u64>) -> PriorAck {
        PriorAck { signature: sign_ack(signer, recipient, prior, ts), prior_total, prior_timestamp: None }
    }

    fn add(m: &mut TrackerStateManager, recipient: &[u8; 32], total: u64, ts: u64, prior: Option<PriorAck>) -> Result<(), NoteError> {
        m.add_note(&pk(&A), &note(recipient, total, ts), prior)
    }

    fn total(m: &TrackerStateManager, recipient: &[u8; 32]) -> Option<u64> {
        m.get_note_optional(&pk(&A), &pk(recipient), &denom()).unwrap().map(|n| n.amount_collected)
    }

    fn key(recipient: &[u8; 32]) -> [u8; 32] {
        NoteKey::from_assets(&pk(&A), &pk(recipient), &denom()).key_hash
    }

    fn bound_only(m: &TrackerStateManager, recipient: &[u8; 32]) -> bool {
        m.is_bound_only(&pk(&A), &pk(recipient), &denom()).unwrap()
    }

    /// Mark A -> recipient bound-only through the novation commit path, without a novation (the transfer and
    /// set-off tests check that the novations themselves write the marks).
    fn mark_bound_only(m: &TrackerStateManager, recipient: &[u8; 32]) {
        m.storage.commit_note_batch(&[], &[key(recipient)]).unwrap();
    }

    /// The stored state of A -> B after [`setup`]: total 10 at at(1000).
    fn b0() -> (u64, u64) {
        (10, at(1000))
    }

    /// A owes B 10, noted at at(1000) (residue 000).
    fn setup() -> TrackerStateManager {
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &B, 10, at(1000), None).unwrap();
        m
    }

    #[test]
    fn reserved_residues_are_refused_and_ordinary_ones_accepted() {
        let mut m = setup(); // its note at residue 000 was accepted
        let digest = m.get_state().avl_root_digest;
        for ts in [1995, 1996, 1998, 1999] {
            let r = add(&mut m, &B, 20, at(ts), None);
            assert!(matches!(r, Err(NoteError::ReservedTimestamp)), "ts {}: {:?}", ts, r);
        }
        // 997 is the bound-note residue: without a prior acknowledgment it is refused as reserved.
        assert!(matches!(add(&mut m, &B, 20, at(1997), None), Err(NoteError::ReservedTimestamp)));
        // A new edge's first note too.
        assert!(matches!(add(&mut m, &C, 5, at(1999), None), Err(NoteError::ReservedTimestamp)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // The largest ordinary residue (994) and the smallest (000) are accepted.
        add(&mut m, &B, 20, at(1994), None).unwrap();
        add(&mut m, &B, 30, at(2000), None).unwrap();
        assert_eq!(total(&m, &B), Some(30));
    }

    #[test]
    fn the_existing_checks_keep_their_precedence() {
        let mut m = setup();
        // Contract range, future timestamp, past timestamp, no decrease and the note's own signature are all
        // checked before the residue and bound-note rules.
        assert!(matches!(
            add(&mut m, &B, i64::MAX as u64 + 1, at(1999), None),
            Err(NoteError::AmountExceedsContractRange { .. })
        ));
        let future = now_ms() + 3_600_000;
        assert!(matches!(add(&mut m, &B, 20, future - future % 1000 + 995, None), Err(NoteError::FutureTimestamp)));
        assert!(matches!(add(&mut m, &B, 20, at(998), None), Err(NoteError::PastTimestamp)));
        assert!(matches!(
            add(&mut m, &B, 5, at(1997), Some(ack(&A, &B, b0(), at(1997), Some(10)))),
            Err(NoteError::DebtDecreaseNotPermitted { .. })
        ));
        let mut forged = note(&B, 20, at(1999));
        forged.signature = note(&B, 21, at(1999)).signature;
        assert!(matches!(m.add_note(&pk(&A), &forged, None), Err(NoteError::InvalidSignature)));
        let mut forged = note(&B, 20, at(1997));
        forged.signature = note(&B, 21, at(1997)).signature;
        assert!(matches!(m.add_note(&pk(&A), &forged, Some(ack(&A, &B, b0(), at(1997), Some(9)))), Err(NoteError::InvalidSignature)));
        assert_eq!(total(&m, &B), Some(10));
    }

    #[test]
    fn a_bound_note_applies_only_with_the_issuers_acknowledgment_of_the_current_total() {
        let mut m = setup(); // P = 10
        let digest = m.get_state().avl_root_digest;
        let ts = at(1997);
        // A prior at a wrong total: StalePrior when its total is sent, InvalidPriorSignature when it is not.
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, (9, at(1000)), ts, Some(9)))), Err(NoteError::StalePrior)));
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, (9, at(1000)), ts, None))), Err(NoteError::InvalidPriorSignature)));
        // Claiming the right total does not help a signature over another total.
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, (9, at(1000)), ts, Some(10)))), Err(NoteError::InvalidPriorSignature)));
        // The acknowledgment is bound to the note's own timestamp, and to the edge's stored timestamp.
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, b0(), at(1994), Some(10)))), Err(NoteError::InvalidPriorSignature)));
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, (10, at(999)), ts, Some(10)))), Err(NoteError::InvalidPriorSignature)));
        // A bound notes version 2 acknowledgment (the note message M(A,B,P,ts) at the stored total) is refused.
        let v2 = PriorAck { signature: sign(&A, &A, &B, 10, ts), prior_total: Some(10), prior_timestamp: None };
        assert!(matches!(add(&mut m, &B, 15, ts, Some(v2)), Err(NoteError::InvalidPriorSignature)));
        // The acknowledgment of the right state with a wrong diagnostic prior_timestamp is stale.
        let stamped = PriorAck { prior_timestamp: Some(at(999)), ..ack(&A, &B, b0(), ts, None) };
        assert!(matches!(add(&mut m, &B, 15, ts, Some(stamped)), Err(NoteError::StalePrior)));
        assert_eq!(total(&m, &B), Some(10));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // With the issuer's acknowledgment of the stored state the bound note applies (prior_timestamp sent too).
        add(&mut m, &B, 15, ts, Some(PriorAck { prior_timestamp: Some(at(1000)), ..ack(&A, &B, b0(), ts, Some(10)) })).unwrap();
        let stored = m.lookup_note(&pk(&A), &pk(&B), &denom()).unwrap();
        assert_eq!((stored.amount_collected, stored.timestamp), (15, ts));
        assert!(stored.verify_signature(&pk(&A)).is_ok());
        // prior_total is optional; the next bound note acknowledges the new state (15 at ts).
        add(&mut m, &B, 18, at(2997), Some(ack(&A, &B, (15, ts), at(2997), None))).unwrap();
        assert_eq!(total(&m, &B), Some(18));
        // An acknowledgment of the old state is stale now.
        assert!(matches!(add(&mut m, &B, 25, at(3997), Some(ack(&A, &B, (15, ts), at(3997), Some(15)))), Err(NoteError::StalePrior)));
        assert!(matches!(add(&mut m, &B, 25, at(3997), Some(ack(&A, &B, (15, ts), at(3997), None))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &B), Some(18));
    }

    #[test]
    fn a_prior_signed_by_the_recipient_instead_of_the_issuer_is_refused() {
        let mut m = setup();
        // B signs the right message (A -> B at 10, at the note's timestamp); it is still not the issuer's
        // acknowledgment. A third party's signature is refused as well.
        assert!(matches!(add(&mut m, &B, 15, at(1997), Some(ack(&B, &B, b0(), at(1997), Some(10)))), Err(NoteError::InvalidPriorSignature)));
        assert!(matches!(add(&mut m, &B, 15, at(1997), Some(ack(&C, &B, b0(), at(1997), None))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &B), Some(10));
    }

    #[test]
    fn an_equal_total_is_not_an_increase() {
        let mut m = setup();
        assert!(matches!(add(&mut m, &B, 10, at(1997), Some(ack(&A, &B, b0(), at(1997), Some(10)))), Err(NoteError::NotAnIncrease)));
        // On an absent edge P = 0, so a bound note of 0 is not an increase either.
        assert!(matches!(add(&mut m, &C, 0, at(1997), Some(ack(&A, &C, (0, 0), at(1997), Some(0)))), Err(NoteError::NotAnIncrease)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
    }

    #[test]
    fn a_first_bound_note_on_an_absent_edge_acknowledges_zero() {
        let mut m = setup();
        // On an absent edge (P, p) = (0, 0): an acknowledgment of any other state does not fit.
        assert!(matches!(add(&mut m, &C, 7, at(1997), Some(ack(&A, &C, (10, 0), at(1997), Some(10)))), Err(NoteError::StalePrior)));
        assert!(matches!(add(&mut m, &C, 7, at(1997), Some(ack(&A, &C, (10, 0), at(1997), None))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &C), None);
        assert!(matches!(add(&mut m, &C, 7, at(1997), Some(ack(&A, &C, (0, 1), at(1997), Some(0)))), Err(NoteError::InvalidPriorSignature)));
        let stamped = PriorAck { prior_timestamp: Some(1), ..ack(&A, &C, (0, 0), at(1997), Some(0)) };
        assert!(matches!(add(&mut m, &C, 7, at(1997), Some(stamped)), Err(NoteError::StalePrior)));
        add(&mut m, &C, 7, at(1997), Some(PriorAck { prior_timestamp: Some(0), ..ack(&A, &C, (0, 0), at(1997), Some(0)) })).unwrap();
        assert_eq!(total(&m, &C), Some(7));
        assert_eq!(m.get_recipient_notes(&pk(&C)).unwrap().len(), 1);
        assert!(!bound_only(&m, &C), "a bound note does not make its edge bound-only");
    }

    #[test]
    fn the_prior_acknowledgment_itself_cannot_be_submitted_as_a_note() {
        // Bound notes version 3: the acknowledgment is a 105-byte prior-acknowledgment message, never a note
        // message, so as a note's signature it does not verify (version 2's acknowledgment was a valid note message
        // at P and was kept out only by the residue and NotAnIncrease, which stay).
        let mut m = setup();
        let digest = m.get_state().avl_root_digest;
        let prior = ack(&A, &B, b0(), at(1997), Some(10));
        let as_note = IouNote {
            recipient_pubkey: pk(&B),
            amount_collected: 10,
            amount_redeemed: 0,
            timestamp: at(1997),
            signature: prior.signature,
            tracker_nft_id: NFT,
            denomination: denom(),
        };
        assert!(as_note.verify_signature(&pk(&A)).is_err());
        assert!(matches!(m.add_note(&pk(&A), &as_note, None), Err(NoteError::InvalidSignature)));
        assert!(matches!(m.add_note(&pk(&A), &as_note, Some(prior)), Err(NoteError::InvalidSignature)));
        // The same for a first bound note's acknowledgment of (0, 0) on an absent edge.
        let zero = ack(&A, &C, (0, 0), at(1997), Some(0));
        let zero_note = IouNote { recipient_pubkey: pk(&C), amount_collected: 0, signature: zero.signature, ..as_note.clone() };
        assert!(zero_note.verify_signature(&pk(&A)).is_err());
        assert!(matches!(m.add_note(&pk(&A), &zero_note, None), Err(NoteError::InvalidSignature)));
        assert!(matches!(m.add_note(&pk(&A), &zero_note, Some(zero)), Err(NoteError::InvalidSignature)));
        // NotAnIncrease is kept: a validly signed bound note at the stored total is still refused.
        assert!(matches!(add(&mut m, &B, 10, at(1997), Some(prior)), Err(NoteError::NotAnIncrease)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        assert_eq!(m.get_state().avl_root_digest, digest);
    }

    #[test]
    fn a_prior_on_a_note_that_is_not_a_bound_note_is_an_invalid_request() {
        let mut m = setup();
        assert!(matches!(add(&mut m, &B, 20, at(2000), Some(ack(&A, &B, b0(), at(2000), Some(10)))), Err(NoteError::InvalidRequest(_))));
        assert!(matches!(add(&mut m, &C, 5, at(2994), Some(ack(&A, &C, (0, 0), at(2994), None))), Err(NoteError::InvalidRequest(_))));
        // At another operation's residue the reserved-residue refusal comes first.
        assert!(matches!(add(&mut m, &B, 20, at(1999), Some(ack(&A, &B, b0(), at(1999), None))), Err(NoteError::ReservedTimestamp)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        // Without the prior the same ordinary note applies.
        add(&mut m, &B, 20, at(2000), None).unwrap();
        assert_eq!(total(&m, &B), Some(20));
    }

    #[test]
    fn on_a_bound_only_edge_an_unbound_note_is_refused_and_a_bound_note_accepted() {
        let mut m = setup();
        add(&mut m, &C, 4, at(1001), None).unwrap();
        mark_bound_only(&m, &B);
        assert!(bound_only(&m, &B) && !bound_only(&m, &C));
        let digest = m.get_state().avl_root_digest;
        for ts in [2000, 2500, 2994] {
            let r = add(&mut m, &B, 20, at(ts), None);
            assert!(matches!(r, Err(NoteError::BoundNoteRequired)), "ts {}: {:?}", ts, r);
        }
        // With a prior at an ordinary residue the bound-only refusal comes first.
        assert!(matches!(add(&mut m, &B, 20, at(2000), Some(ack(&A, &B, b0(), at(2000), Some(10)))), Err(NoteError::BoundNoteRequired)));
        // Reserved residues stay reserved.
        assert!(matches!(add(&mut m, &B, 20, at(2999), None), Err(NoteError::ReservedTimestamp)));
        assert_eq!(total(&m, &B), Some(10));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // A bound note applies, and the edge stays bound-only.
        add(&mut m, &B, 20, at(2997), Some(ack(&A, &B, b0(), at(2997), Some(10)))).unwrap();
        assert_eq!(total(&m, &B), Some(20));
        assert!(bound_only(&m, &B));
        assert!(matches!(add(&mut m, &B, 30, at(3000), None), Err(NoteError::BoundNoteRequired)));
        // The other edge still takes ordinary notes.
        add(&mut m, &C, 9, at(3000), None).unwrap();
        assert_eq!(total(&m, &C), Some(9));
    }

    #[test]
    fn bound_only_marks_survive_reopening_the_store() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            add(&mut m, &B, 10, at(1000), None).unwrap();
            add(&mut m, &C, 4, at(1001), None).unwrap();
            mark_bound_only(&m, &B);
        }
        let mut m = TrackerStateManager::new(dir.path());
        assert!(bound_only(&m, &B) && !bound_only(&m, &C));
        assert!(matches!(add(&mut m, &B, 20, at(2000), None), Err(NoteError::BoundNoteRequired)));
        add(&mut m, &B, 20, at(1997), Some(ack(&A, &B, b0(), at(1997), Some(10)))).unwrap();
        add(&mut m, &C, 5, at(2000), None).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(20), Some(5)));
    }

    #[test]
    fn update_note_refuses_reserved_residues_and_bound_only_edges() {
        let mut m = setup();
        // update_note takes no prior acknowledgment, so it refuses all of 995-999, 997 included.
        for ts in [1995, 1996, 1997, 1998, 1999] {
            let r = m.update_note(&pk(&A), &note(&B, 20, at(ts)));
            assert!(matches!(r, Err(NoteError::ReservedTimestamp)), "ts {}: {:?}", ts, r);
        }
        assert_eq!(total(&m, &B), Some(10));
        m.update_note(&pk(&A), &note(&B, 20, at(2000))).unwrap();
        assert_eq!(total(&m, &B), Some(20));
        mark_bound_only(&m, &B);
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 30, at(3000))), Err(NoteError::BoundNoteRequired)));
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 30, at(3997))), Err(NoteError::ReservedTimestamp)));
        // The existing refusals keep their priority.
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 30, at(1500))), Err(NoteError::PastTimestamp)));
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 5, at(3000))), Err(NoteError::DebtDecreaseNotPermitted { .. })));
        assert_eq!(total(&m, &B), Some(20));
        // An edge that is not bound-only is unaffected.
        m.update_note(&pk(&A), &note(&C, 3, at(3000))).unwrap();
        assert_eq!(total(&m, &C), Some(3));
    }

    #[test]
    fn the_bound_only_partition_is_created_when_missing() {
        // Data written by an older binary has no bound_only_edges partition: it opens, and no edge is bound-only.
        let dir = tempfile::tempdir().unwrap();
        {
            let keyspace = fjall::Config::new(dir.path()).open().unwrap();
            let _notes = keyspace.open_partition("iou_notes", fjall::PartitionCreateOptions::default()).unwrap();
            assert!(!keyspace.partition_exists("bound_only_edges"));
        }
        let storage = NoteStorage::open(dir.path()).unwrap();
        assert!(!storage.is_bound_only(&key(&B)).unwrap());
        storage.commit_note_batch(&[], &[key(&B)]).unwrap();
        assert!(storage.is_bound_only(&key(&B)).unwrap());
        assert!(!storage.is_bound_only(&key(&C)).unwrap());
        drop(storage);
        let storage = NoteStorage::open(dir.path()).unwrap();
        assert!(storage.is_bound_only(&key(&B)).unwrap() && !storage.is_bound_only(&key(&C)).unwrap());
    }

    #[test]
    fn a_bound_note_older_than_the_maximum_age_is_refused_and_a_fresh_one_accepted() {
        // Tracker v3.1 (design decision D-1): a bound note is a payment attempt and expires like a novation, so an
        // unapplied one is dead an hour after its timestamp. A owes B 10, noted two hours ago: an expired bound
        // note's timestamp is still newer than the edge's.
        let old = (now_ms() - 2 * NOVATION_MAX_AGE_MS) / 1000 * 1000;
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &B, 10, old, None).unwrap();
        let digest = m.get_state().avl_root_digest;
        let expired = at_residue(now_ms() - NOVATION_MAX_AGE_MS - 1_000, 997);
        assert!(expired > old);
        // With the issuer's acknowledgment of the stored total, which would apply a fresh note.
        let r = add(&mut m, &B, 15, expired, Some(ack(&A, &B, (10, old), expired, Some(10))));
        assert!(matches!(r, Err(NoteError::NovationExpired)), "{:?}", r);
        // The age is the first bound-note rule: before a stale prior_total, a wrong acknowledgment, no increase.
        assert!(matches!(add(&mut m, &B, 15, expired, Some(ack(&A, &B, (9, old), expired, Some(9)))), Err(NoteError::NovationExpired)));
        assert!(matches!(add(&mut m, &B, 15, expired, Some(ack(&B, &B, (10, old), expired, None))), Err(NoteError::NovationExpired)));
        assert!(matches!(add(&mut m, &B, 10, expired, Some(ack(&A, &B, (10, old), expired, Some(10)))), Err(NoteError::NovationExpired)));
        // ...and after the prior requirement: without a prior the residue is refused as reserved, at any age.
        assert!(matches!(add(&mut m, &B, 15, expired, None), Err(NoteError::ReservedTimestamp)));
        // The existing checks still come first: a bound note older than the edge's note is a past timestamp.
        let older = old - 3; // ends in 997 ms
        assert!(matches!(add(&mut m, &B, 15, older, Some(ack(&A, &B, (10, old), older, Some(10)))), Err(NoteError::PastTimestamp)));
        // An expired first bound note on an absent edge.
        assert!(matches!(add(&mut m, &C, 5, expired, Some(ack(&A, &C, (0, 0), expired, Some(0)))), Err(NoteError::NovationExpired)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Just inside the maximum age the same bound note applies, and a fresh one after it.
        let inside = at_residue(now_ms() - NOVATION_MAX_AGE_MS + 10_000, 997);
        add(&mut m, &B, 15, inside, Some(ack(&A, &B, (10, old), inside, Some(10)))).unwrap();
        let fresh = at_residue(now_ms() - 60_000, 997);
        add(&mut m, &B, 18, fresh, Some(ack(&A, &B, (15, inside), fresh, None))).unwrap();
        add(&mut m, &C, 5, fresh, Some(ack(&A, &C, (0, 0), fresh, Some(0)))).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(18), Some(5)));
        // Ordinary notes have no maximum age: A -> B's own note above was two hours old.
    }
}

/// Pure-credit mode and fail-closed loading (tracker v3, design §3.7): undecodable note records are counted
/// instead of dropped silently, a failed confirmation load is reported and makes the store refuse novations,
/// and the pure-credit startup check refuses a store with a stored on-chain total, an undecodable record or a
/// failed load.
#[cfg(test)]
mod pure_credit_startup_tests {
    use crate::persistence::NoteRecordScan;
    use crate::{
        DenomKey, IouNote, NoteConfirmation, NoteError, PureCreditRefusal, SetOffEdge, StoreLoadReport,
        TrackerStateManager, TransferSignatures, TransferTotals, NOVATIONS_DISABLED_CONFIRMATIONS,
    };
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    /// Keys of planted records that do not decode.
    const BAD_DENOMINATION_KEY: [u8; 32] = [0x77; 32];
    const SHORT_RECORD_KEY: [u8; 32] = [0x78; 32];
    const CORRUPT_CONFIRMATION_KEY: [u8; 32] = [0x79; 32];

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A whole second ten minutes before this module's first test started. Every timestamp here is an offset of
    /// a few seconds from it: in the past, and well inside the maximum novation age.
    fn base() -> u64 {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| (now_ms() - 600_000) / 1000 * 1000)
    }

    fn pk(secret: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    /// `signer`'s signature over the ERG note message (issuer -> recipient, `total`, `ts`).
    fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &DenomKey::Erg, total, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
    }

    fn note(m: &mut TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) {
        let n = IouNote::create_and_sign(pk(recipient), total, ts, issuer, &NFT, DenomKey::Erg).unwrap();
        m.add_note(&pk(issuer), &n, None).unwrap();
    }

    fn total(m: &TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32]) -> u64 {
        m.lookup_note(&pk(issuer), &pk(recipient), &DenomKey::Erg).unwrap().amount_collected
    }

    /// A owes B 100, B owes A 100 (a two-edge cycle) and A owes C 50.
    fn seed(m: &mut TrackerStateManager) {
        note(m, &A, &B, 100, base() + 1000);
        note(m, &B, &A, 100, base() + 1001);
        note(m, &A, &C, 50, base() + 1002);
    }

    /// A set-off of the cycle A->B (stored total `ab`) and B->A (`ba`) by `d`, both edges signed by both parties
    /// at `ts` (residue 998).
    fn setoff_edges(ab: u64, ba: u64, d: u64, ts: u64) -> Vec<SetOffEdge> {
        [(A, B, ab), (B, A, ba)]
            .iter()
            .map(|(issuer, recipient, before)| SetOffEdge {
                issuer_pubkey: pk(issuer),
                recipient_pubkey: pk(recipient),
                decrease: d,
                timestamp: ts,
                issuer_signature: sign(issuer, issuer, recipient, before - d, ts),
                recipient_signature: sign(recipient, issuer, recipient, before - d, ts),
            })
            .collect()
    }

    /// A transfer (version 4) of `amount` of A's debt from A->B (stored total `x`) to A->C (`y`) at `ts` (residue
    /// 999), with all seven signatures; the prior acknowledgments cover the edges' stored timestamps (0 if absent).
    fn transfer(m: &mut TrackerStateManager, x: u64, y: u64, amount: u64, ts: u64) -> Result<TransferTotals, NoteError> {
        let stamp = |m: &TrackerStateManager, r: &[u8; 32]| {
            m.get_note_optional(&pk(&A), &pk(r), &DenomKey::Erg).ok().flatten().map(|n| n.timestamp).unwrap_or(0)
        };
        let ack = |signer: &[u8; 32], r: &[u8; 32], prior: u64, prior_ts: u64| {
            let msg = crate::schnorr::prior_ack_message(&pk(&A), &pk(r), &DenomKey::Erg, prior, prior_ts, ts, &NFT);
            crate::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
        };
        let (p, q) = (stamp(m, &B), stamp(m, &C));
        let s = TransferSignatures {
            issuer_from: sign(&A, &A, &B, x - amount, ts),
            issuer_to: sign(&A, &A, &C, y + amount, ts),
            from_recipient_prior: ack(&B, &B, x, p),
            from_recipient: sign(&B, &A, &B, x - amount, ts),
            from_recipient_to: sign(&B, &A, &C, y + amount, ts),
            to_recipient_prior: ack(&C, &C, y, q),
            to_recipient: sign(&C, &A, &C, y + amount, ts),
        };
        m.transfer_debt(&pk(&A), &pk(&B), &pk(&C), &DenomKey::Erg, amount, ts, &s, None, None, None, None)
    }

    #[test]
    fn undecodable_note_records_are_counted_not_dropped_silently() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            // A record of the right length with an unknown denomination byte, and a record of the wrong length.
            let mut bad_denomination = vec![0u8; 220];
            bad_denomination[187] = 0x02;
            m.storage.insert_raw_note_record(&BAD_DENOMINATION_KEY, &bad_denomination);
            m.storage.insert_raw_note_record(&SHORT_RECORD_KEY, &[0u8; 10]);

            assert_eq!(m.storage.rebuild_indices().unwrap(), NoteRecordScan { decoded: 3, skipped: 2 });
            let (notes, skipped) = m.storage.get_all_notes_with_issuer_counted().unwrap();
            assert_eq!((notes.len(), skipped), (3, 2));
            // The other readers still skip them, as before.
            assert_eq!(m.get_all_notes_with_issuer().unwrap().len(), 3);
        }
        // Reopened, the store records the count, and it serves the notes that decode as before.
        let m = TrackerStateManager::new(dir.path());
        assert_eq!(
            m.load_report(),
            &StoreLoadReport { undecodable_note_records: 2, load_errors: vec![], confirmations_error: None }
        );
        assert_eq!(m.get_all_notes_with_issuer().unwrap().len(), 3);
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A), total(&m, &A, &C)), (100, 100, 50));
    }

    #[test]
    fn a_pure_credit_store_passes_the_check_also_after_novations() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            assert_eq!(m.check_pure_credit(), Ok(()));
            transfer(&mut m, 100, 50, 20, base() + 2999).unwrap();
            m.set_off(&DenomKey::Erg, &setoff_edges(80, 100, 30, base() + 3998)).unwrap();
            assert_eq!(m.check_pure_credit(), Ok(()));
        }
        let m = TrackerStateManager::new(dir.path());
        assert_eq!(m.load_report(), &StoreLoadReport::default());
        assert_eq!(m.check_pure_credit(), Ok(()));
        assert_eq!((m.onchain_edge_count(), m.novations_disabled_reason()), (0, None));
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A), total(&m, &A, &C)), (50, 70, 70));
    }

    #[test]
    fn pure_credit_refuses_a_stored_committed_total() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            assert_eq!(m.onchain_edge_count(), 0);
            // A tracker box committed the current tree on-chain: every edge now has a confirmed total.
            let digest = m.get_state().avl_root_digest;
            assert_eq!(m.reconcile_with_confirmed_digest(&digest, "box1", 100).unwrap(), 3);
            assert_eq!(m.onchain_edge_count(), 3);
        }
        let m = TrackerStateManager::new(dir.path());
        let refusal = m.check_pure_credit().unwrap_err();
        assert_eq!(refusal, PureCreditRefusal { onchain_confirmations: 3, undecodable_note_records: 0, load_errors: vec![] });
        let message = refusal.to_string();
        assert!(
            message.starts_with(
                "refusing to start in pure_credit mode: 3 stored confirmation record(s) carry a committed or pending on-chain total."
            ),
            "{message}"
        );
        // Outside pure-credit mode the store opens as before: its on-chain edges are counted, and novations are not
        // disabled (the scope check refuses those edges one by one).
        assert_eq!(m.load_report(), &StoreLoadReport::default());
        assert_eq!((m.onchain_edge_count(), m.novations_disabled_reason()), (3, None));
    }

    #[test]
    fn pure_credit_refuses_a_stored_pending_total_although_a_restart_clears_it_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            note(&mut m, &A, &B, 100, base() + 1000);
            // A tracker box update covering the edge was submitted and has not confirmed.
            let digest = m.get_state().avl_root_digest;
            assert_eq!(m.mark_notes_pending(digest, "tx1", 100).unwrap(), 1);
            assert_eq!(m.onchain_edge_count(), 1);
        }
        let m = TrackerStateManager::new(dir.path());
        // The restart clears the pending total in memory; the stored record keeps it, and the transaction may still
        // confirm.
        assert_eq!(m.onchain_edge_count(), 0);
        assert_eq!(m.check_pure_credit(), Err(PureCreditRefusal { onchain_confirmations: 1, ..Default::default() }));
    }

    #[test]
    fn pure_credit_refuses_an_undecodable_note_record() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            assert_eq!(m.check_pure_credit(), Ok(()));
            m.storage.insert_raw_note_record(&SHORT_RECORD_KEY, &[0u8; 10]);
            // The check reads the records again, so it refuses at once, before any restart.
            assert_eq!(m.check_pure_credit(), Err(PureCreditRefusal { undecodable_note_records: 1, ..Default::default() }));
        }
        let m = TrackerStateManager::new(dir.path());
        assert_eq!(m.load_report().undecodable_note_records, 1);
        let refusal = m.check_pure_credit().unwrap_err();
        assert_eq!(refusal, PureCreditRefusal { undecodable_note_records: 1, ..Default::default() });
        assert!(
            refusal
                .to_string()
                .starts_with("refusing to start in pure_credit mode: 1 note record(s) could not be decoded."),
            "{refusal}"
        );
        // Outside pure-credit mode the notes that decode are served and novations are not disabled.
        assert_eq!((m.novations_disabled_reason(), total(&m, &A, &B)), (None, 100));
    }

    #[test]
    fn a_confirmation_load_failure_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            m.storage.insert_raw_confirmation_record(&CORRUPT_CONFIRMATION_KEY, b"not json");
        }
        let mut m = TrackerStateManager::new(dir.path());
        let digest = m.get_state().avl_root_digest;

        // rebuild_confirmations returns the error (it used to log it and continue); the failure is recorded.
        let recorded = m.load_report().confirmations_error.clone().expect("the failure is recorded");
        assert!(
            recorded.starts_with("confirmation rebuild: StorageError(\"Failed to parse confirmation"),
            "{recorded}"
        );
        assert!(matches!(m.rebuild_confirmations(), Err(NoteError::StorageError(_))));
        assert_eq!(m.novations_disabled_reason().as_deref(), Some(NOVATIONS_DISABLED_CONFIRMATIONS));

        // A pure-credit tracker refuses to start on it.
        let refusal = m.check_pure_credit().unwrap_err();
        assert_eq!((refusal.onchain_confirmations, refusal.undecodable_note_records), (0, 0));
        assert!(refusal.load_errors.iter().any(|e| e.starts_with("confirmation rebuild: ")), "{refusal:?}");
        assert!(refusal.to_string().contains("loading failed (confirmation rebuild: "), "{refusal}");

        // Any other tracker refuses every novation: it cannot tell which edges carry on-chain state.
        assert!(matches!(
            transfer(&mut m, 100, 50, 20, base() + 2999),
            Err(NoteError::OnchainScope(msg)) if msg == NOVATIONS_DISABLED_CONFIRMATIONS
        ));
        assert!(matches!(
            m.set_off(&DenomKey::Erg, &setoff_edges(100, 100, 30, base() + 3998)),
            Err(NoteError::OnchainScope(msg)) if msg == NOVATIONS_DISABLED_CONFIRMATIONS
        ));
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A), total(&m, &A, &C)), (100, 100, 50));
        assert_eq!(m.get_state().avl_root_digest, digest);
        assert!(!m.is_bound_only(&pk(&A), &pk(&B), &DenomKey::Erg).unwrap());
        // Ordinary notes still apply.
        note(&mut m, &A, &C, 60, base() + 1500);
        assert_eq!(total(&m, &A, &C), 60);

        // Once the record is readable again, a rebuild succeeds, the failure is cleared and novations apply.
        m.storage.store_confirmation(&CORRUPT_CONFIRMATION_KEY, &NoteConfirmation::local_only()).unwrap();
        assert_eq!(m.rebuild_confirmations().unwrap(), 3);
        assert_eq!((m.load_report().confirmations_error.clone(), m.novations_disabled_reason()), (None, None));
        assert_eq!(m.check_pure_credit(), Ok(()));
        m.set_off(&DenomKey::Erg, &setoff_edges(100, 100, 30, base() + 3998)).unwrap();
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (70, 70));
    }

    /// The key of the confirmation record of the edge issuer -> recipient in `denomination`.
    fn record_key(issuer: &[u8; 32], recipient: &[u8; 32], denomination: &DenomKey) -> [u8; 32] {
        crate::NoteKey::from_assets(&pk(issuer), &pk(recipient), denomination).key_hash
    }

    /// The raw stored confirmation records of `keys` (`None` where there is none).
    fn stored_records(m: &TrackerStateManager, keys: &[[u8; 32]]) -> Vec<Option<Vec<u8>>> {
        keys.iter().map(|k| m.storage.raw_confirmation_record(k)).collect()
    }

    #[test]
    fn stored_confirmation_evidence_is_not_written_back_while_the_records_failed_to_load() {
        // Tracker v3.1. Outside pure_credit a store whose confirmation records fail to load still opens, with
        // novations refused. Its in-memory records are then built without the stored ones (every note local-only),
        // so writing one back would replace stored evidence of on-chain state with a readable local-only record:
        // after a restart the load would succeed and a committed edge would look pure-credit. Here a tracker box
        // committed A->B 100, B->A 100 and A->C 50 (each has a stored confirmed total), and then A->C's own record
        // was damaged.
        let dir = tempfile::tempdir().unwrap();
        let erg = DenomKey::Erg;
        let (ab, ba, ac, cb) = (record_key(&A, &B, &erg), record_key(&B, &A, &erg), record_key(&A, &C, &erg), record_key(&C, &B, &erg));
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            let digest = m.get_state().avl_root_digest;
            assert_eq!(m.reconcile_with_confirmed_digest(&digest, "box1", 100).unwrap(), 3);
            m.storage.insert_raw_confirmation_record(&ac, b"not json");
        }
        let mut m = TrackerStateManager::new(dir.path());
        assert!(m.load_report().confirmations_error.is_some());
        let before = stored_records(&m, &[ab, ba, ac, cb]);
        let committed: NoteConfirmation = serde_json::from_slice(before[0].as_ref().unwrap()).unwrap();
        assert_eq!(committed.confirmed_total_debt, Some(100), "A->B's stored record is the committed one");
        assert_eq!(before[2].as_deref(), Some(&b"not json"[..]));
        assert_eq!(before[3], None, "C->B is not an edge yet");

        // add_note on the committed edge, on the edge whose record is corrupt, and on a new edge: the notes apply,
        // and no stored record is written (the new edge's record exists in memory only).
        note(&mut m, &A, &B, 110, base() + 1500);
        note(&mut m, &A, &C, 60, base() + 1501);
        note(&mut m, &C, &B, 5, base() + 1502);
        assert_eq!((total(&m, &A, &B), total(&m, &A, &C), total(&m, &C, &B)), (110, 60, 5));
        assert_eq!(stored_records(&m, &[ab, ba, ac, cb]), before);

        // The tracker box updater's write-backs update the in-memory records only: pending, reverted, pending
        // again, confirmed, and a reconciliation after a further note.
        let digest = m.get_state().avl_root_digest;
        assert_eq!(m.mark_notes_pending(digest, "tx1", 101).unwrap(), 4);
        assert_eq!(m.revert_pending_notes().unwrap(), 4);
        assert_eq!(m.mark_notes_pending(digest, "tx2", 102).unwrap(), 4);
        assert_eq!(m.confirm_pending_notes(digest, "box2", 103).unwrap(), 4);
        assert_eq!(m.onchain_edge_count(), 4);
        note(&mut m, &A, &B, 120, base() + 1503);
        let digest = m.get_state().avl_root_digest;
        assert_eq!(m.reconcile_with_confirmed_digest(&digest, "box3", 104).unwrap(), 1);
        assert_eq!(stored_records(&m, &[ab, ba, ac, cb]), before);

        // Novations stay refused, and their refusal writes nothing either.
        assert!(matches!(transfer(&mut m, 120, 60, 20, base() + 2999), Err(NoteError::OnchainScope(_))));
        assert!(matches!(m.set_off(&erg, &setoff_edges(120, 100, 30, base() + 3998)), Err(NoteError::OnchainScope(_))));
        assert_eq!(stored_records(&m, &[ab, ba, ac, cb]), before);

        // After a restart all the evidence is still there: the load fails again, novations stay refused, and a
        // pure-credit tracker still refuses to start (two stored committed totals and the unreadable record).
        drop(m);
        let mut m = TrackerStateManager::new(dir.path());
        assert!(m.load_report().confirmations_error.is_some());
        assert_eq!(m.novations_disabled_reason().as_deref(), Some(NOVATIONS_DISABLED_CONFIRMATIONS));
        let refusal = m.check_pure_credit().unwrap_err();
        assert!(refusal.load_errors.iter().any(|e| e.starts_with("confirmation records: ")), "{refusal:?}");
        assert!(matches!(
            m.set_off(&erg, &setoff_edges(120, 100, 30, base() + 3998)),
            Err(NoteError::OnchainScope(msg)) if msg == NOVATIONS_DISABLED_CONFIRMATIONS
        ));
        assert_eq!(stored_records(&m, &[ab, ba, ac, cb]), before);
        assert_eq!((total(&m, &A, &B), total(&m, &A, &C), total(&m, &C, &B)), (120, 60, 5));

        // Once the damaged record is readable again (repaired by hand), the rebuild succeeds and the write-backs
        // resume: A->B's committed record still holds its confirmed total, so the edge is still on-chain scope.
        m.storage.store_confirmation(&ac, &NoteConfirmation::local_only()).unwrap();
        assert_eq!(m.rebuild_confirmations().unwrap(), 4);
        assert_eq!(m.get_confirmation(&pk(&A), &pk(&B), &erg).unwrap().confirmed_total_debt, Some(100));
        assert!(matches!(transfer(&mut m, 120, 60, 20, base() + 2999), Err(NoteError::OnchainScope(msg)) if msg != NOVATIONS_DISABLED_CONFIRMATIONS));
        note(&mut m, &C, &B, 6, base() + 1504);
        assert!(m.storage.raw_confirmation_record(&cb).is_some(), "written back again once the records load");
    }

    #[test]
    fn an_exchange_does_not_write_confirmation_records_back_while_they_failed_to_load() {
        // Both phases of a cross-denomination exchange write the confirmation record of the edge they change. While
        // the stored records failed to load, neither does: the committed records of both edges stay as they are.
        let dir = tempfile::tempdir().unwrap();
        let (erg, token) = (DenomKey::Erg, DenomKey::Token([0xa5; 32]));
        let (from_key, to_key) = (record_key(&A, &B, &erg), record_key(&A, &B, &token));
        {
            let mut m = TrackerStateManager::new(dir.path());
            seed(&mut m);
            let n = IouNote::create_and_sign(pk(&B), 40, base() + 1003, &A, &NFT, token).unwrap();
            m.add_note(&pk(&A), &n, None).unwrap();
            let digest = m.get_state().avl_root_digest;
            assert_eq!(m.reconcile_with_confirmed_digest(&digest, "box1", 100).unwrap(), 4);
            m.storage.insert_raw_confirmation_record(&CORRUPT_CONFIRMATION_KEY, b"not json");
        }
        let mut m = TrackerStateManager::new(dir.path());
        assert!(m.load_report().confirmations_error.is_some());
        let before = stored_records(&m, &[from_key, to_key, CORRUPT_CONFIRMATION_KEY]);
        assert!(before.iter().all(Option::is_some));

        // Phase 1: A->B ERG falls from 100 to 90; the token increase (40 -> 50) is staged.
        let xts = base() + 1500;
        let sig = |secret: &[u8; 32], denomination: &DenomKey, total: u64| {
            let msg = crate::schnorr::signing_message(&pk(&A), &pk(&B), denomination, total, xts, &NFT);
            crate::schnorr::schnorr_sign(&msg, secret, &pk(secret)).unwrap()
        };
        m.exchange_debt(
            &pk(&A), &pk(&B), &erg, 10, &token, 10, xts,
            &sig(&A, &erg, 90), &sig(&B, &erg, 90), &sig(&A, &token, 50), &sig(&B, &token, 50),
            "reserve_box",
        )
        .unwrap();
        assert_eq!(total(&m, &A, &B), 90);
        assert_eq!(stored_records(&m, &[from_key, to_key, CORRUPT_CONFIRMATION_KEY]), before);

        // Phase 2, once a tracker box commits the decrease (in memory) and the reserve is intact: the token
        // increase applies, and its record is not written back either.
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 101).unwrap();
        m.confirm_pending_notes(digest, "box2", 102).unwrap();
        let staged = m.list_pending_exchanges(None, None).unwrap();
        assert_eq!(staged.len(), 1);
        let observation = crate::ExchangeObservation {
            key: staged[0].0,
            reserve_r5_digest: Some(m.reserve_state_digest(&pk(&A), &erg).try_into().unwrap()),
        };
        let processed = m.process_staged_exchanges(&[observation]).unwrap();
        assert_eq!((processed.applied, processed.cancelled), (1, 0));
        assert_eq!(m.lookup_note(&pk(&A), &pk(&B), &token).unwrap().amount_collected, 50);
        assert_eq!(stored_records(&m, &[from_key, to_key, CORRUPT_CONFIRMATION_KEY]), before);
    }
}

/// Tracker v3.2 regression tests for audit VS-1 and readiness R-17 (design §3 (a)-(d)). A prior acknowledgment is
/// PA(I,R,X,p,T): the edge's stored total X AND stored timestamp p, at the operation's timestamp T, in a 105-byte
/// message (tag `basis/prior-ack/1`) that no note, set-off or statement signs. An edge's timestamp strictly increases
/// with every write, so each state (X, p) occurs once and an acknowledgment applies at most once, whatever totals
/// the edge takes later. Every scenario here has honest signers only.
///
/// The helpers between the VERSION markers are the only code that differs when this module is run against tracker
/// v3.1 (24675ec), whose acknowledgments are the 80-byte note messages M(I,R,X,T) of the totals alone and whose
/// transfer takes no prior timestamps: there the stale attempts of (a), (b), (c2) and (d) apply.
#[cfg(test)]
mod prior_ack_regression_tests {
    use crate::{DenomKey, IouNote, NoteError, PriorAck, SetOffEdge, TrackerStateManager, TransferSignatures, TransferTotals};
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    const D: [u8; 32] = [4u8; 32];
    const E: [u8; 32] = [5u8; 32];
    const Z: [u8; 32] = [6u8; 32];

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A whole second ten minutes before this module's first test started: every timestamp is a small offset from
    /// it, in the past and well inside the one-hour maximum age.
    fn base() -> u64 {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| (now_ms() - 600_000) / 1000 * 1000)
    }

    /// `offset` ms after [`base`]: offsets ending in 999 are transfers, 998 set-offs, 997 bound notes, 000-994
    /// ordinary notes.
    fn at(offset: u64) -> u64 {
        base() + offset
    }

    fn pk(secret: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn sign_bytes(signer: &[u8; 32], message: &[u8]) -> [u8; 65] {
        crate::schnorr::schnorr_sign(message, signer, &pk(signer)).unwrap()
    }

    /// `signer`'s signature over the 80-byte note message M(issuer, recipient, total, ts).
    fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> [u8; 65] {
        sign_bytes(signer, &crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &DenomKey::Erg, total, ts, &NFT))
    }

    fn note(issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> IouNote {
        IouNote::create_and_sign(pk(recipient), total, ts, issuer, &NFT, DenomKey::Erg).unwrap()
    }

    /// An ordinary note.
    fn add(m: &mut TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) {
        m.add_note(&pk(issuer), &note(issuer, recipient, total, ts), None).unwrap();
    }

    /// The stored state (total, timestamp) of issuer -> recipient, (0, 0) when the edge is absent.
    fn state(m: &TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32]) -> (u64, u64) {
        m.get_note_optional(&pk(issuer), &pk(recipient), &DenomKey::Erg)
            .unwrap()
            .map(|n| (n.amount_collected, n.timestamp))
            .unwrap_or((0, 0))
    }

    // ---- VERSION: tracker v3.2 ------------------------------------------------------------------------------------

    /// The message a prior acknowledgment of issuer -> recipient in the state `prior` (total, timestamp) for the
    /// operation at `ts` is a signature over: PA(issuer, recipient, total, timestamp, ts).
    fn ack_message(issuer: &[u8; 32], recipient: &[u8; 32], prior: (u64, u64), ts: u64) -> Vec<u8> {
        crate::schnorr::prior_ack_message(&pk(issuer), &pk(recipient), &DenomKey::Erg, prior.0, prior.1, ts, &NFT)
    }

    /// A debt transfer without the diagnostic fields (the signatures bind).
    #[allow(clippy::too_many_arguments)]
    fn transfer(
        m: &mut TrackerStateManager, issuer: &[u8; 32], from: &[u8; 32], to: &[u8; 32], amount: u64, ts: u64,
        s: &TransferSignatures,
    ) -> Result<TransferTotals, NoteError> {
        m.transfer_debt(&pk(issuer), &pk(from), &pk(to), &DenomKey::Erg, amount, ts, s, None, None, None, None)
    }

    /// A bound note (residue 997) with `prior_signature` and no diagnostic fields.
    fn bound_note(
        m: &mut TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64, prior_signature: [u8; 65],
    ) -> Result<(), NoteError> {
        let prior = PriorAck { signature: prior_signature, prior_total: None, prior_timestamp: None };
        m.add_note(&pk(issuer), &note(issuer, recipient, total, ts), Some(prior))
    }

    // ---- end VERSION ----------------------------------------------------------------------------------------------

    /// `signer`'s prior acknowledgment of issuer -> recipient in the state `prior` for the operation at `ts`.
    fn ack(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], prior: (u64, u64), ts: u64) -> [u8; 65] {
        sign_bytes(signer, &ack_message(issuer, recipient, prior, ts))
    }

    /// The seven signatures of a transfer of `amount` of `issuer`'s debt from issuer -> from to issuer -> to at
    /// `ts`, each creditor acknowledging its edge in the given state ((0, 0) for an absent edge).
    #[allow(clippy::too_many_arguments)]
    fn sigs(
        issuer: &[u8; 32], from: &[u8; 32], to: &[u8; 32], from_state: (u64, u64), to_state: (u64, u64), amount: u64,
        ts: u64,
    ) -> TransferSignatures {
        let (from_total, to_total) = (from_state.0 - amount, to_state.0 + amount);
        TransferSignatures {
            issuer_from: sign(issuer, issuer, from, from_total, ts),
            issuer_to: sign(issuer, issuer, to, to_total, ts),
            from_recipient_prior: ack(from, issuer, from, from_state, ts),
            from_recipient: sign(from, issuer, from, from_total, ts),
            from_recipient_to: sign(from, issuer, to, to_total, ts),
            to_recipient_prior: ack(to, issuer, to, to_state, ts),
            to_recipient: sign(to, issuer, to, to_total, ts),
        }
    }

    /// A set-off edge issuer -> recipient lowered from `before` by `decrease` at `ts`, signed by both parties.
    fn edge(issuer: &[u8; 32], recipient: &[u8; 32], before: u64, decrease: u64, ts: u64) -> SetOffEdge {
        SetOffEdge {
            issuer_pubkey: pk(issuer),
            recipient_pubkey: pk(recipient),
            decrease,
            timestamp: ts,
            issuer_signature: sign(issuer, issuer, recipient, before - decrease, ts),
            recipient_signature: sign(recipient, issuer, recipient, before - decrease, ts),
        }
    }

    #[test]
    fn a_vs1_a_retry_on_the_same_prior_cannot_apply_after_both_edges_return_to_its_totals() {
        // Spec 18.6 (audit VS-1). A owes B 100 and C 10. B pays C 30 of A's debt: attempt 1 at T1 and, as a retry of
        // the same payment built on the same state of both edges, attempt 2 at T2 > T1. Attempt 1 applies. Between
        // T1 and T2, A raises A->B back to 100 (a bound note: A owes B 30 more, for something else) and C moves 30
        // out of A->C (C pays D 30 of A's debt), so both edges are back at the totals both attempts acknowledged;
        // none of this needs B's signature. Attempt 2 must not apply: B would pay C twice.
        let mut m = TrackerStateManager::new_with_temp_storage();
        let (pb, pc) = (at(1000), at(1001));
        add(&mut m, &A, &B, 100, pb);
        add(&mut m, &A, &C, 10, pc);
        let (t1, t2) = (at(2999), at(5999));
        let attempt1 = sigs(&A, &B, &C, (100, pb), (10, pc), 30, t1);
        let attempt2 = sigs(&A, &B, &C, (100, pb), (10, pc), 30, t2);
        transfer(&mut m, &A, &B, &C, 30, t1, &attempt1).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((70, t1), (40, t1)));
        // A->B is bound-only after the transfer: A raises it back to 100 with a bound note acknowledging (70, T1).
        let tb = at(3997);
        bound_note(&mut m, &A, &B, 100, tb, ack(&A, &A, &B, (70, t1), tb)).unwrap();
        // C pays D 30 of A's debt: A->C is back at 10.
        let tc = at(4999);
        transfer(&mut m, &A, &C, &D, 30, tc, &sigs(&A, &C, &D, (40, t1), (0, 0), 30, tc)).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((100, tb), (10, tc)), "both totals are back");
        let digest = m.get_state().avl_root_digest;
        // Attempt 2 is refused and changes nothing.
        let r = transfer(&mut m, &A, &B, &C, 30, t2, &attempt2);
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "attempt 2 must not apply (B would pay C twice): {:?}", r);
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((100, tb), (10, tc)));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Each acknowledgment alone refuses it: with either one re-signed over the current state, still refused.
        let only_from = TransferSignatures { from_recipient_prior: ack(&B, &A, &B, (100, tb), t2), ..attempt2 };
        assert!(matches!(transfer(&mut m, &A, &B, &C, 30, t2, &only_from), Err(NoteError::InvalidSignature)));
        let only_to = TransferSignatures { to_recipient_prior: ack(&C, &A, &C, (10, tc), t2), ..attempt2 };
        assert!(matches!(transfer(&mut m, &A, &B, &C, 30, t2, &only_to), Err(NoteError::InvalidSignature)));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Control: only the acknowledgments refused it. A new attempt acknowledging the current states (B and C
        // sign again, knowing them) applies at the same T2.
        transfer(&mut m, &A, &B, &C, 30, t2, &sigs(&A, &B, &C, (100, tb), (10, tc), 30, t2)).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((70, t2), (40, t2)));
    }

    #[test]
    fn b_two_bound_notes_on_the_same_prior_exclude_each_other_after_the_edge_returns_to_it() {
        // Readiness R-17 (the issuer-side twin). A owes B 100. A pays B 30 more by a bound note at ts1 and, as a
        // retry of the same payment, by a second bound note at ts2 > ts1, both acknowledging A->B as stored then.
        // The first applies (130). A novation then lowers A->B back to 100 between ts1 and ts2: a transfer (B pays E
        // 30 of A's debt) or a set-off (B owes A 30, cleared against A->B). The second bound note must not apply:
        // A would pay B twice.
        for lowered_by in ["a transfer", "a set-off"] {
            let mut m = TrackerStateManager::new_with_temp_storage();
            let p = at(1000);
            add(&mut m, &A, &B, 100, p);
            add(&mut m, &B, &A, 30, at(1001));
            let (ts1, ts2) = (at(2997), at(4997));
            let (first, retry) = (ack(&A, &A, &B, (100, p), ts1), ack(&A, &A, &B, (100, p), ts2));
            bound_note(&mut m, &A, &B, 130, ts1, first).unwrap();
            assert_eq!(state(&m, &A, &B), (130, ts1));
            let tx = if lowered_by == "a transfer" {
                let tx = at(3999);
                transfer(&mut m, &A, &B, &E, 30, tx, &sigs(&A, &B, &E, (130, ts1), (0, 0), 30, tx)).unwrap();
                tx
            } else {
                let tx = at(3998);
                m.set_off(&DenomKey::Erg, &[edge(&A, &B, 130, 30, tx), edge(&B, &A, 30, 30, tx)]).unwrap();
                tx
            };
            assert_eq!(state(&m, &A, &B), (100, tx), "{}: A->B is back at the acknowledged total", lowered_by);
            let digest = m.get_state().avl_root_digest;
            let r = bound_note(&mut m, &A, &B, 130, ts2, retry);
            assert!(
                matches!(r, Err(NoteError::InvalidPriorSignature)),
                "{}: the second bound note must not apply (A would pay B twice): {:?}",
                lowered_by,
                r
            );
            assert_eq!(state(&m, &A, &B), (100, tx));
            assert_eq!(m.get_state().avl_root_digest, digest);
            // Control: a bound note acknowledging the current state (100, tx) applies at ts2.
            bound_note(&mut m, &A, &B, 130, ts2, ack(&A, &A, &B, (100, tx), ts2)).unwrap();
            assert_eq!(state(&m, &A, &B), (130, ts2));
        }
    }

    #[test]
    fn c1_an_acknowledgment_lifted_from_another_role_is_refused() {
        // Design §1: acknowledging the prior note's own message M(A,B,X,p) would be the obvious fix, and it is wrong:
        // B signs exactly that message in other roles. A->B reaches its state by a transfer into it (Z pays B 40 of
        // A's debt at T0: B signs M(A,B,40,T0) as the new creditor) or by a set-off (B signs M(A,B,X,p) as the
        // creditor of the lowered edge). Neither signature, nor B's acknowledgment of the same state for another
        // attempt (another T), is B's prior acknowledgment of a transfer out of A->B at T.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &Z, 50, at(1000));
        add(&mut m, &A, &C, 10, at(1001));
        let t0 = at(1999);
        let into = sigs(&A, &Z, &B, (50, at(1000)), (0, 0), 40, t0);
        transfer(&mut m, &A, &Z, &B, 40, t0, &into).unwrap();
        assert_eq!(state(&m, &A, &B), (40, t0));
        let t = at(2999);
        let honest = sigs(&A, &B, &C, (40, t0), (10, at(1001)), 15, t);
        let digest = m.get_state().avl_root_digest;
        for (what, lifted) in [
            ("B's to_recipient signature of the transfer into A->B, M(A,B,40,T0)", into.to_recipient),
            ("B's signature over M(A,B,X,p) = M(A,B,40,T0)", sign(&B, &A, &B, 40, t0)),
            ("B's acknowledgment of the same state for an attempt at another T", ack(&B, &A, &B, (40, t0), t - 1000)),
            ("B's acknowledgment of the to-edge's state", ack(&B, &A, &C, (10, at(1001)), t)),
            ("A's acknowledgment of the same state (an issuer's, as for a bound note)", ack(&A, &A, &B, (40, t0), t)),
        ] {
            let s = TransferSignatures { from_recipient_prior: lifted, ..honest };
            let r = transfer(&mut m, &A, &B, &C, 15, t, &s);
            assert!(matches!(r, Err(NoteError::InvalidSignature)), "{}: {:?}", what, r);
        }
        assert_eq!(m.get_state().avl_root_digest, digest);
        // The same from a set-off: B owes A 5; a set-off lowers A->B and B->A by 5 at S, B signing M(A,B,35,S) as
        // the creditor of A->B, which is M(A,B,X,p) of A->B's new state.
        add(&mut m, &B, &A, 5, at(2000));
        let s_ts = at(2998);
        let setoff = [edge(&A, &B, 40, 5, s_ts), edge(&B, &A, 5, 5, s_ts)];
        m.set_off(&DenomKey::Erg, &setoff).unwrap();
        assert_eq!(state(&m, &A, &B), (35, s_ts));
        let honest = sigs(&A, &B, &C, (35, s_ts), (10, at(1001)), 15, t);
        let lifted = TransferSignatures { from_recipient_prior: setoff[0].recipient_signature, ..honest };
        let r = transfer(&mut m, &A, &B, &C, 15, t, &lifted);
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "B's set-off signature M(A,B,35,S): {:?}", r);
        // Control: the honest acknowledgment applies.
        transfer(&mut m, &A, &B, &C, 15, t, &honest).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((20, t), (25, t)));
    }

    #[test]
    fn c2_an_80_byte_note_message_is_never_an_acknowledgment() {
        // Tracker v3.1's acknowledgments were the note messages of the prior totals at the operation's timestamp:
        // M(A,B,X,T) and M(A,C,Y,T) for a transfer, M(I,R,P,ts) for a bound note. Version 4 transfers and version
        // 3 bound notes refuse them (the honest ones apply).
        let mut m = TrackerStateManager::new_with_temp_storage();
        let (pb, pc) = (at(1000), at(1001));
        add(&mut m, &A, &B, 100, pb);
        add(&mut m, &A, &C, 10, pc);
        let t = at(2999);
        let honest = sigs(&A, &B, &C, (100, pb), (10, pc), 30, t);
        let digest = m.get_state().avl_root_digest;
        let old_from = TransferSignatures { from_recipient_prior: sign(&B, &A, &B, 100, t), ..honest };
        assert!(matches!(transfer(&mut m, &A, &B, &C, 30, t, &old_from), Err(NoteError::InvalidSignature)), "M(A,B,X,T)");
        let old_to = TransferSignatures { to_recipient_prior: sign(&C, &A, &C, 10, t), ..honest };
        assert!(matches!(transfer(&mut m, &A, &B, &C, 30, t, &old_to), Err(NoteError::InvalidSignature)), "M(A,C,Y,T)");
        let ts = at(1997);
        let r = bound_note(&mut m, &A, &C, 15, ts, sign(&A, &A, &C, 10, ts));
        assert!(matches!(r, Err(NoteError::InvalidPriorSignature)), "M(I,R,P,ts): {:?}", r);
        assert_eq!(m.get_state().avl_root_digest, digest);
        bound_note(&mut m, &A, &C, 15, ts, ack(&A, &A, &C, (10, pc), ts)).unwrap();
        transfer(&mut m, &A, &B, &C, 30, t, &sigs(&A, &B, &C, (100, pb), (15, ts), 30, t)).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((70, t), (45, t)));
    }

    #[test]
    fn c3_the_new_creditor_role_signs_the_same_acknowledgment_but_gives_no_consent_to_pay() {
        // PA binds an edge's state and T, not the signer's role: B acknowledging A->B at (X, p) for T is the same
        // message whether B is the new creditor of a transfer into A->B at T or the old creditor of a transfer out
        // of A->B at T. That gives nothing away: paying out of A->B also needs B's consent to the decrease
        // (from_recipient, M(A,B,X-a,T)) and to the destination (from_recipient_to), which a transfer into A->B
        // never contains; and both transfers move A->B to T, so at most one of them can ever apply.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &B, 40, at(1000));
        add(&mut m, &A, &Z, 50, at(1001));
        let t = at(2999);
        let into = sigs(&A, &Z, &B, (50, at(1001)), (40, at(1000)), 20, t); // Z pays B 20: A->B 40 -> 60
        let out = sigs(&A, &B, &C, (40, at(1000)), (0, 0), 15, t); // B pays C 15: A->B 40 -> 25
        // B's new-creditor acknowledgment of A->B is a valid old-creditor acknowledgment of A->B at T.
        let message = ack_message(&A, &B, (40, at(1000)), t);
        assert!(crate::schnorr::schnorr_verify(&into.to_recipient_prior, &message, &pk(&B)).is_ok());
        assert!(crate::schnorr::schnorr_verify(&out.from_recipient_prior, &message, &pk(&B)).is_ok());
        let digest = m.get_state().avl_root_digest;
        // With B's acknowledgment taken from the transfer into A->B, and every signature of B's that the transfer
        // into A->B holds, the payment out of A->B still lacks B's consent.
        for (what, from_recipient, from_recipient_to) in [
            ("without B's consent", out.issuer_from, out.issuer_to),
            ("B's new-creditor signature as consent", into.to_recipient, into.to_recipient),
        ] {
            let s = TransferSignatures {
                from_recipient_prior: into.to_recipient_prior,
                from_recipient,
                from_recipient_to,
                ..out
            };
            assert!(matches!(transfer(&mut m, &A, &B, &C, 15, t, &s), Err(NoteError::InvalidSignature)), "{}", what);
        }
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Whichever applies first, the other is dead: here the transfer into A->B.
        transfer(&mut m, &A, &Z, &B, 20, t, &into).unwrap();
        assert_eq!(state(&m, &A, &B), (60, t));
        assert!(transfer(&mut m, &A, &B, &C, 15, t, &out).is_err());
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((60, t), (0, 0)));
    }

    #[test]
    fn d1_acknowledging_zero_zero_works_for_an_absent_to_edge_and_nothing_else_does() {
        // An absent edge is acknowledged as (0, 0) (a stored note can be (0, 0) too, which is economically identical
        // to the absent edge and cannot recur after any write with a later timestamp: d3). For a new to-edge, C's
        // (0, 0) applies and any other prior timestamp is refused.
        let mut m = TrackerStateManager::new_with_temp_storage();
        let pb = at(1000);
        add(&mut m, &A, &B, 100, pb);
        let t = at(2999);
        let honest = sigs(&A, &B, &C, (100, pb), (0, 0), 30, t);
        let digest = m.get_state().avl_root_digest;
        for p in [1, pb, t] {
            let s = TransferSignatures { to_recipient_prior: ack(&C, &A, &C, (0, p), t), ..honest };
            let r = transfer(&mut m, &A, &B, &C, 30, t, &s);
            assert!(matches!(r, Err(NoteError::InvalidSignature)), "(0, {}) for an absent edge: {:?}", p, r);
        }
        // A first bound note on an absent edge likewise.
        let ts = at(1997);
        let r = bound_note(&mut m, &A, &E, 5, ts, ack(&A, &A, &E, (0, 1), ts));
        assert!(matches!(r, Err(NoteError::InvalidPriorSignature)), "(0, 1) for an absent edge: {:?}", r);
        assert_eq!(m.get_state().avl_root_digest, digest);
        bound_note(&mut m, &A, &E, 5, ts, ack(&A, &A, &E, (0, 0), ts)).unwrap();
        transfer(&mut m, &A, &B, &C, 30, t, &honest).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C), state(&m, &A, &E)), ((70, t), (30, t), (5, ts)));
    }

    #[test]
    fn d2_an_existing_edge_is_never_acknowledged_as_zero_zero_even_at_total_zero() {
        // An existing to-edge is acknowledged at its stored state, never as (0, 0): with a positive total, and also
        // at total 0, which version 3 could not tell from an absent edge (its acknowledgment M(A,C,0,T) has no
        // timestamp). The zero case is the to-edge ABA: C acknowledged A->C before the edge existed, A->C was then
        // created and moved back to 0, and the old acknowledgment would still apply.
        let mut m = TrackerStateManager::new_with_temp_storage();
        let pb = at(1000);
        add(&mut m, &A, &B, 100, pb);
        let t = at(4999);
        let signed_before = sigs(&A, &B, &C, (100, pb), (0, 0), 30, t); // A->C absent when B and C signed
        // A->C is created (10 at pc) and C moves it all out again (C pays D 10 of A's debt): A->C is at (0, tc).
        let pc = at(1001);
        add(&mut m, &A, &C, 10, pc);
        let tc = at(2999);
        transfer(&mut m, &A, &C, &D, 10, tc, &sigs(&A, &C, &D, (10, pc), (0, 0), 10, tc)).unwrap();
        assert_eq!(state(&m, &A, &C), (0, tc));
        // A->E exists with a positive total.
        let e = at(1002);
        add(&mut m, &A, &E, 7, e);
        let digest = m.get_state().avl_root_digest;
        let r = transfer(&mut m, &A, &B, &C, 30, t, &signed_before);
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "(0, 0) for an existing edge at total 0: {:?}", r);
        // A bound note on an existing edge at 0 likewise.
        let ts = at(3997);
        let r = bound_note(&mut m, &A, &C, 4, ts, ack(&A, &A, &C, (0, 0), ts));
        assert!(matches!(r, Err(NoteError::InvalidPriorSignature)), "(0, 0) for an existing edge at total 0: {:?}", r);
        // And for an edge with a positive total, (0, 0) and (Y, 0) are refused.
        for prior in [(0, 0), (7, 0)] {
            let s = TransferSignatures { to_recipient_prior: ack(&E, &A, &E, prior, t), ..sigs(&A, &B, &E, (100, pb), (7, e), 30, t) };
            let r = transfer(&mut m, &A, &B, &E, 30, t, &s);
            assert!(matches!(r, Err(NoteError::InvalidSignature)), "{:?} for an edge at (7, e): {:?}", prior, r);
        }
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Control: acknowledging the stored state (0, tc) applies.
        transfer(&mut m, &A, &B, &C, 30, t, &sigs(&A, &B, &C, (100, pb), (0, tc), 30, t)).unwrap();
        assert_eq!((state(&m, &A, &B), state(&m, &A, &C)), ((70, t), (30, t)));
    }

    #[test]
    fn d3_a_stored_zero_zero_is_the_absent_state_and_cannot_recur() {
        // An ordinary note of total 0 at timestamp 0 is accepted, so an edge can be stored at (0, 0), the state an
        // absent edge is acknowledged as. It is economically the absent edge: acknowledging (0, 0) applies on it as on
        // an absent edge (a bound note, and a transfer's new creditor). It cannot recur: every later write has a later
        // timestamp, so afterwards neither a note at timestamp 0 nor an acknowledgment of (0, 0) applies.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &E, 0, 0);
        add(&mut m, &A, &C, 0, 0);
        assert!(m.get_note_optional(&pk(&A), &pk(&E), &DenomKey::Erg).unwrap().is_some());
        assert_eq!((state(&m, &A, &E), state(&m, &A, &C)), ((0, 0), (0, 0)));
        let ts = at(1997);
        bound_note(&mut m, &A, &E, 5, ts, ack(&A, &A, &E, (0, 0), ts)).unwrap();
        let pb = at(1000);
        add(&mut m, &A, &B, 100, pb);
        let t = at(2999);
        transfer(&mut m, &A, &B, &C, 30, t, &sigs(&A, &B, &C, (100, pb), (0, 0), 30, t)).unwrap();
        assert_eq!((state(&m, &A, &E), state(&m, &A, &C)), ((5, ts), (30, t)));
        let digest = m.get_state().avl_root_digest;
        let r = m.add_note(&pk(&A), &note(&A, &E, 0, 0), None);
        assert!(matches!(r, Err(NoteError::PastTimestamp)), "a note at timestamp 0 after a write: {:?}", r);
        let later = at(3997);
        let r = bound_note(&mut m, &A, &E, 9, later, ack(&A, &A, &E, (0, 0), later));
        assert!(matches!(r, Err(NoteError::InvalidPriorSignature)), "(0, 0) after a write: {:?}", r);
        let t2 = at(4999);
        let s = TransferSignatures { to_recipient_prior: ack(&C, &A, &C, (0, 0), t2), ..sigs(&A, &B, &C, (70, t), (30, t), 10, t2) };
        let r = transfer(&mut m, &A, &B, &C, 10, t2, &s);
        assert!(matches!(r, Err(NoteError::InvalidSignature)), "(0, 0) for the new creditor after a write: {:?}", r);
        assert_eq!(m.get_state().avl_root_digest, digest);
        // Control: acknowledging the stored state applies.
        bound_note(&mut m, &A, &E, 9, later, ack(&A, &A, &E, (5, ts), later)).unwrap();
        assert_eq!(state(&m, &A, &E), (9, later));
    }
}

/// An edge's stored timestamp strictly increases with every write: tracker v3.2's prior acknowledgments rely on it
/// (each state (total, timestamp) of an edge occurs once, so an acknowledgment applies at most once). Phase 2 of an
/// exchange used to write the staged to-edge note at its staging timestamp without comparing it with the edge's
/// current one; it now cancels the exchange when the to-edge was written at or after that timestamp. Every write
/// path refuses a timestamp that is not later than the edge's stored one, and so does the guard each path calls
/// right before its storage commit. Exchanges here use the exchange residue 995.
#[cfg(test)]
mod timestamp_monotonicity_tests {
    use crate::{
        DenomKey, ExchangeObservation, IouNote, NoteError, PendingExchangeStatus, PriorAck, SetOffEdge,
        TrackerStateManager, TransferSignatures,
    };
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const ERG: DenomKey = DenomKey::Erg;
    const TOKEN: DenomKey = DenomKey::Token([0x77; 32]);
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    const D: [u8; 32] = [4u8; 32];

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A whole second ten minutes before this module's first test started (inside the one-hour maximum age).
    fn base() -> u64 {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| (now_ms() - 600_000) / 1000 * 1000)
    }

    /// `offset` ms after [`base`]: offsets ending in 999 are transfers, 998 set-offs, 997 bound notes, 995
    /// exchanges, 000-994 ordinary notes.
    fn at(offset: u64) -> u64 {
        base() + offset
    }

    fn pk(secret: &[u8; 32]) -> [u8; 33] {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret).expect("valid secret key");
        secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
    }

    fn sign_bytes(signer: &[u8; 32], message: &[u8]) -> [u8; 65] {
        crate::schnorr::schnorr_sign(message, signer, &pk(signer)).unwrap()
    }

    /// `signer`'s signature over the note message M(issuer, recipient, denomination, total, ts).
    fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], d: DenomKey, total: u64, ts: u64) -> [u8; 65] {
        sign_bytes(signer, &crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &d, total, ts, &NFT))
    }

    /// `signer`'s prior acknowledgment of issuer -> recipient (ERG) in the state `prior` for the operation at `ts`.
    fn ack(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], prior: (u64, u64), ts: u64) -> [u8; 65] {
        sign_bytes(
            signer,
            &crate::schnorr::prior_ack_message(&pk(issuer), &pk(recipient), &ERG, prior.0, prior.1, ts, &NFT),
        )
    }

    fn note(issuer: &[u8; 32], recipient: &[u8; 32], d: DenomKey, total: u64, ts: u64) -> IouNote {
        IouNote::create_and_sign(pk(recipient), total, ts, issuer, &NFT, d).unwrap()
    }

    /// An ordinary note.
    fn add(m: &mut TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32], d: DenomKey, total: u64, ts: u64) {
        m.add_note(&pk(issuer), &note(issuer, recipient, d, total, ts), None).unwrap();
    }

    /// The stored state (total, timestamp) of issuer -> recipient, (0, 0) when the edge is absent.
    fn state(m: &TrackerStateManager, issuer: &[u8; 32], recipient: &[u8; 32], d: DenomKey) -> (u64, u64) {
        m.get_note_optional(&pk(issuer), &pk(recipient), &d)
            .unwrap()
            .map(|n| (n.amount_collected, n.timestamp))
            .unwrap_or((0, 0))
    }

    /// An ERG -> TOKEN exchange on A->C at `ts`, signed by A and C: ERG lowered by 30, TOKEN raised by 40, from
    /// the edges' current totals.
    fn exchange(m: &mut TrackerStateManager, ts: u64) -> Result<(), NoteError> {
        let (erg, token) = (state(m, &A, &C, ERG).0 - 30, state(m, &A, &C, TOKEN).0 + 40);
        m.exchange_debt(
            &pk(&A), &pk(&C), &ERG, 30, &TOKEN, 40, ts,
            &sign(&A, &A, &C, ERG, erg, ts), &sign(&C, &A, &C, ERG, erg, ts),
            &sign(&A, &A, &C, TOKEN, token, ts), &sign(&C, &A, &C, TOKEN, token, ts),
            "reserve_box",
        )
    }

    /// A tracker box commits the staged exchange's from-edge decrease and its reserve is observed intact: phase 2
    /// applies or cancels it. Returns (applied, cancelled).
    fn phase_two(m: &mut TrackerStateManager) -> (usize, usize) {
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 1).unwrap();
        m.confirm_pending_notes(digest, "box1", 2).unwrap();
        let staged = m.list_staged_exchanges().unwrap();
        assert_eq!(staged.len(), 1);
        let observation = ExchangeObservation {
            key: staged[0].key,
            reserve_r5_digest: Some(m.reserve_state_digest(&pk(&A), &ERG).try_into().unwrap()),
        };
        let processed = m.process_staged_exchanges(&[observation]).unwrap();
        (processed.applied, processed.cancelled)
    }

    /// The seven signatures of a transfer of `amount` of A's ERG debt from A -> from to A -> to at `ts`, each
    /// creditor acknowledging its edge in the given state ((0, 0) for an absent edge).
    fn transfer(
        m: &mut TrackerStateManager, from: &[u8; 32], to: &[u8; 32], from_state: (u64, u64), to_state: (u64, u64),
        amount: u64, ts: u64,
    ) -> Result<(), NoteError> {
        let (from_total, to_total) = (from_state.0 - amount, to_state.0 + amount);
        let s = TransferSignatures {
            issuer_from: sign(&A, &A, from, ERG, from_total, ts),
            issuer_to: sign(&A, &A, to, ERG, to_total, ts),
            from_recipient_prior: ack(from, &A, from, from_state, ts),
            from_recipient: sign(from, &A, from, ERG, from_total, ts),
            from_recipient_to: sign(from, &A, to, ERG, to_total, ts),
            to_recipient_prior: ack(to, &A, to, to_state, ts),
            to_recipient: sign(to, &A, to, ERG, to_total, ts),
        };
        m.transfer_debt(&pk(&A), &pk(from), &pk(to), &ERG, amount, ts, &s, None, None, None, None).map(|_| ())
    }

    /// A set-off edge issuer -> recipient (ERG) lowered from `before` by `decrease` at `ts`, signed by both parties.
    fn edge(issuer: &[u8; 32], recipient: &[u8; 32], before: u64, decrease: u64, ts: u64) -> SetOffEdge {
        SetOffEdge {
            issuer_pubkey: pk(issuer),
            recipient_pubkey: pk(recipient),
            decrease,
            timestamp: ts,
            issuer_signature: sign(issuer, issuer, recipient, ERG, before - decrease, ts),
            recipient_signature: sign(recipient, issuer, recipient, ERG, before - decrease, ts),
        }
    }

    fn past(r: Result<(), NoteError>, what: &str) {
        assert!(matches!(r, Err(NoteError::PastTimestamp)), "{}: {:?}", what, r);
    }

    #[test]
    fn a_staged_exchange_is_cancelled_when_its_to_edge_was_written_at_a_later_timestamp_meanwhile() {
        // A->C TOKEN is at (50, t1); an exchange staged at t2 raises it to 90 once its tracker box commits; meanwhile
        // an ordinary note writes A->C TOKEN at the same total at t3 > t2. Applying the staged note would move the
        // edge from (50, t3) to (90, t2): its timestamp back, after which the state (50, t3) could occur again (an
        // exchange out of the token edge at t3 lowers it to 50 there). Phase 2 cancels the exchange instead.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &C, ERG, 100, at(1000));
        add(&mut m, &A, &C, TOKEN, 50, at(1001));
        exchange(&mut m, at(1995)).unwrap();
        add(&mut m, &A, &C, TOKEN, 50, at(3000));
        assert_eq!(phase_two(&mut m), (0, 1), "the staged exchange is cancelled, not applied");
        assert_eq!(state(&m, &A, &C, TOKEN), (50, at(3000)));
        assert_eq!(state(&m, &A, &C, ERG), (70, at(1995)), "the from-edge decrease stays applied, as for every cancellation");
        let records = m.list_pending_exchanges(None, None).unwrap();
        assert_eq!(records.len(), 1);
        assert!(
            matches!(&records[0].1.status, PendingExchangeStatus::Cancelled { reason } if reason.contains("would move the edge's timestamp back")),
            "{:?}",
            records[0].1.status
        );
    }

    #[test]
    fn a_staged_exchange_still_applies_when_its_to_edge_was_written_at_the_same_total_before_its_timestamp() {
        // Control: the same note at t1 < t' < t2 leaves the to-edge's total unchanged and earlier than the exchange,
        // so the exchange applies and the edge's timestamp keeps increasing (t1, t', t2).
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &C, ERG, 100, at(1000));
        add(&mut m, &A, &C, TOKEN, 50, at(1001));
        exchange(&mut m, at(1995)).unwrap();
        add(&mut m, &A, &C, TOKEN, 50, at(1500));
        assert_eq!(phase_two(&mut m), (1, 0));
        assert_eq!(state(&m, &A, &C, TOKEN), (90, at(1995)));
        assert!(m.list_pending_exchanges(None, None).unwrap().is_empty());
    }

    #[test]
    fn every_write_path_refuses_a_timestamp_not_later_than_the_edge_s_stored_one() {
        // Each path at the edge's stored timestamp and before it: PastTimestamp, and nothing changes. (Phase 2 of
        // an exchange: the two tests above.)
        // POST /notes, an ordinary note, and update_note.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &B, ERG, 10, at(2000));
        let digest = m.get_state().avl_root_digest;
        for ts in [at(2000), at(1000)] {
            past(m.add_note(&pk(&A), &note(&A, &B, ERG, 20, ts), None), "ordinary note");
            past(m.update_note(&pk(&A), &note(&A, &B, ERG, 20, ts)), "update_note");
        }
        assert_eq!(m.get_state().avl_root_digest, digest);
        // POST /notes, a bound note: the edge was created by a bound note at t.
        let t = at(2997);
        let first = PriorAck { signature: ack(&A, &A, &C, (0, 0), t), prior_total: None, prior_timestamp: None };
        m.add_note(&pk(&A), &note(&A, &C, ERG, 10, t), Some(first)).unwrap();
        let digest_c = m.get_state().avl_root_digest;
        for ts in [t, at(1997)] {
            let prior = PriorAck { signature: ack(&A, &A, &C, (10, t), ts), prior_total: None, prior_timestamp: None };
            past(m.add_note(&pk(&A), &note(&A, &C, ERG, 20, ts), Some(prior)), "bound note");
        }
        assert_eq!(m.get_state().avl_root_digest, digest_c);
        assert_ne!(digest, digest_c);
        assert_eq!((state(&m, &A, &B, ERG), state(&m, &A, &C, ERG)), ((10, at(2000)), (10, t)));

        // An exchange: the from-edge, then the to-edge, later than the exchange; then both edges at its timestamp
        // (after an exchange at that timestamp applied).
        for (erg_ts, token_ts) in [(at(3000), at(1001)), (at(1000), at(3000))] {
            let mut m = TrackerStateManager::new_with_temp_storage();
            add(&mut m, &A, &C, ERG, 100, erg_ts);
            add(&mut m, &A, &C, TOKEN, 50, token_ts);
            let digest = m.get_state().avl_root_digest;
            past(exchange(&mut m, at(2995)), "exchange");
            assert_eq!(m.get_state().avl_root_digest, digest);
            assert!(m.list_pending_exchanges(None, None).unwrap().is_empty());
        }
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &C, ERG, 100, at(1000));
        add(&mut m, &A, &C, TOKEN, 50, at(1001));
        exchange(&mut m, at(1995)).unwrap();
        assert_eq!(phase_two(&mut m), (1, 0));
        let digest = m.get_state().avl_root_digest;
        past(exchange(&mut m, at(1995)), "exchange at the edges' timestamp");
        assert_eq!(m.get_state().avl_root_digest, digest);
        assert_eq!((state(&m, &A, &C, ERG), state(&m, &A, &C, TOKEN)), ((70, at(1995)), (90, at(1995))));

        // A debt transfer: both edges at its timestamp (after a transfer at that timestamp applied) and before it;
        // then the to-edge later than the transfer.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &B, ERG, 100, at(1000));
        let t = at(2999);
        transfer(&mut m, &B, &C, (100, at(1000)), (0, 0), 30, t).unwrap();
        add(&mut m, &A, &D, ERG, 5, at(4000));
        let digest = m.get_state().avl_root_digest;
        for ts in [t, at(1999)] {
            past(transfer(&mut m, &B, &C, (70, t), (30, t), 10, ts), "transfer");
        }
        past(transfer(&mut m, &B, &D, (70, t), (5, at(4000)), 10, at(3999)), "transfer to a later edge");
        assert_eq!(m.get_state().avl_root_digest, digest);
        assert_eq!((state(&m, &A, &B, ERG), state(&m, &A, &C, ERG)), ((70, t), (30, t)));

        // A set-off: both edges at its timestamp (after a set-off at that timestamp applied), both before it, and
        // one edge at its stored timestamp next to a fresh one.
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &A, &B, ERG, 10, at(1000));
        add(&mut m, &B, &A, ERG, 10, at(1001));
        let s = at(1998);
        m.set_off(&ERG, &[edge(&A, &B, 10, 5, s), edge(&B, &A, 10, 5, s)]).unwrap();
        let digest = m.get_state().avl_root_digest;
        for (ab, ba) in [(s, s), (at(998), at(998)), (at(2998), s)] {
            let r = m.set_off(&ERG, &[edge(&A, &B, 5, 2, ab), edge(&B, &A, 5, 2, ba)]).map(|_| ());
            past(r, "set-off");
        }
        assert_eq!(m.get_state().avl_root_digest, digest);
        assert_eq!((state(&m, &A, &B, ERG), state(&m, &B, &A, ERG)), ((5, s), (5, s)));
    }

    #[test]
    fn the_write_guard_refuses_a_timestamp_not_later_than_the_stored_one() {
        // The check every write path makes right before its storage commit.
        let mut m = TrackerStateManager::new_with_temp_storage();
        // An absent edge takes any timestamp, 0 included.
        for ts in [0, 1, at(1000)] {
            assert!(m.require_later_timestamp(&pk(&A), &note(&A, &B, ERG, 5, ts)).is_ok(), "{}", ts);
        }
        add(&mut m, &A, &B, ERG, 10, at(2000));
        for ts in [0, at(1999), at(2000)] {
            past(m.require_later_timestamp(&pk(&A), &note(&A, &B, ERG, 20, ts)), "guard");
        }
        assert!(m.require_later_timestamp(&pk(&A), &note(&A, &B, ERG, 20, at(2001))).is_ok());
        // Another denomination or another creditor is another edge.
        assert!(m.require_later_timestamp(&pk(&A), &note(&A, &B, TOKEN, 20, at(1000))).is_ok());
        assert!(m.require_later_timestamp(&pk(&A), &note(&A, &C, ERG, 20, at(1000))).is_ok());
    }
}
