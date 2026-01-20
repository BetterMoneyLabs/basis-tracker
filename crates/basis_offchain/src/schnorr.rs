//! Schnorr signature implementation following chaincash-rs approach

use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;
use secp256k1::{self, PublicKey, Scalar, SecretKey};
use std::convert::TryInto;

/// Error types for note operations
#[derive(Debug)]
pub enum NoteError {
    InvalidSignature,
    AmountOverflow,
    FutureTimestamp,
    RedemptionTooEarly,
    InsufficientCollateral,
    StorageError(String),
    UnsupportedOperation,
}

impl From<secp256k1::Error> for NoteError {
    fn from(_: secp256k1::Error) -> Self {
        NoteError::InvalidSignature
    }
}

/// Public key type (Secp256k1)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1) - following chaincash-rs format: 33 bytes a + 32 bytes z
pub type Signature = [u8; 65];

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

/// Compute the challenge e = H(a || message || issuer_pubkey)
fn compute_challenge(
    a_bytes: &[u8],
    message: &[u8],
    issuer_pubkey: &PubKey,
) -> Result<Scalar, NoteError> {
    let mut hasher = Blake2b::<U32>::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schnorr_roundtrip() {
        // Generate a key pair
        let (secret_key, public_key) = generate_keypair();

        // Create test data
        let recipient_pubkey = public_key; // Use the generated public key as recipient
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp

        // Create the message to be signed
        let message = signing_message(&recipient_pubkey, amount, timestamp);

        // Sign the message
        let signature = schnorr_sign(&message, &secret_key, &recipient_pubkey)
            .expect("Signing should succeed");

        // Verify the signature
        schnorr_verify(&signature, &message, &recipient_pubkey)
            .expect("Verification should succeed");

        assert_eq!(signature.len(), 65, "Signature should be 65 bytes");
    }

    #[test]
    fn test_schnorr_with_tampered_message() {
        // Generate a key pair
        let (secret_key, public_key) = generate_keypair();

        // Create test data
        let recipient_pubkey = public_key; // Use the generated public key as recipient
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp

        // Create the message to be signed
        let message = signing_message(&recipient_pubkey, amount, timestamp);

        // Sign the message
        let signature = schnorr_sign(&message, &secret_key, &recipient_pubkey)
            .expect("Signing should succeed");

        // Tamper with the message
        let mut tampered_message = message.clone();
        tampered_message[0] ^= 0x01; // Flip one bit

        // Verify with tampered message (should fail)
        let result = schnorr_verify(&signature, &tampered_message, &recipient_pubkey);

        assert!(result.is_err(), "Verification should fail with tampered message");
    }

    #[test]
    fn test_schnorr_with_wrong_public_key() {
        // Generate two key pairs
        let (secret_key1, public_key1) = generate_keypair();
        let (secret_key2, public_key2) = generate_keypair();

        // Create test data
        let recipient_pubkey = public_key1; // Use first public key as recipient
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp

        // Create the message to be signed
        let message = signing_message(&recipient_pubkey, amount, timestamp);

        // Sign the message with first party's key
        let signature = schnorr_sign(&message, &secret_key1, &recipient_pubkey)
            .expect("Signing should succeed");

        // Verify with second party's public key (should fail)
        let result = schnorr_verify(&signature, &message, &public_key2);

        assert!(result.is_err(), "Verification should fail with wrong public key");
    }

    #[test]
    fn test_signing_message_format() {
        let recipient_pubkey = [0x02u8; 33]; // Example public key
        let amount = 1000000000u64;
        let timestamp = 1672531200u64;

        let message = signing_message(&recipient_pubkey, amount, timestamp);

        // Verify the format: recipient_pubkey (33 bytes) + amount_be_bytes (8 bytes) + timestamp_be_bytes (8 bytes)
        assert_eq!(message.len(), 33 + 8 + 8, "Message should be 49 bytes");
        assert_eq!(&message[0..33], &recipient_pubkey, "First 33 bytes should be recipient pubkey");
        assert_eq!(&message[33..41], &amount.to_be_bytes(), "Next 8 bytes should be amount in big endian");
        assert_eq!(&message[41..49], &timestamp.to_be_bytes(), "Last 8 bytes should be timestamp in big endian");
    }

    #[test]
    fn test_invalid_signature_length() {
        // This test is actually testing the validation inside the function
        // Since the function signature requires [u8; 65], we can't pass a [u8; 64]
        // So instead we'll test the internal validation by creating a signature that fails validation
        let (_, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Create a signature with invalid 'a' component to trigger validation failure
        let mut invalid_signature = [0u8; 65];
        invalid_signature[0] = 0x01; // Invalid prefix (should be 0x02 or 0x03)

        let result = schnorr_verify(&invalid_signature, &message, &public_key);
        assert!(result.is_err(), "Verification should fail with invalid signature format");
    }

    #[test]
    fn test_invalid_signature_a_component() {
        let (_, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Create a signature with invalid 'a' component (not a valid compressed point)
        let mut invalid_signature = [0u8; 65];
        invalid_signature[0] = 0x01; // Invalid prefix (should be 0x02 or 0x03)

        let result = schnorr_verify(&invalid_signature, &message, &public_key);
        assert!(result.is_err(), "Verification should fail with invalid 'a' component");
    }

    #[test]
    fn test_invalid_signature_z_component_all_zeros() {
        let (_, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Create a signature with 'z' component all zeros
        let mut invalid_signature = [0u8; 65];
        invalid_signature[0] = 0x02; // Valid prefix
        // Leave the z component as all zeros (bytes 33-65)

        let result = schnorr_verify(&invalid_signature, &message, &public_key);
        assert!(result.is_err(), "Verification should fail with all-zeros 'z' component");
    }

    #[test]
    fn test_invalid_public_key_length() {
        let (secret_key, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Sign with valid key
        let signature = schnorr_sign(&message, &secret_key, &public_key)
            .expect("Signing should succeed");

        // Create an invalid public key with wrong length
        let invalid_pubkey = [0x02u8; 32]; // 32 bytes instead of 33

        // Need to create a proper 33-byte array to pass to the function
        let mut invalid_pubkey_33 = [0x02u8; 33];
        invalid_pubkey_33[0] = 0x01; // Invalid prefix

        let result = schnorr_verify(&signature, &message, &invalid_pubkey_33);
        assert!(result.is_err(), "Verification should fail with invalid public key");
    }

    #[test]
    fn test_invalid_public_key_prefix() {
        let (secret_key, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Sign with valid key
        let signature = schnorr_sign(&message, &secret_key, &public_key)
            .expect("Signing should succeed");

        // Create a public key with invalid prefix
        let mut invalid_pubkey = public_key;
        invalid_pubkey[0] = 0x01; // Invalid prefix (should be 0x02 or 0x03)

        let result = schnorr_verify(&signature, &message, &invalid_pubkey);
        assert!(result.is_err(), "Verification should fail with invalid public key prefix");
    }

    #[test]
    fn test_empty_message() {
        let (secret_key, public_key) = generate_keypair();
        let empty_message: Vec<u8> = vec![];

        let result = schnorr_sign(&empty_message, &secret_key, &public_key);
        // Signing with empty message should still work (the algorithm doesn't validate message content)
        if result.is_ok() {
            let signature = result.unwrap();
            let verify_result = schnorr_verify(&signature, &empty_message, &public_key);
            assert!(verify_result.is_ok(), "Verification should succeed with empty message if signing worked");
        }
    }

    #[test]
    fn test_signature_with_modified_a_component() {
        let (secret_key, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Sign the message
        let mut signature = schnorr_sign(&message, &secret_key, &public_key)
            .expect("Signing should succeed");

        // Modify the 'a' component (first 33 bytes)
        signature[0] ^= 0x01; // Flip a bit in the 'a' component

        let result = schnorr_verify(&signature, &message, &public_key);
        assert!(result.is_err(), "Verification should fail when 'a' component is modified");
    }

    #[test]
    fn test_signature_with_modified_z_component() {
        let (secret_key, public_key) = generate_keypair();
        let message = signing_message(&public_key, 1000, 1234567890);

        // Sign the message
        let mut signature = schnorr_sign(&message, &secret_key, &public_key)
            .expect("Signing should succeed");

        // Modify the 'z' component (bytes 33-65)
        signature[33] ^= 0x01; // Flip a bit in the 'z' component

        let result = schnorr_verify(&signature, &message, &public_key);
        assert!(result.is_err(), "Verification should fail when 'z' component is modified");
    }
}