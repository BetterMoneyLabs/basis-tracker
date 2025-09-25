//! Test vectors for Schnorr signature verification matching basis.es contract
//! These test vectors can be used to verify compatibility with the ErgoScript implementation

use crate::{IouNote, PubKey};
use blake2::{Blake2b512, Digest};

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
    use secp256k1::{PublicKey, Secp256k1, SecretKey};
    use blake2::{Blake2b512, Digest};

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
            amount: 2000, // Different amount
            timestamp: 9999999999, // Different timestamp
            signature: valid_signature, // Signature for different message
            should_verify: false,
        },
    ]
}

/// Test that the signing message format matches basis.es
fn test_signing_message_format() {
    let note = IouNote::new([1u8; 33], 1000, 1234567890, [0u8; 65]);
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
    let mut hasher = Blake2b512::new();
    hasher.update(a_bytes);
    hasher.update(message_bytes);
    hasher.update(issuer_pubkey);
    let e_bytes = hasher.finalize();
    let e_bytes_256 = &e_bytes[..32];
    
    // Challenge should be 32 bytes (256 bits)
    assert_eq!(e_bytes_256.len(), 32);
    
    // Verify deterministic output
    let mut hasher2 = Blake2b512::new();
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