// Manual test runner functions

use crate::{simple_hash, IouNote, NoteKey};

pub fn run_all_tests() -> Result<(), String> {
    println!("Running Basis Store tests...");

    test_iou_note_creation()?;
    test_signing_message()?;
    test_note_key_generation()?;
    test_signature_verification()?;
    test_simple_hash_consistency()?;

    println!("All tests passed!");
    Ok(())
}

fn test_iou_note_creation() -> Result<(), String> {
    let recipient_pubkey = [1u8; 33];
    let signature = [2u8; 64];

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
    let note = IouNote::new([1u8; 33], 1000, 1234567890, [2u8; 64]);

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
    let mut note = IouNote::new(
        [1u8; 33], 1000, 1234567890, [0u8; 64], // Invalid signature (all zeros)
    );

    let issuer_pubkey = [1u8; 33];

    if note.verify_signature(&issuer_pubkey).is_ok() {
        return Err("should fail with invalid signature".to_string());
    }

    note.signature = [1u8; 64];
    if note.verify_signature(&issuer_pubkey).is_err() {
        return Err("should pass with non-zero signature".to_string());
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
    let signature = [3u8; 64];

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
}
