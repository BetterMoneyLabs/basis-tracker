// schnorr_roundtrip_test.rs
// Roundtrip test for Schnorr signature signing and verification

use secp256k1::{Secp256k1, SecretKey, PublicKey};

/// Create a message for signing following the format: recipient_pubkey || amount_be_bytes || timestamp_be_bytes
fn create_signing_message(recipient_pubkey: &[u8; 33], amount: u64, timestamp: u64) -> Vec<u8> {
    let mut message = Vec::new();
    message.extend_from_slice(recipient_pubkey);
    message.extend_from_slice(&amount.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message
}

fn main() {
    println!("Running Schnorr signature roundtrip test...");
    
    let secp = Secp256k1::new();
    
    // Generate a random private key for testing
    let secret_key = SecretKey::new(&mut rand::thread_rng());
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    
    // Create test data
    let recipient_pubkey = public_key.serialize(); // 33-byte compressed public key
    let amount = 1000000000u64; // 1 ERG in nanoERG
    let timestamp = 1672531200u64; // Example timestamp
    
    println!("Recipient pubkey: {}", hex::encode(&recipient_pubkey));
    println!("Amount: {}", amount);
    println!("Timestamp: {}", timestamp);
    
    // Create the message to be signed
    let message = create_signing_message(&recipient_pubkey, amount, timestamp);
    println!("Message to sign: {}", hex::encode(&message));
    
    // Use the actual basis_offchain schnorr implementation
    let signature = match basis_offchain::schnorr::schnorr_sign(&message, &secret_key, &recipient_pubkey) {
        Ok(sig) => {
            println!("Signature generated successfully!");
            println!("Signature (hex): {}", hex::encode(&sig));
            println!("Signature length: {} bytes", sig.len());
            
            if sig.len() != 65 {
                println!("ERROR: Signature is not 65 bytes as expected!");
                return;
            }
            
            sig
        },
        Err(e) => {
            println!("ERROR: Failed to sign message: {}", e);
            return;
        }
    };
    
    // Verify the signature using the actual basis_offchain verification
    match basis_offchain::schnorr::schnorr_verify(&signature, &message, &recipient_pubkey) {
        Ok(()) => {
            println!("SUCCESS: Signature verification passed!");
            println!("Roundtrip test completed successfully.");
        },
        Err(e) => {
            println!("ERROR: Signature verification failed: {}", e);
        }
    }
    
    // Test with a tampered message (should fail)
    println!("\nTesting with tampered message (should fail verification)...");
    let mut tampered_message = message.clone();
    tampered_message[0] ^= 0x01; // Flip one bit
    
    match basis_offchain::schnorr::schnorr_verify(&signature, &tampered_message, &recipient_pubkey) {
        Ok(()) => {
            println!("ERROR: Tampered message verification unexpectedly passed!");
        },
        Err(_) => {
            println!("SUCCESS: Tampered message correctly rejected.");
        }
    }
    
    // Test with wrong public key (should fail)
    println!("\nTesting with wrong public key (should fail verification)...");
    let wrong_secret_key = SecretKey::new(&mut rand::thread_rng());
    let wrong_public_key = PublicKey::from_secret_key(&secp, &wrong_secret_key);
    let wrong_pubkey_bytes = wrong_public_key.serialize();
    
    match basis_offchain::schnorr::schnorr_verify(&signature, &message, &wrong_pubkey_bytes) {
        Ok(()) => {
            println!("ERROR: Wrong public key verification unexpectedly passed!");
        },
        Err(_) => {
            println!("SUCCESS: Wrong public key correctly rejected.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey, PublicKey};

    #[test]
    fn test_schnorr_roundtrip() {
        let secp = Secp256k1::new();
        
        // Generate a random private key for testing
        let secret_key = SecretKey::new(&mut rand::thread_rng());
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        
        // Create test data
        let recipient_pubkey = public_key.serialize(); // 33-byte compressed public key
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp
        
        // Create the message to be signed
        let message = create_signing_message(&recipient_pubkey, amount, timestamp);
        
        // Sign the message using the actual implementation
        let signature = basis_offchain::schnorr::schnorr_sign(&message, &secret_key, &recipient_pubkey)
            .expect("Signing should succeed");
        
        // Verify the signature using the actual implementation
        basis_offchain::schnorr::schnorr_verify(&signature, &message, &recipient_pubkey)
            .expect("Verification should succeed");
        
        assert_eq!(signature.len(), 65, "Signature should be 65 bytes");
    }

    #[test]
    fn test_schnorr_with_tampered_message() {
        let secp = Secp256k1::new();
        
        // Generate a random private key for testing
        let secret_key = SecretKey::new(&mut rand::thread_rng());
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        
        // Create test data
        let recipient_pubkey = public_key.serialize(); // 33-byte compressed public key
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp
        
        // Create the message to be signed
        let message = create_signing_message(&recipient_pubkey, amount, timestamp);
        
        // Sign the message
        let signature = basis_offchain::schnorr::schnorr_sign(&message, &secret_key, &recipient_pubkey)
            .expect("Signing should succeed");
        
        // Tamper with the message
        let mut tampered_message = message.clone();
        tampered_message[0] ^= 0x01; // Flip one bit
        
        // Verify with tampered message (should fail)
        let result = basis_offchain::schnorr::schnorr_verify(&signature, &tampered_message, &recipient_pubkey);
        
        assert!(result.is_err(), "Verification should fail with tampered message");
    }

    #[test]
    fn test_schnorr_with_wrong_public_key() {
        let secp = Secp256k1::new();
        
        // Generate keys for first party
        let secret_key1 = SecretKey::new(&mut rand::thread_rng());
        let public_key1 = PublicKey::from_secret_key(&secp, &secret_key1);
        
        // Generate keys for second party
        let secret_key2 = SecretKey::new(&mut rand::thread_rng());
        let public_key2 = PublicKey::from_secret_key(&secp, &secret_key2);
        
        // Create test data
        let recipient_pubkey = public_key1.serialize(); // 33-byte compressed public key
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp
        
        // Create the message to be signed
        let message = create_signing_message(&recipient_pubkey, amount, timestamp);
        
        // Sign the message with first party's key
        let signature = basis_offchain::schnorr::schnorr_sign(&message, &secret_key1, &recipient_pubkey)
            .expect("Signing should succeed");
        
        // Verify with second party's public key (should fail)
        let wrong_pubkey_bytes = public_key2.serialize();
        let result = basis_offchain::schnorr::schnorr_verify(&signature, &message, &wrong_pubkey_bytes);
        
        assert!(result.is_err(), "Verification should fail with wrong public key");
    }
}