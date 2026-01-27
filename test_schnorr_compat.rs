use basis_core::impls::{schnorr_verify, schnorr_sign, generate_keypair};
use basis_core::types::{PubKey, Signature};
use secp256k1::{Secp256k1, SecretKey};

fn main() {
    // Test with a properly generated signature using the same algorithm
    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key_obj = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let issuer_pubkey = public_key_obj.serialize();

    // Generate a test message in the same format as chaincash-rs
    let recipient_pubkey = [0x02u8; 33];
    let amount = 1000u64;
    let timestamp = 1672531200u64;

    let signing_message = basis_core::types::signing_message(&recipient_pubkey, amount, timestamp);

    // Create a signature using the canonical implementation
    let signature = schnorr_sign(&signing_message, &secret_key.secret_bytes(), &issuer_pubkey)
        .expect("Failed to create signature");

    println!("Generated signature: {:?}", &signature[..10]); // Print first 10 bytes
    println!("Signature length: {}", signature.len());
    println!("Issuer pubkey: {:?}", &issuer_pubkey[..5]); // Print first 5 bytes

    // Verify the signature
    match schnorr_verify(&signature, &signing_message, &issuer_pubkey) {
        Ok(()) => println!("✅ Round-trip signature verification succeeded!"),
        Err(e) => println!("❌ Round-trip signature verification failed: {:?}", e),
    }

    // Test with a tampered signature (should fail)
    let mut tampered_signature = signature;
    tampered_signature[50] ^= 0x01; // Flip one bit

    match schnorr_verify(&tampered_signature, &signing_message, &issuer_pubkey) {
        Ok(()) => println!("❌ Tampered signature verification unexpectedly succeeded!"),
        Err(_) => println!("✅ Tampered signature verification correctly failed!"),
    }

    // Test with wrong public key (should fail)
    let (_, wrong_pubkey) = generate_keypair();
    match schnorr_verify(&signature, &signing_message, &wrong_pubkey) {
        Ok(()) => println!("❌ Wrong public key verification unexpectedly succeeded!"),
        Err(_) => println!("✅ Wrong public key verification correctly failed!"),
    }

    // Test with wrong message (should fail) - need to recreate message with different values
    let wrong_recipient_pubkey = [0x03u8; 33]; // Different prefix
    let wrong_amount = amount + 1;
    let wrong_message = basis_core::types::signing_message(&wrong_recipient_pubkey, wrong_amount, timestamp);
    match schnorr_verify(&signature, &wrong_message, &issuer_pubkey) {
        Ok(()) => println!("❌ Wrong message verification unexpectedly succeeded!"),
        Err(_) => println!("✅ Wrong message verification correctly failed!"),
    }

    println!("\nAll compatibility tests completed!");
}