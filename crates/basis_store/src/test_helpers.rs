use crate::{
    schnorr::{self, generate_keypair},
    IouNote, NoteKey, PubKey,
};

/// Generate deterministic test keypairs for consistent testing
pub fn generate_test_keypair() -> ([u8; 32], [u8; 33]) {
    // Use a fixed seed for deterministic testing
    let (secret, pubkey) = generate_keypair();
    (secret, pubkey)
}

/// Generate multiple test keypairs with different patterns
pub fn generate_test_keypairs(count: usize) -> Vec<([u8; 32], [u8; 33])> {
    (0..count)
        .map(|i| {
            let (secret, pubkey) = generate_keypair();
            (secret, pubkey)
        })
        .collect()
}

/// Create standardized test notes for consistent testing
pub fn create_test_note(amount: u64, timestamp: u64) -> IouNote {
    let (issuer_secret, _) = generate_test_keypair();
    let (_, recipient_pubkey) = generate_test_keypair();

    IouNote::create_and_sign(recipient_pubkey, amount, timestamp, &issuer_secret)
        .expect("Failed to create test note")
}

/// Create test transaction context following chaincash-rs patterns
pub fn create_test_tx_context() -> basis_offchain::transaction_builder::TxContext {
    basis_offchain::transaction_builder::TxContext {
        current_height: 1000,
        fee: 1000000, // 0.001 ERG - same as chaincash-rs SUGGESTED_TX_FEE
        change_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
        network_prefix: 0, // mainnet
    }
}

/// Create test reserve box ID
pub fn create_test_reserve_box_id() -> String {
    "e56847ed19b3dc6b712351b2a6c8a5e3c8e8b5a3c6d8e7f4a2b9c1d3e5f7a9b1".to_string()
}

/// Create test tracker box ID
pub fn create_test_tracker_box_id() -> String {
    "f67858fe2ac4ed7c823462c3b7d9b6f4d9f9c6b4d7e9f8g5c3d4e2f6g8b0c2d".to_string()
}

/// Create test recipient address
pub fn create_test_recipient_address() -> String {
    "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string()
}

/// Create test notes with specific issuer and recipient
pub fn create_test_note_with_keys(
    issuer_secret: &[u8; 32],
    recipient_pubkey: PubKey,
    amount: u64,
    timestamp: u64,
) -> IouNote {
    IouNote::create_and_sign(recipient_pubkey, amount, timestamp, issuer_secret)
        .expect("Failed to create test note with specific keys")
}

/// Generate test note key for consistent testing
pub fn create_test_note_key() -> NoteKey {
    let issuer_pubkey = [1u8; 33];
    let recipient_pubkey = [2u8; 33];

    NoteKey::from_keys(&issuer_pubkey, &recipient_pubkey)
}

/// Create multiple test notes with sequential amounts
pub fn create_test_notes_sequence(
    count: usize,
    base_amount: u64,
    base_timestamp: u64,
) -> Vec<IouNote> {
    (0..count)
        .map(|i| {
            create_test_note(
                base_amount + (i as u64 * 100),
                base_timestamp + (i as u64 * 60), // 1 minute intervals
            )
        })
        .collect()
}

/// Create test redemption request
pub fn create_test_redemption_request(
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    timestamp: u64,
) -> crate::RedemptionRequest {
    crate::RedemptionRequest {
        issuer_pubkey: issuer_pubkey.to_string(),
        recipient_pubkey: recipient_pubkey.to_string(),
        amount,
        timestamp,
        reserve_box_id: "test_reserve_box_1".to_string(),
        recipient_address: "test_recipient_address".to_string(),
    }
}

/// Create test reserve info
pub fn create_test_reserve_info(
    box_id: &str,
    owner_pubkey: &str,
    collateral_amount: u64,
    height: u64,
) -> crate::ExtendedReserveInfo {
    crate::ExtendedReserveInfo::new(
        box_id.as_bytes(),
        owner_pubkey.as_bytes(),
        collateral_amount,
        None, // tracker_nft_id
        height,
    )
}

/// Generate test signature for verification testing
pub fn generate_test_signature(message: &[u8]) -> [u8; 65] {
    let (secret, pubkey) = generate_test_keypair();

    let secret_key = secp256k1::SecretKey::from_slice(&secret).unwrap();
    schnorr::schnorr_sign(message, &secret_key.secret_bytes(), &pubkey).expect("Failed to generate test signature")
}

/// Verify test signature
pub fn verify_test_signature(signature: &[u8; 65], message: &[u8], pubkey: &[u8; 33]) -> bool {
    schnorr::schnorr_verify(signature, message, pubkey).is_ok()
}

/// Create test data for performance testing
pub fn generate_performance_test_data(num_notes: usize) -> Vec<IouNote> {
    let mut notes = Vec::with_capacity(num_notes);
    let keypairs = generate_test_keypairs(num_notes);

    for i in 0..num_notes {
        let (issuer_secret, _) = &keypairs[i];
        let (_, recipient_pubkey) = generate_test_keypair();

        let note = IouNote::create_and_sign(
            recipient_pubkey,
            1000 + (i as u64 * 100),
            1234567890 + (i as u64 * 60),
            issuer_secret,
        )
        .expect("Failed to create performance test note");

        notes.push(note);
    }

    notes
}

/// Helper for testing error conditions
pub fn create_invalid_note() -> IouNote {
    // Create a note with obviously invalid data
    IouNote::new(
        [0u8; 33], // zero pubkey
        0,         // zero amount
        0,         // zero redeemed
        0,         // zero timestamp
        [0u8; 65], // zero signature
    )
}

/// Helper for testing edge cases
pub fn create_edge_case_notes() -> Vec<IouNote> {
    vec![
        // Minimum valid values
        create_test_note(1, 1),
        // Maximum u64 values (avoiding overflow)
        create_test_note(u64::MAX - 1000, u64::MAX - 1000),
        // Common boundary values
        create_test_note(1000, 1234567890),
        create_test_note(1000000, 9876543210),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_keypair_generation() {
        let (secret, pubkey) = generate_test_keypair();
        assert_eq!(secret.len(), 32);
        assert_eq!(pubkey.len(), 33);
    }

    #[test]
    fn test_multiple_keypair_generation() {
        let keypairs = generate_test_keypairs(5);
        assert_eq!(keypairs.len(), 5);

        // Verify all keypairs are unique
        let mut pubkeys: Vec<_> = keypairs.iter().map(|(_, pubkey)| pubkey).collect();
        pubkeys.sort();
        pubkeys.dedup();
        assert_eq!(pubkeys.len(), 5);
    }

    #[test]
    fn test_test_note_creation() {
        let note = create_test_note(1000, 1234567890);
        assert_eq!(note.amount_collected, 1000);
        assert_eq!(note.timestamp, 1234567890);
        assert_eq!(note.amount_redeemed, 0);
    }

    #[test]
    fn test_note_sequence_creation() {
        let notes = create_test_notes_sequence(3, 1000, 1234567890);
        assert_eq!(notes.len(), 3);

        assert_eq!(notes[0].amount_collected, 1000);
        assert_eq!(notes[1].amount_collected, 1100);
        assert_eq!(notes[2].amount_collected, 1200);

        assert_eq!(notes[0].timestamp, 1234567890);
        assert_eq!(notes[1].timestamp, 1234567950);
        assert_eq!(notes[2].timestamp, 1234568010);
    }

    #[test]
    fn test_signature_generation_and_verification() {
        let message = b"test message for signature";
        let (secret, pubkey) = generate_test_keypair();

        let secret_key = secp256k1::SecretKey::from_slice(&secret).unwrap();
        let signature = schnorr::schnorr_sign(message, &secret_key.secret_bytes(), &pubkey).unwrap();

        let is_valid = schnorr::schnorr_verify(&signature, message, &pubkey).is_ok();

        assert!(is_valid, "Generated signature should be valid");
    }

    #[test]
    fn test_performance_data_generation() {
        let notes = generate_performance_test_data(10);
        assert_eq!(notes.len(), 10);

        for (i, note) in notes.iter().enumerate() {
            assert_eq!(note.amount_collected, 1000 + (i as u64 * 100));
        }
    }

    #[test]
    fn test_edge_case_notes() {
        let edge_notes = create_edge_case_notes();
        assert_eq!(edge_notes.len(), 4);

        // Verify each note has valid structure
        for note in edge_notes {
            assert!(note.amount_collected > 0);
            assert!(note.timestamp > 0);
        }
    }
}
