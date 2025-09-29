//! Schnorr signature implementation following chaincash-rs approach

use crate::{NoteError, PubKey, Signature};
use blake2::{Blake2b512, Digest};
use secp256k1::{self, PublicKey, Scalar, SecretKey};
use std::convert::TryInto;

/// Generate the signing message in the same format as chaincash-rs
pub fn signing_message(recipient_pubkey: &PubKey, amount: u64, timestamp: u64) -> Vec<u8> {
    let mut message = Vec::new();
    message.extend_from_slice(recipient_pubkey);
    message.extend_from_slice(&amount.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message
}

/// Validate that a public key is a valid compressed secp256k1 point
pub fn validate_public_key(pubkey: &PubKey) -> Result<(), NoteError> {
    use secp256k1::PublicKey;

    // Check if the first byte indicates a compressed point (0x02 or 0x03)
    if pubkey[0] != 0x02 && pubkey[0] != 0x03 {
        return Err(NoteError::InvalidSignature);
    }

    // Try to parse the public key to ensure it's valid
    PublicKey::from_slice(pubkey)
        .map(|_| ())
        .map_err(|_| NoteError::InvalidSignature)
}

/// Validate that a signature has the correct format (33-byte a + 32-byte z)
pub fn validate_signature_format(signature: &Signature) -> Result<(), NoteError> {
    // Check that the signature is exactly 65 bytes
    if signature.len() != 65 {
        return Err(NoteError::InvalidSignature);
    }

    // Check that the a component (first 33 bytes) is a valid compressed point
    let a_bytes = &signature[0..33];
    if a_bytes[0] != 0x02 && a_bytes[0] != 0x03 {
        return Err(NoteError::InvalidSignature);
    }

    // Check that the z component (last 32 bytes) is not all zeros
    let z_bytes = &signature[33..65];
    if z_bytes.iter().all(|&b| b == 0) {
        return Err(NoteError::InvalidSignature);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};

    #[test]
    fn test_validate_public_key() {
        // Test valid compressed public key
        let secp = Secp256k1::new();
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let pubkey_bytes = public_key.serialize();

        assert!(validate_public_key(&pubkey_bytes).is_ok());

        // Test invalid public key (wrong prefix)
        let mut invalid_pubkey = pubkey_bytes;
        invalid_pubkey[0] = 0x04; // Uncompressed prefix
        assert!(validate_public_key(&invalid_pubkey).is_err());

        // Test invalid public key (all zeros)
        let zero_pubkey = [0u8; 33];
        assert!(validate_public_key(&zero_pubkey).is_err());
    }

    #[test]
    fn test_validate_signature_format() {
        // Test valid signature format
        let mut valid_signature = [0u8; 65];
        valid_signature[0] = 0x02; // Valid compressed point prefix
        valid_signature[33] = 0x01; // Non-zero z component

        assert!(validate_signature_format(&valid_signature).is_ok());

        // Test invalid signature (wrong length)
        let _short_signature = [0u8; 64];
        // Note: We can't directly test this since Signature is fixed at 65 bytes

        // Test invalid signature (invalid a prefix)
        let mut invalid_signature = valid_signature;
        invalid_signature[0] = 0x04; // Invalid prefix
        assert!(validate_signature_format(&invalid_signature).is_err());

        // Test invalid signature (all-zero z)
        let mut zero_z_signature = valid_signature;
        zero_z_signature[33..65].fill(0);
        assert!(validate_signature_format(&zero_z_signature).is_err());
    }

    #[test]
    fn test_signing_message_format() {
        let recipient_pubkey = [0x02u8; 33];
        let amount = 1000u64;
        let timestamp = 1234567890u64;

        let message = signing_message(&recipient_pubkey, amount, timestamp);

        // Check message length
        assert_eq!(message.len(), 33 + 8 + 8); // pubkey + amount + timestamp

        // Check message content
        assert_eq!(&message[0..33], &recipient_pubkey);
        assert_eq!(&message[33..41], &amount.to_be_bytes());
        assert_eq!(&message[41..49], &timestamp.to_be_bytes());
    }

    #[test]
    fn test_roundtrip_signature() {
        let secp = Secp256k1::new();

        // Generate test key pair
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = public_key.serialize();

        // Test data
        let recipient_pubkey = [0x02u8; 33];
        let amount = 1000u64;
        let timestamp = 1234567890u64;

        // Generate signing message
        let message = signing_message(&recipient_pubkey, amount, timestamp);

        // Create signature
        let signature = schnorr_sign(&message, &secret_key, &issuer_pubkey)
            .expect("Failed to create signature");

        // Verify signature
        assert!(schnorr_verify(&signature, &message, &issuer_pubkey).is_ok());

        // Test with wrong message
        let wrong_message = signing_message(&recipient_pubkey, amount + 1, timestamp);
        assert!(schnorr_verify(&signature, &wrong_message, &issuer_pubkey).is_err());

        // Test with wrong issuer
        let wrong_secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let wrong_public_key = secp256k1::PublicKey::from_secret_key(&secp, &wrong_secret_key);
        let wrong_issuer_pubkey = wrong_public_key.serialize();
        assert!(schnorr_verify(&signature, &message, &wrong_issuer_pubkey).is_err());

        // Test with corrupted signature
        let mut corrupted_signature = signature;
        corrupted_signature[50] ^= 0x01; // Flip one bit
        assert!(schnorr_verify(&corrupted_signature, &message, &issuer_pubkey).is_err());
    }

    #[test]
    fn test_compute_challenge() {
        let a_bytes = [0x02u8; 33];
        let message = b"test message";
        let issuer_pubkey = [0x03u8; 33];

        let challenge = compute_challenge(&a_bytes, message, &issuer_pubkey)
            .expect("Failed to compute challenge");

        // Challenge should be a valid scalar
        assert!(challenge.to_be_bytes().iter().any(|&b| b != 0));
    }

    #[test]
    fn test_key_generation() {
        let (_secret_key, pubkey) = generate_keypair();

        // Validate the generated public key
        assert!(validate_public_key(&pubkey).is_ok());

        // Test hex conversion
        let hex_pubkey = pubkey_to_hex(&pubkey);
        let pubkey_from_hex = pubkey_from_hex(&hex_pubkey).expect("Failed to parse hex pubkey");
        assert_eq!(pubkey, pubkey_from_hex);
    }

    #[test]
    fn test_signature_hex_conversion() {
        let (secret_key, pubkey) = generate_keypair();
        let message = b"test message for hex conversion";

        let signature =
            schnorr_sign(message, &secret_key, &pubkey).expect("Failed to create signature");

        // Test hex conversion
        let hex_signature = signature_to_hex(&signature);
        let signature_from_hex =
            signature_from_hex(&hex_signature).expect("Failed to parse hex signature");

        assert_eq!(signature, signature_from_hex);

        // Verify the signature still works after hex conversion
        assert!(schnorr_verify(&signature_from_hex, message, &pubkey).is_ok());
    }
}

/// Compute the challenge e = H(a || message || issuer_pubkey)
fn compute_challenge(
    a_bytes: &[u8],
    message: &[u8],
    issuer_pubkey: &PubKey,
) -> Result<Scalar, NoteError> {
    let mut hasher = Blake2b512::new();
    hasher.update(a_bytes);
    hasher.update(message);
    hasher.update(issuer_pubkey);
    let e_bytes = hasher.finalize();

    // Take first 32 bytes for the scalar
    let e_bytes_32: [u8; 32] = e_bytes[..32]
        .try_into()
        .map_err(|_| NoteError::InvalidSignature)?;

    Scalar::from_be_bytes(e_bytes_32).map_err(|_| NoteError::InvalidSignature)
}

/// Generate a new key pair for testing and development
pub fn generate_keypair() -> (SecretKey, PubKey) {
    let secp = secp256k1::Secp256k1::new();
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let pubkey_bytes = public_key.serialize();

    (secret_key, pubkey_bytes)
}

/// Convert a hex string to a public key
pub fn pubkey_from_hex(hex_str: &str) -> Result<PubKey, NoteError> {
    let bytes = hex::decode(hex_str).map_err(|_| NoteError::InvalidSignature)?;

    if bytes.len() != 33 {
        return Err(NoteError::InvalidSignature);
    }

    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&bytes);

    validate_public_key(&pubkey)?;
    Ok(pubkey)
}

/// Convert a public key to a hex string
pub fn pubkey_to_hex(pubkey: &PubKey) -> String {
    hex::encode(pubkey)
}

/// Convert a signature to a hex string
pub fn signature_to_hex(signature: &Signature) -> String {
    hex::encode(signature)
}

/// Convert a hex string to a signature
pub fn signature_from_hex(hex_str: &str) -> Result<Signature, NoteError> {
    let bytes = hex::decode(hex_str).map_err(|_| NoteError::InvalidSignature)?;

    if bytes.len() != 65 {
        return Err(NoteError::InvalidSignature);
    }

    let mut signature = [0u8; 65];
    signature.copy_from_slice(&bytes);

    validate_signature_format(&signature)?;
    Ok(signature)
}

/// Schnorr signature implementation following chaincash-rs approach
pub fn schnorr_sign(
    message: &[u8],
    secret_key: &secp256k1::SecretKey,
    issuer_pubkey: &PubKey,
) -> Result<Signature, NoteError> {
    use secp256k1::Secp256k1;

    let secp = Secp256k1::new();

    // Generate a random nonce for the Schnorr signature
    let nonce_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let a_point = secp256k1::PublicKey::from_secret_key(&secp, &nonce_secret);
    let a_bytes = a_point.serialize();

    // Compute challenge e = H(a || message || issuer_pubkey)
    let e_scalar = compute_challenge(&a_bytes, message, issuer_pubkey)?;

    // Convert scalars to their big integer representations for modular arithmetic
    let k_big = num_bigint::BigUint::from_bytes_be(&nonce_secret.secret_bytes());
    let s_big = num_bigint::BigUint::from_bytes_be(&secret_key.secret_bytes());
    let e_big = num_bigint::BigUint::from_bytes_be(&e_scalar.to_be_bytes());

    // Curve order for secp256k1
    let n = num_bigint::BigUint::from_bytes_be(&[
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36,
        0x41, 0x41,
    ]);

    // Compute z = k + e * s (mod n)
    let e_times_s = (&e_big * &s_big) % &n;
    let z_big = (&k_big + &e_times_s) % &n;

    // Ensure z is in the valid range [1, n-1]
    if z_big == num_bigint::BigUint::from(0u32) || z_big >= n {
        return Err(NoteError::InvalidSignature);
    }

    // Convert back to scalar
    let z_vec = z_big.to_bytes_be();
    let mut z_bytes = [0u8; 32];
    if z_vec.len() > 32 {
        z_bytes.copy_from_slice(&z_vec[z_vec.len() - 32..]);
    } else if z_vec.len() < 32 {
        let start_idx = 32 - z_vec.len();
        z_bytes[start_idx..].copy_from_slice(&z_vec);
    } else {
        z_bytes.copy_from_slice(&z_vec);
    }

    let z_scalar =
        secp256k1::Scalar::from_be_bytes(z_bytes).map_err(|_| NoteError::InvalidSignature)?;

    // Convert z to bytes (32 bytes for Schnorr signatures - following chaincash-rs)
    let z_bytes_full = z_scalar.to_be_bytes();

    // Create the signature (a || z) - 33 bytes for a, 32 bytes for z
    let mut signature = [0u8; 65];
    signature[0..33].copy_from_slice(&a_bytes);
    signature[33..65].copy_from_slice(&z_bytes_full);

    Ok(signature)
}

/// Schnorr signature verification following chaincash-rs approach
pub fn schnorr_verify(
    signature: &Signature,
    message: &[u8],
    issuer_pubkey: &PubKey,
) -> Result<(), NoteError> {
    use secp256k1::Secp256k1;

    let secp = Secp256k1::new();

    // Validate signature format first
    validate_signature_format(signature)?;

    // Validate issuer public key
    validate_public_key(issuer_pubkey)?;

    // Split signature into components (a, z)
    let a_bytes = &signature[0..33];
    let z_bytes = &signature[33..65];

    // Parse the compressed public key (issuer key)
    let issuer_key =
        PublicKey::from_slice(issuer_pubkey).map_err(|_| NoteError::InvalidSignature)?;

    // Parse random point a from signature (33 bytes compressed point)
    let a_point = PublicKey::from_slice(a_bytes).map_err(|_| NoteError::InvalidSignature)?;

    // Compute challenge e = H(a || message || issuer_pubkey)
    let e_scalar = compute_challenge(a_bytes, message, issuer_pubkey)?;

    // Parse z as a scalar (32 bytes)
    let z_scalar = SecretKey::from_slice(z_bytes).map_err(|_| NoteError::InvalidSignature)?;

    // Compute g^z (generator point raised to z power)
    let g_z = secp256k1::PublicKey::from_secret_key(&secp, &z_scalar);

    // Compute x^e (issuer public key raised to e power)
    let x_e_tweak = issuer_key
        .mul_tweak(&secp, &e_scalar)
        .map_err(|_| NoteError::InvalidSignature)?;

    // Compute a * x^e (point addition)
    let a_x_e = a_point
        .combine(&x_e_tweak)
        .map_err(|_| NoteError::InvalidSignature)?;

    // Verify g^z == a * x^e
    if g_z != a_x_e {
        return Err(NoteError::InvalidSignature);
    }

    Ok(())
}
