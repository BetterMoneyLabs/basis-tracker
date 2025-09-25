// Manual test runner functions

use crate::{simple_hash, IouNote, NoteKey, schnorr_tests};

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
    schnorr_tests::run_schnorr_test_vectors()?;

    println!("All tests passed!");
    Ok(())
}

fn test_iou_note_creation() -> Result<(), String> {
    let recipient_pubkey = [1u8; 33];
    let signature = [2u8; 65];

    let note = IouNote::new(recipient_pubkey, 1000, 1234567890, signature);

    if note.recipient_pubkey != recipient_pubkey {
        return Err("recipient_pubkey mismatch".to_string());
    }
    if note.amount != 1000 {
        return Err("amount mismatch".to_string());
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
    let note = IouNote::new([1u8; 33], 1000, 1234567890, [2u8; 65]);

    let message = note.signing_message();
    if message.is_empty() {
        return Err("signing message is empty".to_string());
    }

    if message.len() < 33 + 8 + 8 {
        return Err("signing message too short".to_string());
    }

    if &message[0..33] != &[1u8; 33] {
        return Err("pubkey not at start of message".to_string());
    }

    let amount_bytes = 1000u64.to_be_bytes();
    let timestamp_bytes = 1234567890u64.to_be_bytes();
    if !message.windows(8).any(|window| window == amount_bytes) {
        return Err("amount bytes not found in message".to_string());
    }
    if !message.windows(8).any(|window| window == timestamp_bytes) {
        return Err("timestamp bytes not found in message".to_string());
    }

    println!("✓ test_signing_message passed");
    Ok(())
}

fn test_note_key_generation() -> Result<(), String> {
    let issuer_pubkey = [1u8; 33];
    let recipient_pubkey = [2u8; 33];

    let note_key = NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey);

    if note_key.issuer_hash == note_key.recipient_hash {
        return Err("issuer and recipient hashes should be different".to_string());
    }

    let note_key2 = NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey);
    if note_key.issuer_hash != note_key2.issuer_hash {
        return Err("issuer hash should be consistent".to_string());
    }
    if note_key.recipient_hash != note_key2.recipient_hash {
        return Err("recipient hash should be consistent".to_string());
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
    let invalid_note = IouNote::new(recipient_pubkey, amount, timestamp, [0u8; 65]);
    
    if invalid_note.verify_signature(&issuer_pubkey).is_ok() {
        return Err("should fail with zero signature".to_string());
    }

    println!("✓ test_signature_verification passed");
    Ok(())
}

fn test_simple_hash_consistency() -> Result<(), String> {
    let data1 = [1u8; 33];
    let data2 = [2u8; 33];

    let hash1 = simple_hash(&data1);
    let hash2 = simple_hash(&data1);
    if hash1 != hash2 {
        return Err("same input should produce same hash".to_string());
    }

    let hash3 = simple_hash(&data2);
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
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    
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
    if note.amount != amount {
        return Err("amount mismatch".to_string());
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
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    
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
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
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
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    note3.amount = 2000;
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
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
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
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    
    let note2 = IouNote::create_and_sign(
        [2u8; 33],
        2000,
        1234567891,
        &secret_key2.secret_bytes(),
    ).map_err(|e| format!("Failed to create and sign note: {:?}", e))?;
    
    // Verify each note with its correct issuer
    note1.verify_signature(&issuer_pubkey1)
        .map_err(|e| format!("Note1 verification failed: {:?}", e))?;
    
    note2.verify_signature(&issuer_pubkey2)
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

fn test_note_persistence() -> Result<(), String> {
    use crate::persistence::NoteStorage;
    use tempfile::tempdir;

    // Create a temporary directory for testing
    let temp_dir = tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let db_path = temp_dir.path().join("test_db");

    // Create storage
    let storage =
        NoteStorage::open(&db_path).map_err(|e| format!("Failed to open storage: {:?}", e))?;

    // Create test note
    let issuer_pubkey = [1u8; 33];
    let recipient_pubkey = [2u8; 33];
    let signature = [3u8; 65];

    let note = IouNote::new(recipient_pubkey, 1000, 1234567890, signature);

    // Store note
    storage
        .store_note(&issuer_pubkey, &note)
        .map_err(|e| format!("Failed to store note: {:?}", e))?;

    // Retrieve note
    let retrieved_note = storage
        .get_note(&issuer_pubkey, &recipient_pubkey)
        .map_err(|e| format!("Failed to get note: {:?}", e))?
        .ok_or("Note should exist".to_string())?;

    if retrieved_note.recipient_pubkey != recipient_pubkey {
        return Err("recipient_pubkey mismatch".to_string());
    }
    if retrieved_note.amount != 1000 {
        return Err("amount mismatch".to_string());
    }
    if retrieved_note.timestamp != 1234567890 {
        return Err("timestamp mismatch".to_string());
    }
    if retrieved_note.signature != signature {
        return Err("signature mismatch".to_string());
    }

    // Test getting issuer notes
    let issuer_notes = storage
        .get_issuer_notes(&issuer_pubkey)
        .map_err(|e| format!("Failed to get issuer notes: {:?}", e))?;
    if issuer_notes.len() != 1 {
        return Err(format!("Expected 1 note, got {}", issuer_notes.len()));
    }
    if issuer_notes[0].amount != 1000 {
        return Err("issuer note amount mismatch".to_string());
    }

    // Test getting recipient notes
    let recipient_notes = storage
        .get_recipient_notes(&recipient_pubkey)
        .map_err(|e| format!("Failed to get recipient notes: {:?}", e))?;
    if recipient_notes.len() != 1 {
        return Err(format!("Expected 1 note, got {}", recipient_notes.len()));
    }
    if recipient_notes[0].amount != 1000 {
        return Err("recipient note amount mismatch".to_string());
    }

    // Test getting all notes
    let all_notes = storage
        .get_all_notes()
        .map_err(|e| format!("Failed to get all notes: {:?}", e))?;
    if all_notes.len() != 1 {
        return Err(format!("Expected 1 note, got {}", all_notes.len()));
    }
    if all_notes[0].amount != 1000 {
        return Err("all notes amount mismatch".to_string());
    }

    // Remove note
    storage
        .remove_note(&issuer_pubkey, &recipient_pubkey)
        .map_err(|e| format!("Failed to remove note: {:?}", e))?;

    // Verify note is gone
    let should_be_none = storage
        .get_note(&issuer_pubkey, &recipient_pubkey)
        .map_err(|e| format!("Failed to get note after removal: {:?}", e))?;
    if should_be_none.is_some() {
        return Err("Note should have been removed".to_string());
    }

    println!("✓ test_note_persistence passed");
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
    fn test_note_persistence() {
        super::test_note_persistence().unwrap();
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
}
