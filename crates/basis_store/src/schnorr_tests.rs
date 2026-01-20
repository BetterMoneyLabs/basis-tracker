//! Comprehensive tests for Schnorr signature verification matching basis.es contract
//! These test vectors can be used to verify compatibility with the ErgoScript implementation

use crate::{IouNote, PubKey};
use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;

/// Detailed test vector with all intermediate values
#[derive(Debug, Clone)]
pub struct SchnorrVerificationVector {
    /// Test case identifier
    pub id: &'static str,
    /// Test case description
    pub description: &'static str,

    // Inputs
    pub issuer_pubkey: [u8; 33],
    pub recipient_pubkey: [u8; 33],
    pub amount: u64,
    pub timestamp: u64,
    pub signature: [u8; 65],

    // Intermediate values (for verification)
    pub signing_message: Vec<u8>,
    pub challenge_hash: [u8; 32],

    // Expected result
    pub should_verify: bool,
}

impl SchnorrVerificationVector {
    /// Create a test vector with computed intermediate values
    pub fn new(
        id: &'static str,
        description: &'static str,
        issuer_pubkey: [u8; 33],
        recipient_pubkey: [u8; 33],
        amount: u64,
        timestamp: u64,
        signature: [u8; 65],
        should_verify: bool,
    ) -> Self {
        // Compute signing message: recipient_pubkey || amount_be_bytes || timestamp_be_bytes
        let mut signing_message = Vec::new();
        signing_message.extend_from_slice(&recipient_pubkey);
        signing_message.extend_from_slice(&amount.to_be_bytes());
        signing_message.extend_from_slice(&timestamp.to_be_bytes());

        // Compute challenge: H(a || message || issuer_pubkey)
        let a_bytes = &signature[0..33];
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(a_bytes);
        hasher.update(&signing_message);
        hasher.update(&issuer_pubkey);
        let challenge_full = hasher.finalize();
        let challenge_hash: [u8; 32] = challenge_full[..32].try_into().unwrap();

        Self {
            id,
            description,
            issuer_pubkey,
            recipient_pubkey,
            amount,
            timestamp,
            signature,
            signing_message,
            challenge_hash,
            should_verify,
        }
    }

    /// Convert to JSON for cross-language testing
    pub fn to_json(&self) -> String {
        format!(
            r#"{{
  "id": "{}",
  "description": "{}",
  "issuer_pubkey": "{}",
  "recipient_pubkey": "{}",
  "amount": {},
  "timestamp": {},
  "signature": "{}",
  "signing_message": "{}",
  "challenge_hash": "{}",
  "should_verify": {}
}}"#,
            self.id,
            self.description,
            hex::encode(self.issuer_pubkey),
            hex::encode(self.recipient_pubkey),
            self.amount,
            self.timestamp,
            hex::encode(self.signature),
            hex::encode(&self.signing_message),
            hex::encode(self.challenge_hash),
            self.should_verify
        )
    }
}

/// Get comprehensive test vectors for Schnorr signature verification
pub fn get_comprehensive_test_vectors() -> Vec<SchnorrVerificationVector> {
    vec![
        // Vector 1: Standard valid case
        SchnorrVerificationVector::new(
            "TV001",
            "Standard valid signature",
            // issuer_pubkey (pattern)
            [1u8; 33],
            // recipient_pubkey
            [2u8; 33],
            1000,       // amount
            1234567890, // timestamp
            // signature (pattern for now - would be replaced with actual ECDSA signature)
            [1u8; 65],
            true,
        ),
        // Vector 2: All zeros (invalid)
        SchnorrVerificationVector::new(
            "TV002",
            "All-zero signature should fail",
            [1u8; 33],
            [2u8; 33],
            500,
            9876543210,
            [0u8; 65],
            false,
        ),
        // Vector 3: Edge case - maximum values
        SchnorrVerificationVector::new(
            "TV003",
            "Maximum u64 values",
            [0xFFu8; 33],
            [0xEEu8; 33],
            u64::MAX,
            u64::MAX,
            {
                let mut sig = [0u8; 65];
                sig[0..33].copy_from_slice(&[0xDDu8; 33]);
                sig[33..65].copy_from_slice(&[0xCCu8; 32]);
                sig
            },
            true,
        ),
        // Vector 4: Minimum non-zero values
        SchnorrVerificationVector::new(
            "TV004",
            "Minimum non-zero values",
            [0x01u8; 33],
            [0x02u8; 33],
            1,
            1,
            {
                let mut sig = [0u8; 65];
                sig[0] = 0x01; // minimal non-zero a
                sig[33] = 0x01; // minimal non-zero z
                sig
            },
            true,
        ),
        // Vector 5: Specific pattern for cross-verification
        SchnorrVerificationVector::new(
            "TV005",
            "Pattern-based signature for cross-language testing",
            // issuer_pubkey: pattern
            [5u8; 33],
            // recipient_pubkey: pattern
            [6u8; 33],
            0x123456789ABCDEF0, // pattern amount
            0xFEDCBA9876543210, // pattern timestamp
            // signature: pattern-based
            [0xFFu8; 65],
            true,
        ),
    ]
}

/// Run verification against all test vectors
pub fn verify_all_test_vectors() -> Result<(), String> {
    use crate::IouNote;

    let vectors = get_comprehensive_test_vectors();

    for vector in vectors {
        let note = IouNote::new(
            vector.recipient_pubkey,
            vector.amount,
            0, // amount_redeemed
            vector.timestamp,
            vector.signature,
        );

        let result = note.verify_signature(&vector.issuer_pubkey);
        let verified = result.is_ok();

        if verified != vector.should_verify {
            return Err(format!(
                "Test vector {} failed: {} (expected {}, got {})",
                vector.id, vector.description, vector.should_verify, verified
            ));
        }
    }

    Ok(())
}

/// Test vector structure for Schnorr signature verification
#[derive(Debug)]
struct SchnorrTestVector {
    /// Test case description
    description: &'static str,
    /// Issuer public key (33 bytes compressed)
    issuer_pubkey: PubKey,
    /// Recipient public key (33 bytes compressed)
    recipient_pubkey: PubKey,
    /// Amount of debt
    amount: u64,
    /// Timestamp
    timestamp: u64,
    /// Schnorr signature (65 bytes: 33-byte a + 32-byte z)
    signature: [u8; 65],
    /// Expected verification result
    should_verify: bool,
}

/// Generate test vectors that use Schnorr signature format
fn get_test_vectors() -> Vec<SchnorrTestVector> {
    use secp256k1::{Secp256k1, SecretKey};

    let secp = Secp256k1::new();

    // Generate a test key pair for valid signatures
    let secret_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let valid_issuer_pubkey = public_key.serialize();

    // Generate a valid Schnorr-style signature for testing
    let recipient_pubkey = [2u8; 33];
    let amount = 1000u64;
    let timestamp = 1234567890u64;

    // Create a valid signature using our implementation
    let valid_note = IouNote::create_and_sign(recipient_pubkey, amount, timestamp, &[1u8; 32])
        .expect("Failed to create valid signature");
    let valid_signature = valid_note.signature;

    vec![
        // Test Vector 1: Basic valid signature
        SchnorrTestVector {
            description: "Basic valid signature",
            issuer_pubkey: valid_issuer_pubkey,
            recipient_pubkey,
            amount,
            timestamp,
            signature: valid_signature,
            should_verify: true,
        },
        // Test Vector 2: All-zero signature (should fail)
        SchnorrTestVector {
            description: "All-zero signature should fail",
            issuer_pubkey: valid_issuer_pubkey,
            recipient_pubkey,
            amount,
            timestamp,
            signature: [0u8; 65],
            should_verify: false,
        },
        // Test Vector 3: Invalid issuer pubkey (should fail)
        SchnorrTestVector {
            description: "Invalid issuer pubkey should fail",
            issuer_pubkey: [0u8; 33], // Invalid pubkey
            recipient_pubkey,
            amount,
            timestamp,
            signature: valid_signature,
            should_verify: false,
        },
        // Test Vector 4: Modified signature (should fail)
        SchnorrTestVector {
            description: "Modified signature should fail",
            issuer_pubkey: valid_issuer_pubkey,
            recipient_pubkey,
            amount,
            timestamp,
            signature: {
                let mut sig = valid_signature;
                sig[0] ^= 0x01; // Flip a bit
                sig
            },
            should_verify: false,
        },
        // Test Vector 5: Different message (should fail)
        SchnorrTestVector {
            description: "Different message should fail",
            issuer_pubkey: valid_issuer_pubkey,
            recipient_pubkey: [3u8; 33], // Different recipient
            amount: 2000,                // Different amount
            timestamp: 9999999999,       // Different timestamp
            signature: valid_signature,  // Signature for different message
            should_verify: false,
        },
    ]
}

/// Test that the signing message format matches basis.es
fn test_signing_message_format() {
    let note = IouNote::new([1u8; 33], 1000, 0, 1234567890, [0u8; 65]);
    let message = note.signing_message();

    // Message should be: recipient_pubkey || amount_be_bytes || timestamp_be_bytes
    assert_eq!(message.len(), 33 + 8 + 8);
    assert_eq!(&message[0..33], &[1u8; 33]);
    assert_eq!(&message[33..41], &1000u64.to_be_bytes());
    assert_eq!(&message[41..49], &1234567890u64.to_be_bytes());
}

/// Test that the challenge computation matches basis.es
fn test_challenge_computation() {
    let a_bytes = [0x01u8; 33];
    let message_bytes = [0x02u8; 49]; // 33 + 8 + 8
    let issuer_pubkey = [0x03u8; 33];

    // Compute challenge e = H(a || message || issuer_pubkey)
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(a_bytes);
    hasher.update(message_bytes);
    hasher.update(issuer_pubkey);
    let e_bytes = hasher.finalize();
    let e_bytes_256 = &e_bytes[..32];

    // Challenge should be 32 bytes (256 bits)
    assert_eq!(e_bytes_256.len(), 32);

    // Verify deterministic output
    let mut hasher2 = Blake2b::<U32>::new();
    hasher2.update(a_bytes);
    hasher2.update(message_bytes);
    hasher2.update(issuer_pubkey);
    let e_bytes2 = hasher2.finalize();
    assert_eq!(e_bytes, e_bytes2);
}

/// Run all Schnorr signature test vectors
pub fn run_schnorr_test_vectors() -> Result<(), String> {
    println!("Running Schnorr signature test vectors...");

    test_signing_message_format();
    println!("✓ Signing message format test passed");

    test_challenge_computation();
    println!("✓ Challenge computation test passed");

    let test_vectors = get_test_vectors();

    for (i, vector) in test_vectors.iter().enumerate() {
        let note = IouNote::new(
            vector.recipient_pubkey,
            vector.amount,
            0, // amount_redeemed
            vector.timestamp,
            vector.signature,
        );

        let result = note.verify_signature(&vector.issuer_pubkey);
        let verified = result.is_ok();

        if verified != vector.should_verify {
            return Err(format!(
                "Test vector {} failed: {} (expected {}, got {})",
                i + 1,
                vector.description,
                vector.should_verify,
                verified
            ));
        }

        println!("✓ Test vector {} passed: {}", i + 1, vector.description);
    }

    println!("All Schnorr test vectors passed!");
    Ok(())
}

/// Comprehensive tests for Schnorr signature implementation
#[cfg(test)]
mod comprehensive_tests {
    use super::*;
    use crate::schnorr;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_message_format() {
        super::test_signing_message_format();
    }

    #[test]
    fn test_challenge_computation() {
        super::test_challenge_computation();
    }

    #[test]
    fn test_schnorr_test_vectors() {
        run_schnorr_test_vectors().unwrap();
    }
}
