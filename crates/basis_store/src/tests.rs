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
/// edges: A owes B; B is paid by moving part of A's debt to C.
#[cfg(test)]
mod debt_transfer_tests {
    use crate::{
        is_bound_note_timestamp, is_reserved_timestamp, is_setoff_timestamp, is_transfer_timestamp,
        DenomKey, ExchangeAcceptanceView, IouNote, NoteError, TrackerStateManager, TransferTotals,
    };
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const TOKEN: [u8; 32] = [0xa5; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    const E: [u8; 32] = [4u8; 32];
    const Z: [u8; 32] = [5u8; 32];
    // Novation timestamps end in 999 ms.
    const T1: u64 = 1999;
    const T2: u64 = 2999;

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

    /// The five signatures of a transfer of `amount` from A->old to A->new.
    struct Sigs {
        issuer_from: [u8; 65],
        from_recipient: [u8; 65],
        from_recipient_to: [u8; 65],
        issuer_to: [u8; 65],
        to_recipient: [u8; 65],
    }

    fn sigs(old: &[u8; 32], new: &[u8; 32], from_before: u64, to_before: u64, amount: u64, ts: u64) -> Sigs {
        let (a, o, n) = (pubkey(&A), pubkey(old), pubkey(new));
        let from_total = from_before.saturating_sub(amount);
        Sigs {
            issuer_from: sign(&A, &a, &o, from_total, ts),
            from_recipient: sign(old, &a, &o, from_total, ts),
            from_recipient_to: sign(old, &a, &n, to_before + amount, ts),
            issuer_to: sign(&A, &a, &n, to_before + amount, ts),
            to_recipient: sign(new, &a, &n, to_before + amount, ts),
        }
    }

    fn transfer(m: &mut TrackerStateManager, old: &[u8; 32], new: &[u8; 32], amount: u64, ts: u64, s: &Sigs)
        -> Result<TransferTotals, NoteError> {
        m.transfer_debt(
            &pubkey(&A), &pubkey(old), &pubkey(new), &DenomKey::Token(TOKEN), amount, ts,
            &s.issuer_from, &s.from_recipient, &s.from_recipient_to, &s.issuer_to, &s.to_recipient,
        )
    }

    fn total(m: &TrackerStateManager, recipient: &[u8; 32]) -> Option<u64> {
        m.get_note_optional(&pubkey(&A), &pubkey(recipient), &DenomKey::Token(TOKEN))
            .unwrap()
            .map(|n| n.amount_collected)
    }

    /// A owes B 100 (and C `to_before`, if any).
    fn setup(to_before: u64) -> TrackerStateManager {
        let mut m = TrackerStateManager::new_with_temp_storage();
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 100, 1000), None).unwrap();
        if to_before > 0 {
            m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), to_before, 1001), None).unwrap();
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
    }

    #[test]
    fn transfer_moves_debt_to_a_new_creditor() {
        let mut m = setup(0);
        let digest_before = m.get_state().avl_root_digest;
        let totals = transfer(&mut m, &B, &C, 30, T1, &sigs(&B, &C, 100, 0, 30, T1)).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 70, to_new_total: 30 });
        assert_eq!(total(&m, &B), Some(70));
        assert_eq!(total(&m, &C), Some(30));
        assert_ne!(m.get_state().avl_root_digest, digest_before);
        let c_notes = m.get_recipient_notes(&pubkey(&C)).unwrap();
        assert_eq!(c_notes.len(), 1);
        assert_eq!(c_notes[0].amount_collected, 30);
        let b_note = m.lookup_note(&pubkey(&A), &pubkey(&B), &DenomKey::Token(TOKEN)).unwrap();
        assert_eq!((b_note.amount_collected, b_note.timestamp), (70, T1));
        assert!(b_note.verify_signature(&pubkey(&A)).is_ok());
        let c_note = m.lookup_note(&pubkey(&A), &pubkey(&C), &DenomKey::Token(TOKEN)).unwrap();
        assert!(c_note.verify_signature(&pubkey(&A)).is_ok());
    }

    #[test]
    fn transfer_adds_to_an_existing_edge_and_can_move_the_whole_debt() {
        let mut m = setup(10);
        let totals = transfer(&mut m, &B, &C, 100, T1, &sigs(&B, &C, 100, 10, 100, T1)).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 0, to_new_total: 110 });
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(0), Some(110)));
    }

    #[test]
    fn transferred_debt_can_move_again() {
        let mut m = setup(0);
        transfer(&mut m, &B, &C, 30, T1, &sigs(&B, &C, 100, 0, 30, T1)).unwrap();
        transfer(&mut m, &C, &E, 20, T2, &sigs(&C, &E, 30, 0, 20, T2)).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C), total(&m, &E)), (Some(70), Some(10), Some(20)));
    }

    #[test]
    fn the_old_creditors_consent_is_required() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let mut s = sigs(&B, &C, 100, 0, 30, T1);
        s.from_recipient = sign(&C, &pubkey(&A), &pubkey(&B), 70, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidSignature)));
        s.from_recipient = sign(&A, &pubkey(&A), &pubkey(&B), 70, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn the_old_creditors_consent_binds_the_destination() {
        // A (with a key Z it controls) takes B's from-edge consent and tries to send the debt to Z instead of C.
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let honest = sigs(&B, &C, 100, 0, 30, T1);
        let puppet = sigs(&B, &Z, 100, 0, 30, T1); // A and Z can produce every signature except B's
        let forged = Sigs {
            issuer_from: puppet.issuer_from,
            from_recipient: honest.from_recipient,        // B's real consent (from-edge)
            from_recipient_to: honest.from_recipient_to,  // B's real consent (bound to A->C)
            issuer_to: puppet.issuer_to,
            to_recipient: puppet.to_recipient,
        };
        assert!(matches!(transfer(&mut m, &B, &Z, 30, T1, &forged), Err(NoteError::InvalidSignature)));
        // Without B's to-edge signature at all, it fails too.
        let mut s = sigs(&B, &C, 100, 0, 30, T1);
        s.from_recipient_to = sign(&A, &pubkey(&A), &pubkey(&C), 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn the_new_creditors_acceptance_and_the_issuers_signatures_are_required() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let mut s = sigs(&B, &C, 100, 0, 30, T1);
        s.to_recipient = sign(&B, &pubkey(&A), &pubkey(&C), 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, 100, 0, 30, T1);
        s.issuer_to = sign(&B, &pubkey(&A), &pubkey(&C), 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidSignature)));
        let mut s = sigs(&B, &C, 100, 0, 30, T1);
        s.issuer_from = sign(&C, &pubkey(&A), &pubkey(&B), 70, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidSignature)));
        let s = sigs(&B, &C, 100, 0, 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 31, T1, &s), Err(NoteError::InvalidSignature)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn the_raised_edges_issuer_signature_cannot_be_used_as_an_ordinary_note() {
        // The issuer's to-edge signature is over a valid note message; its novation timestamp keeps it out of
        // POST /notes, so C cannot collect it without B's edge going down.
        let mut m = setup(0);
        let s = sigs(&B, &C, 100, 0, 30, T1);
        let standalone = IouNote {
            recipient_pubkey: pubkey(&C),
            amount_collected: 30,
            amount_redeemed: 0,
            timestamp: T1,
            signature: s.issuer_to,
            tracker_nft_id: NFT,
            denomination: DenomKey::Token(TOKEN),
        };
        assert!(standalone.verify_signature(&pubkey(&A)).is_ok());
        assert!(matches!(m.add_note(&pubkey(&A), &standalone, None), Err(NoteError::ReservedTimestamp)));
        assert_eq!(total(&m, &C), None);
        // Ordinary notes at other milliseconds are unaffected (995-998 are now reserved too, so the latest
        // ordinary millisecond before T1 is 994).
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), 5, 1994), None).unwrap();
    }

    #[test]
    fn a_transfer_needs_a_novation_timestamp() {
        let mut m = setup(0);
        let s = sigs(&B, &C, 100, 0, 30, 2000);
        assert!(matches!(transfer(&mut m, &B, &C, 30, 2000, &s), Err(NoteError::InvalidTransfer(_))));
        assert_eq!(total(&m, &B), Some(100));
    }

    #[test]
    fn cannot_transfer_more_than_is_owed_or_nothing() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let s = sigs(&B, &C, 100, 0, 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 101, T1, &s), Err(NoteError::InvalidTransfer(_))));
        assert!(matches!(transfer(&mut m, &B, &C, 0, T1, &s), Err(NoteError::InvalidTransfer(_))));
        assert!(matches!(transfer(&mut m, &E, &C, 10, T1, &s), Err(NoteError::NotFound)));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn creditors_must_differ_and_cannot_be_the_issuer() {
        let mut m = setup(0);
        let s = sigs(&B, &C, 100, 0, 30, T1);
        assert!(matches!(transfer(&mut m, &B, &B, 30, T1, &s), Err(NoteError::InvalidTransfer(_))));
        assert!(matches!(transfer(&mut m, &B, &A, 30, T1, &s), Err(NoteError::InvalidTransfer(_))));
    }

    #[test]
    fn timestamps_must_move_forward_so_a_transfer_cannot_be_replayed() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        let s = sigs(&B, &C, 100, 10, 30, 999);
        assert!(matches!(transfer(&mut m, &B, &C, 30, 999, &s), Err(NoteError::PastTimestamp)));
        assert_unchanged(&m, &digest, 100, Some(10));
        let s = sigs(&B, &C, 100, 10, 30, T1);
        transfer(&mut m, &B, &C, 30, T1, &s).unwrap();
        assert!(transfer(&mut m, &B, &C, 30, T1, &s).is_err());
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
        assert!(m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&B), 100, 1000), None).is_err());
    }

    #[test]
    fn refused_while_the_old_edge_has_a_pending_or_committed_on_chain_total() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 100).unwrap();
        let s = sigs(&B, &C, 100, 0, 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidTransfer(_))));
        m.confirm_pending_notes(digest, "box1", 101).unwrap();
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::InvalidTransfer(_))));
        assert_unchanged(&m, &digest, 100, None);
    }

    #[test]
    fn only_the_edge_being_lowered_must_be_free_of_on_chain_state() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 100).unwrap();
        m.confirm_pending_notes(digest, "box1", 101).unwrap();
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&E), 50, 1500), None).unwrap();
        let totals = transfer(&mut m, &E, &C, 20, T1, &sigs(&E, &C, 50, 10, 20, T1)).unwrap();
        assert_eq!(totals, TransferTotals { from_new_total: 30, to_new_total: 30 });
    }

    #[test]
    fn a_storage_failure_leaves_state_unchanged() {
        let mut m = setup(0);
        let digest = m.get_state().avl_root_digest;
        let confirmations = m.all_confirmations();
        m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
        let s = sigs(&B, &C, 100, 0, 30, T1);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::StorageError(_))));
        assert_unchanged(&m, &digest, 100, None);
        assert_eq!(m.all_confirmations(), confirmations);
        m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
        transfer(&mut m, &B, &C, 30, T1, &s).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(30)));
    }

    #[test]
    fn the_new_creditors_acceptance_policy_sees_the_pre_transfer_state_and_can_refuse() {
        let mut m = setup(10);
        let digest = m.get_state().avl_root_digest;
        let s = sigs(&B, &C, 100, 10, 30, T1);
        let refuse: crate::ExchangeAcceptanceCheckFn = Box::new(|view: ExchangeAcceptanceView| {
            assert_eq!(view.to_note.map(|n| n.amount_collected), Some(10));
            Err("C accepts at most 20 from A".to_string())
        });
        let r = m.transfer_debt_checked(
            &pubkey(&A), &pubkey(&B), &pubkey(&C), &DenomKey::Token(TOKEN), 30, T1,
            &s.issuer_from, &s.from_recipient, &s.from_recipient_to, &s.issuer_to, &s.to_recipient, Some(refuse),
        );
        assert!(matches!(r, Err(NoteError::AcceptancePolicy(_))));
        assert_unchanged(&m, &digest, 100, Some(10));
        let accept: crate::ExchangeAcceptanceCheckFn = Box::new(|_view: ExchangeAcceptanceView| Ok(()));
        m.transfer_debt_checked(
            &pubkey(&A), &pubkey(&B), &pubkey(&C), &DenomKey::Token(TOKEN), 30, T1,
            &s.issuer_from, &s.from_recipient, &s.from_recipient_to, &s.issuer_to, &s.to_recipient, Some(accept),
        )
        .unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(40)));
    }

    #[test]
    fn the_tree_stays_reproducible_after_a_transfer() {
        let mut m = setup(10);
        transfer(&mut m, &B, &C, 30, T1, &sigs(&B, &C, 100, 10, 30, T1)).unwrap();
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
        let bound = |m: &TrackerStateManager, r: &[u8; 32]| {
            m.is_bound_only(&pubkey(&A), &pubkey(r), &DenomKey::Token(TOKEN)).unwrap()
        };
        let s = sigs(&B, &C, 100, 10, 30, T1);
        m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(transfer(&mut m, &B, &C, 30, T1, &s), Err(NoteError::StorageError(_))));
        assert!(!bound(&m, &B) && !bound(&m, &C));
        m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
        transfer(&mut m, &B, &C, 30, T1, &s).unwrap();
        assert!(bound(&m, &B) && bound(&m, &C));
        // An edge the transfer did not touch is not marked.
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&E), 5, 1500), None).unwrap();
        assert!(!bound(&m, &E));
        // On a marked edge an ordinary note is refused and a bound note (residue 997, the issuer's acknowledgment
        // of the current total 40) applies. Timestamps are relative to T1, which ends in 999.
        let ordinary = create_note(&A, &pubkey(&C), 50, T1 + 1001);
        assert!(matches!(m.add_note(&pubkey(&A), &ordinary, None), Err(NoteError::BoundNoteRequired)));
        let ts = T1 + 1998;
        let prior = crate::PriorAck { signature: sign(&A, &pubkey(&A), &pubkey(&C), 40, ts), prior_total: Some(40) };
        m.add_note(&pubkey(&A), &create_note(&A, &pubkey(&C), 50, ts), Some(prior)).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(70), Some(50)));
    }
}

/// Multilateral set-off (obligation clearing), pure-credit edges: lowered edges,
/// two signatures each, balanced for every participant.
#[cfg(test)]
mod setoff_tests {
    use crate::{DenomKey, IouNote, NoteError, SetOffEdge, TrackerStateManager, MAX_SETOFF_EDGES};
    use secp256k1::{Secp256k1, SecretKey};

    const NFT: [u8; 32] = [0x42; 32];
    const TOKEN: [u8; 32] = [0xa5; 32];
    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];
    const C: [u8; 32] = [3u8; 32];
    const D: [u8; 32] = [4u8; 32];
    // Set-off timestamps end in 998 ms (the set-off residue; 999 belongs to transfers).
    const T: u64 = 5998;

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

    fn sign(secret: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &DenomKey::Token(TOKEN), total, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, secret, &pk(secret)).unwrap()
    }

    /// An edge lowered from `before` by `decrease`, signed by both parties at `ts`.
    fn edge(issuer: &[u8; 32], recipient: &[u8; 32], before: u64, decrease: u64, ts: u64) -> SetOffEdge {
        let new_total = before.saturating_sub(decrease);
        SetOffEdge {
            issuer_pubkey: pk(issuer),
            recipient_pubkey: pk(recipient),
            decrease,
            issuer_signature: sign(issuer, issuer, recipient, new_total, ts),
            recipient_signature: sign(recipient, issuer, recipient, new_total, ts),
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
        note(&mut m, &A, &B, 10, 1000);
        note(&mut m, &B, &C, 7, 1001);
        note(&mut m, &C, &A, 5, 1002);
        m
    }

    #[test]
    fn a_cycle_is_cleared_and_every_net_position_is_unchanged() {
        let mut m = triangle();
        let all = [A, B, C];
        let nets: Vec<i64> = all.iter().map(|w| net(&m, w, &all)).collect();
        let r = m.set_off(&DenomKey::Token(TOKEN), T, &[edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)]).unwrap();
        assert_eq!(r.total_cleared, 15);
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (5, 2, 0));
        let after: Vec<i64> = all.iter().map(|w| net(&m, w, &all)).collect();
        assert_eq!(nets, after);
        // The lowered notes still carry their issuers' signatures over the new totals.
        let n = m.lookup_note(&pk(&A), &pk(&B), &DenomKey::Token(TOKEN)).unwrap();
        assert!(n.verify_signature(&pk(&A)).is_ok());
        assert_eq!(n.timestamp, T);
    }

    #[test]
    fn a_bilateral_pair_and_a_figure_eight_clear() {
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 8, 1000);
        note(&mut m, &B, &A, 3, 1001);
        m.set_off(&DenomKey::Token(TOKEN), T, &[edge(&A, &B, 8, 3, T), edge(&B, &A, 3, 3, T)]).unwrap();
        assert_eq!((total(&m, &A, &B), total(&m, &B, &A)), (5, 0));
        // Two cycles through B: A<->B and B<->C, cleared together.
        let mut m = TrackerStateManager::new_with_temp_storage();
        note(&mut m, &A, &B, 4, 1000);
        note(&mut m, &B, &A, 4, 1001);
        note(&mut m, &B, &C, 2, 1002);
        note(&mut m, &C, &B, 2, 1003);
        let r = m.set_off(&DenomKey::Token(TOKEN), T, &[edge(&A, &B, 4, 4, T), edge(&B, &A, 4, 4, T), edge(&B, &C, 2, 2, T), edge(&C, &B, 2, 2, T)]).unwrap();
        assert_eq!(r.total_cleared, 12);
    }

    fn unchanged(m: &TrackerStateManager, digest: &[u8; 33]) {
        assert_eq!((total(m, &A, &B), total(m, &B, &C), total(m, &C, &A)), (10, 7, 5));
        assert_eq!(&m.get_state().avl_root_digest, digest);
    }

    #[test]
    fn unbalanced_decreases_are_refused() {
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        // B would lose 5 of its claim on A while its own debt to C falls by only 4.
        let r = m.set_off(&DenomKey::Token(TOKEN), T, &[edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 4, T), edge(&C, &A, 5, 5, T)]);
        assert!(matches!(r, Err(NoteError::InvalidSetOff(_))));
        unchanged(&m, &digest);
    }

    #[test]
    fn signatures_from_a_valid_set_off_cannot_be_used_in_an_unbalanced_subset() {
        // Everyone signed the full cycle. Submitting only two of its edges (dropping C->A) would leave A paying
        // B without being paid by C: the balance check refuses it.
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        let full = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        let r = m.set_off(&DenomKey::Token(TOKEN), T, &full[..2]);
        assert!(matches!(r, Err(NoteError::InvalidSetOff(_))));
        unchanged(&m, &digest);
        m.set_off(&DenomKey::Token(TOKEN), T, &full).unwrap();
    }

    #[test]
    fn both_parties_of_every_edge_must_sign() {
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        let mut e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        e[0].recipient_signature = sign(&A, &A, &B, 5, T); // the issuer signs in the creditor's place
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), T, &e), Err(NoteError::InvalidSignature)));
        let mut e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        e[1].issuer_signature = sign(&C, &B, &C, 2, T);
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), T, &e), Err(NoteError::InvalidSignature)));
        // Signatures over other totals do not verify.
        let mut e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        e[2].decrease = 4;
        e[0].decrease = 4;
        e[1].decrease = 4;
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), T, &e), Err(NoteError::InvalidSignature)));
        unchanged(&m, &digest);
    }

    #[test]
    fn malformed_set_offs_are_refused() {
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        let d = DenomKey::Token(TOKEN);
        let invalid = |r: Result<crate::SetOffResult, NoteError>| matches!(r, Err(NoteError::InvalidSetOff(_)));
        assert!(invalid(m.set_off(&d, T, &[edge(&A, &B, 10, 5, T)])));
        assert!(invalid(m.set_off(&d, T, &[edge(&A, &B, 10, 5, T), edge(&A, &B, 10, 5, T)])));
        assert!(invalid(m.set_off(&d, T, &[edge(&A, &A, 10, 5, T), edge(&B, &C, 7, 5, T)])));
        assert!(invalid(m.set_off(&d, T, &[edge(&A, &B, 10, 0, T), edge(&B, &A, 0, 0, T)])));
        // A decrease larger than the edge.
        assert!(invalid(m.set_off(&d, T, &[edge(&A, &B, 10, 6, T), edge(&B, &C, 7, 6, T), edge(&C, &A, 5, 6, T)])));
        // Not the set-off residue (998 ms).
        assert!(invalid(m.set_off(&d, 6000, &[edge(&A, &B, 10, 5, 6000), edge(&B, &C, 7, 5, 6000), edge(&C, &A, 5, 5, 6000)])));
        // Too many edges.
        let many: Vec<SetOffEdge> = (0..=MAX_SETOFF_EDGES).map(|_| edge(&A, &B, 10, 1, T)).collect();
        assert!(invalid(m.set_off(&d, T, &many)));
        // A missing edge.
        assert!(matches!(m.set_off(&d, T, &[edge(&A, &D, 10, 5, T), edge(&D, &A, 10, 5, T)]), Err(NoteError::NotFound)));
        // Not newer than the edges.
        assert!(matches!(m.set_off(&d, 998, &[edge(&A, &B, 10, 5, 998), edge(&B, &C, 7, 5, 998), edge(&C, &A, 5, 5, 998)]), Err(NoteError::PastTimestamp)));
        unchanged(&m, &digest);
    }

    #[test]
    fn a_set_off_cannot_be_replayed() {
        let mut m = triangle();
        let e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        m.set_off(&DenomKey::Token(TOKEN), T, &e).unwrap();
        assert!(m.set_off(&DenomKey::Token(TOKEN), T, &e).is_err());
        assert_eq!((total(&m, &A, &B), total(&m, &B, &C), total(&m, &C, &A)), (5, 2, 0));
    }

    #[test]
    fn edges_with_on_chain_state_are_refused() {
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        m.mark_notes_pending(digest, "tx1", 100).unwrap();
        let e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), T, &e), Err(NoteError::InvalidSetOff(_))));
        unchanged(&m, &digest);
    }

    #[test]
    fn a_storage_failure_leaves_state_unchanged_and_the_tree_stays_reproducible() {
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        let e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), T, &e), Err(NoteError::StorageError(_))));
        unchanged(&m, &digest);
        m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
        m.set_off(&DenomKey::Token(TOKEN), T, &e).unwrap();
        let live = m.get_state().avl_root_digest;
        m.rebuild_tree_canonically().unwrap();
        m.update_state();
        assert_eq!(m.get_state().avl_root_digest, live);
    }

    #[test]
    fn ordinary_notes_still_work_after_a_set_off() {
        // Tracker v3: every edge a set-off lowered is bound-only afterwards, so a new deal on it is a bound note
        // (residue 997, with the issuer's acknowledgment of the current total) and an ordinary note there is
        // refused. Edges the set-off did not touch take ordinary notes as before.
        let mut m = triangle();
        m.set_off(&DenomKey::Token(TOKEN), T, &[edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)]).unwrap();
        // A new deal: A owes B 3 more (cumulative 8), at a later timestamp.
        let ordinary = IouNote::create_and_sign(pk(&B), 8, 7000, &A, &NFT, DenomKey::Token(TOKEN)).unwrap();
        assert!(matches!(m.add_note(&pk(&A), &ordinary, None), Err(NoteError::BoundNoteRequired)));
        let bound = IouNote::create_and_sign(pk(&B), 8, 7997, &A, &NFT, DenomKey::Token(TOKEN)).unwrap();
        let prior = crate::PriorAck { signature: sign(&A, &A, &B, 5, 7997), prior_total: Some(5) };
        m.add_note(&pk(&A), &bound, Some(prior)).unwrap();
        assert_eq!(total(&m, &A, &B), 8);
        // An edge outside the set-off still takes ordinary notes.
        note(&mut m, &A, &C, 4, 7000);
        assert_eq!(total(&m, &A, &C), 4);
    }

    #[test]
    fn a_set_off_marks_every_edge_bound_only_and_the_marks_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let d = DenomKey::Token(TOKEN);
        let cycle = [(A, B), (B, C), (C, A)];
        let e = [edge(&A, &B, 10, 5, T), edge(&B, &C, 7, 5, T), edge(&C, &A, 5, 5, T)];
        {
            let mut m = TrackerStateManager::new(dir.path());
            note(&mut m, &A, &B, 10, 1000);
            note(&mut m, &B, &C, 7, 1001);
            note(&mut m, &C, &A, 5, 1002);
            note(&mut m, &A, &C, 3, 1003); // outside the cycle
            // A set-off that fails to commit marks nothing (the marks are in its storage batch).
            m.storage.fail_exchange_commits.store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(matches!(m.set_off(&d, T, &e), Err(NoteError::StorageError(_))));
            for (i, r) in cycle {
                assert!(!m.is_bound_only(&pk(&i), &pk(&r), &d).unwrap());
            }
            m.storage.fail_exchange_commits.store(false, std::sync::atomic::Ordering::SeqCst);
            m.set_off(&d, T, &e).unwrap();
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
        // After the restart an ordinary note on a set-off edge is still refused (T ends in 998, so T + 1002 is an
        // ordinary millisecond), and the untouched edge still takes ordinary notes.
        let ordinary = IouNote::create_and_sign(pk(&B), 8, T + 1002, &A, &NFT, DenomKey::Token(TOKEN)).unwrap();
        assert!(matches!(m.add_note(&pk(&A), &ordinary, None), Err(NoteError::BoundNoteRequired)));
        note(&mut m, &A, &C, 6, T + 1002);
        assert_eq!((total(&m, &A, &B), total(&m, &A, &C)), (5, 6));
    }

    #[test]
    fn set_offs_use_their_own_residue() {
        // 999 belongs to transfers and 997 to bound notes: a set-off signed at either is refused.
        let mut m = triangle();
        let digest = m.get_state().avl_root_digest;
        for ts in [T + 1, T - 1] {
            let e = [edge(&A, &B, 10, 5, ts), edge(&B, &C, 7, 5, ts), edge(&C, &A, 5, 5, ts)];
            assert!(matches!(m.set_off(&DenomKey::Token(TOKEN), ts, &e), Err(NoteError::InvalidSetOff(_))), "ts {}", ts);
        }
        unchanged(&m, &digest);
    }
}

/// Tracker v3 timestamp residues, bound notes and bound-only edges, for add_note
/// (POST /notes) and update_note: residues 995-999 are reserved; a note at
/// residue 997 is a bound note, applied only with the issuer's acknowledgment of
/// the edge's stored total; an edge a novation changed takes only bound notes.
#[cfg(test)]
mod bound_note_tests {
    use crate::persistence::NoteStorage;
    use crate::{DenomKey, IouNote, NoteError, NoteKey, PriorAck, TrackerStateManager};
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

    /// `signer`'s signature over the (issuer -> recipient) note message at `total` and `ts`.
    fn sign(signer: &[u8; 32], issuer: &[u8; 32], recipient: &[u8; 32], total: u64, ts: u64) -> [u8; 65] {
        let msg = crate::schnorr::signing_message(&pk(issuer), &pk(recipient), &denom(), total, ts, &NFT);
        crate::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
    }

    /// A's note to `recipient` at the cumulative `total` and `ts`.
    fn note(recipient: &[u8; 32], total: u64, ts: u64) -> IouNote {
        IouNote::create_and_sign(pk(recipient), total, ts, &A, &NFT, denom()).unwrap()
    }

    /// A prior acknowledgment of the A -> recipient total `prior` at `ts`, signed by `signer`.
    fn ack(signer: &[u8; 32], recipient: &[u8; 32], prior: u64, ts: u64, prior_total: Option<u64>) -> PriorAck {
        PriorAck { signature: sign(signer, &A, recipient, prior, ts), prior_total }
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

    /// A owes B 10, noted at 1000 ms (residue 000).
    fn setup() -> TrackerStateManager {
        let mut m = TrackerStateManager::new_with_temp_storage();
        add(&mut m, &B, 10, 1000, None).unwrap();
        m
    }

    #[test]
    fn reserved_residues_are_refused_and_ordinary_ones_accepted() {
        let mut m = setup(); // its note at residue 000 was accepted
        let digest = m.get_state().avl_root_digest;
        for ts in [1995, 1996, 1998, 1999] {
            let r = add(&mut m, &B, 20, ts, None);
            assert!(matches!(r, Err(NoteError::ReservedTimestamp)), "ts {}: {:?}", ts, r);
        }
        // 997 is the bound-note residue: without a prior acknowledgment it is refused as reserved.
        assert!(matches!(add(&mut m, &B, 20, 1997, None), Err(NoteError::ReservedTimestamp)));
        // A new edge's first note too.
        assert!(matches!(add(&mut m, &C, 5, 1999, None), Err(NoteError::ReservedTimestamp)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // The largest ordinary residue (994) and the smallest (000) are accepted.
        add(&mut m, &B, 20, 1994, None).unwrap();
        add(&mut m, &B, 30, 2000, None).unwrap();
        assert_eq!(total(&m, &B), Some(30));
    }

    #[test]
    fn the_existing_checks_keep_their_precedence() {
        let mut m = setup();
        // Contract range, future timestamp, past timestamp, no decrease and the note's own signature are all
        // checked before the residue and bound-note rules.
        assert!(matches!(
            add(&mut m, &B, i64::MAX as u64 + 1, 1999, None),
            Err(NoteError::AmountExceedsContractRange { .. })
        ));
        let future = now_ms() + 3_600_000;
        assert!(matches!(add(&mut m, &B, 20, future - future % 1000 + 995, None), Err(NoteError::FutureTimestamp)));
        assert!(matches!(add(&mut m, &B, 20, 998, None), Err(NoteError::PastTimestamp)));
        assert!(matches!(
            add(&mut m, &B, 5, 1997, Some(ack(&A, &B, 10, 1997, Some(10)))),
            Err(NoteError::DebtDecreaseNotPermitted { .. })
        ));
        let mut forged = note(&B, 20, 1999);
        forged.signature = note(&B, 21, 1999).signature;
        assert!(matches!(m.add_note(&pk(&A), &forged, None), Err(NoteError::InvalidSignature)));
        let mut forged = note(&B, 20, 1997);
        forged.signature = note(&B, 21, 1997).signature;
        assert!(matches!(m.add_note(&pk(&A), &forged, Some(ack(&A, &B, 10, 1997, Some(9)))), Err(NoteError::InvalidSignature)));
        assert_eq!(total(&m, &B), Some(10));
    }

    #[test]
    fn a_bound_note_applies_only_with_the_issuers_acknowledgment_of_the_current_total() {
        let mut m = setup(); // P = 10
        let digest = m.get_state().avl_root_digest;
        let ts = 1997;
        // A prior at a wrong total: StalePrior when its total is sent, InvalidPriorSignature when it is not.
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, 9, ts, Some(9)))), Err(NoteError::StalePrior)));
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, 9, ts, None))), Err(NoteError::InvalidPriorSignature)));
        // Claiming the right total does not help a signature over another total.
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, 9, ts, Some(10)))), Err(NoteError::InvalidPriorSignature)));
        // The acknowledgment is bound to the note's own timestamp.
        assert!(matches!(add(&mut m, &B, 15, ts, Some(ack(&A, &B, 10, 1994, Some(10)))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &B), Some(10));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // With the issuer's acknowledgment of the stored total the bound note applies.
        add(&mut m, &B, 15, ts, Some(ack(&A, &B, 10, ts, Some(10)))).unwrap();
        let stored = m.lookup_note(&pk(&A), &pk(&B), &denom()).unwrap();
        assert_eq!((stored.amount_collected, stored.timestamp), (15, ts));
        assert!(stored.verify_signature(&pk(&A)).is_ok());
        // prior_total is optional; the next bound note acknowledges the new total.
        add(&mut m, &B, 18, 2997, Some(ack(&A, &B, 15, 2997, None))).unwrap();
        assert_eq!(total(&m, &B), Some(18));
        // An acknowledgment of the old total is stale now.
        assert!(matches!(add(&mut m, &B, 25, 3997, Some(ack(&A, &B, 15, 3997, Some(15)))), Err(NoteError::StalePrior)));
        assert!(matches!(add(&mut m, &B, 25, 3997, Some(ack(&A, &B, 15, 3997, None))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &B), Some(18));
    }

    #[test]
    fn a_prior_signed_by_the_recipient_instead_of_the_issuer_is_refused() {
        let mut m = setup();
        // B signs the right message (A -> B at 10, at the note's timestamp); it is still not the issuer's
        // acknowledgment. A third party's signature is refused as well.
        assert!(matches!(add(&mut m, &B, 15, 1997, Some(ack(&B, &B, 10, 1997, Some(10)))), Err(NoteError::InvalidPriorSignature)));
        assert!(matches!(add(&mut m, &B, 15, 1997, Some(ack(&C, &B, 10, 1997, None))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &B), Some(10));
    }

    #[test]
    fn an_equal_total_is_not_an_increase() {
        let mut m = setup();
        assert!(matches!(add(&mut m, &B, 10, 1997, Some(ack(&A, &B, 10, 1997, Some(10)))), Err(NoteError::NotAnIncrease)));
        // On an absent edge P = 0, so a bound note of 0 is not an increase either.
        assert!(matches!(add(&mut m, &C, 0, 1997, Some(ack(&A, &C, 0, 1997, Some(0)))), Err(NoteError::NotAnIncrease)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
    }

    #[test]
    fn a_first_bound_note_on_an_absent_edge_acknowledges_zero() {
        let mut m = setup();
        // On an absent edge P = 0: an acknowledgment of any other total does not fit.
        assert!(matches!(add(&mut m, &C, 7, 1997, Some(ack(&A, &C, 10, 1997, Some(10)))), Err(NoteError::StalePrior)));
        assert!(matches!(add(&mut m, &C, 7, 1997, Some(ack(&A, &C, 10, 1997, None))), Err(NoteError::InvalidPriorSignature)));
        assert_eq!(total(&m, &C), None);
        add(&mut m, &C, 7, 1997, Some(ack(&A, &C, 0, 1997, Some(0)))).unwrap();
        assert_eq!(total(&m, &C), Some(7));
        assert_eq!(m.get_recipient_notes(&pk(&C)).unwrap().len(), 1);
        assert!(!bound_only(&m, &C), "a bound note does not make its edge bound-only");
    }

    #[test]
    fn the_prior_acknowledgment_itself_cannot_be_submitted_as_a_note() {
        // The acknowledgment is a valid note message at the stored total P with a residue-997 timestamp. As a
        // note it is refused: without a prior as a reserved residue, with itself as the prior as no increase.
        let mut m = setup();
        let digest = m.get_state().avl_root_digest;
        let prior = ack(&A, &B, 10, 1997, Some(10));
        let as_note = IouNote {
            recipient_pubkey: pk(&B),
            amount_collected: 10,
            amount_redeemed: 0,
            timestamp: 1997,
            signature: prior.signature,
            tracker_nft_id: NFT,
            denomination: denom(),
        };
        assert!(as_note.verify_signature(&pk(&A)).is_ok());
        assert!(matches!(m.add_note(&pk(&A), &as_note, None), Err(NoteError::ReservedTimestamp)));
        assert!(matches!(m.add_note(&pk(&A), &as_note, Some(prior)), Err(NoteError::NotAnIncrease)));
        // The same for a first bound note's acknowledgment of 0 on an absent edge.
        let zero = ack(&A, &C, 0, 1997, Some(0));
        let zero_note = IouNote { recipient_pubkey: pk(&C), amount_collected: 0, signature: zero.signature, ..as_note.clone() };
        assert!(zero_note.verify_signature(&pk(&A)).is_ok());
        assert!(matches!(m.add_note(&pk(&A), &zero_note, None), Err(NoteError::ReservedTimestamp)));
        assert!(matches!(m.add_note(&pk(&A), &zero_note, Some(zero)), Err(NoteError::NotAnIncrease)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        assert_eq!(m.get_state().avl_root_digest, digest);
    }

    #[test]
    fn a_prior_on_a_note_that_is_not_a_bound_note_is_an_invalid_request() {
        let mut m = setup();
        assert!(matches!(add(&mut m, &B, 20, 2000, Some(ack(&A, &B, 10, 2000, Some(10)))), Err(NoteError::InvalidRequest(_))));
        assert!(matches!(add(&mut m, &C, 5, 2994, Some(ack(&A, &C, 0, 2994, None))), Err(NoteError::InvalidRequest(_))));
        // At another operation's residue the reserved-residue refusal comes first.
        assert!(matches!(add(&mut m, &B, 20, 1999, Some(ack(&A, &B, 10, 1999, None))), Err(NoteError::ReservedTimestamp)));
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(10), None));
        // Without the prior the same ordinary note applies.
        add(&mut m, &B, 20, 2000, None).unwrap();
        assert_eq!(total(&m, &B), Some(20));
    }

    #[test]
    fn on_a_bound_only_edge_an_unbound_note_is_refused_and_a_bound_note_accepted() {
        let mut m = setup();
        add(&mut m, &C, 4, 1001, None).unwrap();
        mark_bound_only(&m, &B);
        assert!(bound_only(&m, &B) && !bound_only(&m, &C));
        let digest = m.get_state().avl_root_digest;
        for ts in [2000, 2500, 2994] {
            let r = add(&mut m, &B, 20, ts, None);
            assert!(matches!(r, Err(NoteError::BoundNoteRequired)), "ts {}: {:?}", ts, r);
        }
        // With a prior at an ordinary residue the bound-only refusal comes first.
        assert!(matches!(add(&mut m, &B, 20, 2000, Some(ack(&A, &B, 10, 2000, Some(10)))), Err(NoteError::BoundNoteRequired)));
        // Reserved residues stay reserved.
        assert!(matches!(add(&mut m, &B, 20, 2999, None), Err(NoteError::ReservedTimestamp)));
        assert_eq!(total(&m, &B), Some(10));
        assert_eq!(m.get_state().avl_root_digest, digest);
        // A bound note applies, and the edge stays bound-only.
        add(&mut m, &B, 20, 2997, Some(ack(&A, &B, 10, 2997, Some(10)))).unwrap();
        assert_eq!(total(&m, &B), Some(20));
        assert!(bound_only(&m, &B));
        assert!(matches!(add(&mut m, &B, 30, 3000, None), Err(NoteError::BoundNoteRequired)));
        // The other edge still takes ordinary notes.
        add(&mut m, &C, 9, 3000, None).unwrap();
        assert_eq!(total(&m, &C), Some(9));
    }

    #[test]
    fn bound_only_marks_survive_reopening_the_store() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut m = TrackerStateManager::new(dir.path());
            add(&mut m, &B, 10, 1000, None).unwrap();
            add(&mut m, &C, 4, 1001, None).unwrap();
            mark_bound_only(&m, &B);
        }
        let mut m = TrackerStateManager::new(dir.path());
        assert!(bound_only(&m, &B) && !bound_only(&m, &C));
        assert!(matches!(add(&mut m, &B, 20, 2000, None), Err(NoteError::BoundNoteRequired)));
        add(&mut m, &B, 20, 1997, Some(ack(&A, &B, 10, 1997, Some(10)))).unwrap();
        add(&mut m, &C, 5, 2000, None).unwrap();
        assert_eq!((total(&m, &B), total(&m, &C)), (Some(20), Some(5)));
    }

    #[test]
    fn update_note_refuses_reserved_residues_and_bound_only_edges() {
        let mut m = setup();
        // update_note takes no prior acknowledgment, so it refuses all of 995-999, 997 included.
        for ts in [1995, 1996, 1997, 1998, 1999] {
            let r = m.update_note(&pk(&A), &note(&B, 20, ts));
            assert!(matches!(r, Err(NoteError::ReservedTimestamp)), "ts {}: {:?}", ts, r);
        }
        assert_eq!(total(&m, &B), Some(10));
        m.update_note(&pk(&A), &note(&B, 20, 2000)).unwrap();
        assert_eq!(total(&m, &B), Some(20));
        mark_bound_only(&m, &B);
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 30, 3000)), Err(NoteError::BoundNoteRequired)));
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 30, 3997)), Err(NoteError::ReservedTimestamp)));
        // The existing refusals keep their priority.
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 30, 1500)), Err(NoteError::PastTimestamp)));
        assert!(matches!(m.update_note(&pk(&A), &note(&B, 5, 3000)), Err(NoteError::DebtDecreaseNotPermitted { .. })));
        assert_eq!(total(&m, &B), Some(20));
        // An edge that is not bound-only is unaffected.
        m.update_note(&pk(&A), &note(&C, 3, 3000)).unwrap();
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
}
