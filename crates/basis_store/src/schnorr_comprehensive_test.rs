//! Comprehensive tests for Schnorr signature implementation

#[cfg(test)]
use crate::schnorr;
#[cfg(test)]
use secp256k1::Secp256k1;

#[test]
fn test_comprehensive_schnorr_operations() {
    let secp = Secp256k1::new();

    // Generate multiple key pairs
    let (alice_secret, alice_pubkey) = schnorr::generate_keypair();
    let (bob_secret, bob_pubkey) = schnorr::generate_keypair();
    let (charlie_secret, charlie_pubkey) = schnorr::generate_keypair();

    // Test data for IOU notes
    let test_cases = vec![
        (alice_pubkey, 1000u64, 1234567890u64),
        (bob_pubkey, 5000u64, 1234567891u64),
        (charlie_pubkey, 2500u64, 1234567892u64),
    ];

    // Test signing and verification with different issuers
    for (recipient_pubkey, amount, timestamp) in test_cases {
        // Alice signs a note to recipient
        let message = schnorr::signing_message(&recipient_pubkey, amount, timestamp);
        let signature = schnorr::schnorr_sign(&message, &alice_secret, &alice_pubkey)
            .expect("Failed to create signature");

        // Verify with correct issuer
        assert!(schnorr::schnorr_verify(&signature, &message, &alice_pubkey).is_ok());

        // Should fail with wrong issuer
        assert!(schnorr::schnorr_verify(&signature, &message, &bob_pubkey).is_err());

        // Should fail with wrong message
        let wrong_message = schnorr::signing_message(&recipient_pubkey, amount + 1, timestamp);
        assert!(schnorr::schnorr_verify(&signature, &wrong_message, &alice_pubkey).is_err());
    }

    // Test signature format validation
    let message = schnorr::signing_message(&bob_pubkey, 1000, 1234567890);
    let signature = schnorr::schnorr_sign(&message, &alice_secret, &alice_pubkey)
        .expect("Failed to create signature");

    // Test valid signature format
    assert!(schnorr::validate_signature_format(&signature).is_ok());

    // Test corrupted signature format
    let mut corrupted_signature = signature;
    corrupted_signature[0] = 0x04; // Invalid compressed point prefix
    assert!(schnorr::validate_signature_format(&corrupted_signature).is_err());

    // Test hex conversion round-trip
    let hex_pubkey = schnorr::pubkey_to_hex(&alice_pubkey);
    let pubkey_from_hex =
        schnorr::pubkey_from_hex(&hex_pubkey).expect("Failed to parse hex pubkey");
    assert_eq!(alice_pubkey, pubkey_from_hex);

    let hex_signature = schnorr::signature_to_hex(&signature);
    let signature_from_hex =
        schnorr::signature_from_hex(&hex_signature).expect("Failed to parse hex signature");
    assert_eq!(signature, signature_from_hex);

    // Verify signature still works after hex conversion
    assert!(schnorr::schnorr_verify(&signature_from_hex, &message, &alice_pubkey).is_ok());
}

#[test]
fn test_edge_cases() {
    let (secret_key, pubkey) = schnorr::generate_keypair();

    // Test with empty message
    let empty_message = vec![];
    let signature = schnorr::schnorr_sign(&empty_message, &secret_key, &pubkey)
        .expect("Failed to sign empty message");
    assert!(schnorr::schnorr_verify(&signature, &empty_message, &pubkey).is_ok());

    // Test with very long message
    let long_message = vec![0x42u8; 1024];
    let signature = schnorr::schnorr_sign(&long_message, &secret_key, &pubkey)
        .expect("Failed to sign long message");
    assert!(schnorr::schnorr_verify(&signature, &long_message, &pubkey).is_ok());

    // Test invalid public key
    let invalid_pubkey = [0x04u8; 33]; // Invalid compressed prefix
    assert!(schnorr::validate_public_key(&invalid_pubkey).is_err());

    // Test invalid signature (wrong length)
    let short_signature = [0u8; 64];
    // Note: We can't directly test this since Signature is fixed at 65 bytes
}

#[test]
fn test_deterministic_signature_generation() {
    // This test ensures that the same input produces the same signature
    // (when using deterministic nonce generation, which we're not currently doing)

    let (secret_key, pubkey) = schnorr::generate_keypair();
    let recipient_pubkey = [0x02u8; 33];
    let amount = 1000u64;
    let timestamp = 1234567890u64;

    let message = schnorr::signing_message(&recipient_pubkey, amount, timestamp);

    // Create two signatures with the same input
    // Note: Since we use random nonces, these will be different
    let signature1 = schnorr::schnorr_sign(&message, &secret_key, &pubkey)
        .expect("Failed to create first signature");
    let signature2 = schnorr::schnorr_sign(&message, &secret_key, &pubkey)
        .expect("Failed to create second signature");

    // Both should verify correctly
    assert!(schnorr::schnorr_verify(&signature1, &message, &pubkey).is_ok());
    assert!(schnorr::schnorr_verify(&signature2, &message, &pubkey).is_ok());

    // But they should be different due to random nonce
    assert_ne!(signature1, signature2);
}

#[test]
fn test_public_key_validation() {
    // Test valid compressed public keys
    let (_, pubkey1) = schnorr::generate_keypair();
    let (_, pubkey2) = schnorr::generate_keypair();

    assert!(schnorr::validate_public_key(&pubkey1).is_ok());
    assert!(schnorr::validate_public_key(&pubkey2).is_ok());

    // Test invalid public keys
    let zero_pubkey = [0u8; 33];
    assert!(schnorr::validate_public_key(&zero_pubkey).is_err());

    let invalid_prefix_pubkey = {
        let mut key = pubkey1;
        key[0] = 0x04; // Invalid compressed prefix
        key
    };
    assert!(schnorr::validate_public_key(&invalid_prefix_pubkey).is_err());
}
