//! Core implementations for Basis Tracker system

use crate::traits::{SignatureVerifier, CryptoError};
use crate::types::{PubKey, Signature};
use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;
use secp256k1::{self, PublicKey, SecretKey};
use std::convert::TryInto;

/// Canonical Schnorr signature verifier implementation
pub struct SchnorrVerifier;

impl SignatureVerifier for SchnorrVerifier {
    fn verify_signature(&self, signature: &Signature, message: &[u8], public_key: &PubKey) -> Result<(), CryptoError> {
        use secp256k1::Secp256k1;

        let secp = Secp256k1::new();

        // Validate signature format first
        validate_signature_format(signature)?;

        // Validate issuer public key
        validate_public_key(public_key)?;

        // Split signature into components (a, z)
        let a_bytes = &signature[0..33];
        let z_bytes = &signature[33..65];

        // Parse the compressed public key (issuer key)
        let issuer_key =
            PublicKey::from_slice(public_key).map_err(|_| CryptoError::InvalidPublicKey)?;

        // Parse random point a from signature (33 bytes compressed point)
        let a_point = PublicKey::from_slice(a_bytes).map_err(|_| CryptoError::InvalidSignature)?;

        // Compute challenge e = H(a || message || issuer_pubkey)
        let e_scalar = compute_challenge(a_bytes, message, public_key)?;

        // Parse z as a scalar (32 bytes)
        let z_scalar = SecretKey::from_slice(z_bytes).map_err(|_| CryptoError::InvalidSignature)?;

        // Compute g^z (generator point raised to z power)
        let g_z = secp256k1::PublicKey::from_secret_key(&secp, &z_scalar);

        // Compute x^e (issuer public key raised to e power)
        let x_e_tweak = issuer_key
            .mul_tweak(&secp, &e_scalar)
            .map_err(|_| CryptoError::InvalidSignature)?;

        // Compute a * x^e (point addition)
        let a_x_e = a_point
            .combine(&x_e_tweak)
            .map_err(|_| CryptoError::InvalidSignature)?;

        // Verify g^z == a * x^e
        if g_z != a_x_e {
            return Err(CryptoError::InvalidSignature);
        }

        Ok(())
    }

    fn sign_message(&self, message: &[u8], secret_key: &[u8; 32], public_key: &PubKey) -> Result<Signature, CryptoError> {
        use secp256k1::Secp256k1;

        let secp = Secp256k1::new();

        // Parse the secret key
        let secret_key = SecretKey::from_slice(secret_key).map_err(|_| CryptoError::InvalidSignature)?;

        // Generate a random nonce for the Schnorr signature
        let nonce_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let a_point = secp256k1::PublicKey::from_secret_key(&secp, &nonce_secret);
        let a_bytes = a_point.serialize();

        // Compute challenge e = H(a || message || issuer_pubkey)
        let e_scalar = compute_challenge(&a_bytes, message, public_key)?;

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
            return Err(CryptoError::InvalidSignature);
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
            secp256k1::Scalar::from_be_bytes(z_bytes).map_err(|_| CryptoError::InvalidSignature)?;

        // Convert z to bytes (32 bytes for Schnorr signatures - following chaincash-rs)
        let z_bytes_full = z_scalar.to_be_bytes();

        // Create the signature (a || z) - 33 bytes for a, 32 bytes for z
        let mut signature = [0u8; 65];
        signature[0..33].copy_from_slice(&a_bytes);
        signature[33..65].copy_from_slice(&z_bytes_full);

        Ok(signature)
    }
}

/// Validate that a public key is a valid compressed secp256k1 point
pub fn validate_public_key(pubkey: &PubKey) -> Result<(), CryptoError> {
    use secp256k1::PublicKey;

    // Check if the first byte indicates a compressed point (0x02 or 0x03)
    if pubkey[0] != 0x02 && pubkey[0] != 0x03 {
        return Err(CryptoError::InvalidPublicKey);
    }

    // Try to parse the public key to ensure it's valid
    PublicKey::from_slice(pubkey)
        .map(|_| ())
        .map_err(|_| CryptoError::InvalidPublicKey)
}

/// Validate that a signature has the correct format (33-byte a + 32-byte z)
pub fn validate_signature_format(signature: &Signature) -> Result<(), CryptoError> {
    // Check that the signature is exactly 65 bytes
    if signature.len() != 65 {
        return Err(CryptoError::InvalidSignatureFormat);
    }

    // Check that the a component (first 33 bytes) is a valid compressed point
    let a_bytes = &signature[0..33];
    if a_bytes[0] != 0x02 && a_bytes[0] != 0x03 {
        return Err(CryptoError::InvalidSignatureFormat);
    }

    // Check that the z component (last 32 bytes) is not all zeros
    let z_bytes = &signature[33..65];
    if z_bytes.iter().all(|&b| b == 0) {
        return Err(CryptoError::InvalidSignatureFormat);
    }

    Ok(())
}

/// Compute the challenge e = H(a || message || issuer_pubkey)
fn compute_challenge(
    a_bytes: &[u8],
    message: &[u8],
    issuer_pubkey: &PubKey,
) -> Result<secp256k1::Scalar, CryptoError> {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(a_bytes);
    hasher.update(message);
    hasher.update(issuer_pubkey);
    let e_bytes = hasher.finalize();

    // Take first 32 bytes for the scalar
    let e_bytes_32: [u8; 32] = e_bytes[..32]
        .try_into()
        .map_err(|_| CryptoError::InvalidSignature)?;

    secp256k1::Scalar::from_be_bytes(e_bytes_32).map_err(|_| CryptoError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};

    #[test]
    fn test_schnorr_roundtrip() {
        let secp = Secp256k1::new();

        // Generate a key pair
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = public_key.serialize();

        // Create test data
        let recipient_pubkey = issuer_pubkey; // Use the generated public key as recipient
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp

        // Create the message to be signed
        let message = crate::types::signing_message(&recipient_pubkey, amount, timestamp);

        // Sign the message
        let verifier = SchnorrVerifier;
        let signature = verifier.sign_message(&message, &secret_key.secret_bytes(), &recipient_pubkey)
            .expect("Signing should succeed");

        // Verify the signature
        verifier.verify_signature(&signature, &message, &recipient_pubkey)
            .expect("Verification should succeed");

        assert_eq!(signature.len(), 65, "Signature should be 65 bytes");
    }

    #[test]
    fn test_schnorr_with_tampered_message() {
        let secp = Secp256k1::new();

        // Generate a key pair
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let issuer_pubkey = public_key.serialize();

        // Create test data
        let recipient_pubkey = issuer_pubkey; // Use the generated public key as recipient
        let amount = 1000000000u64; // 1 ERG in nanoERG
        let timestamp = 1672531200u64; // Example timestamp

        // Create the message to be signed
        let message = crate::types::signing_message(&recipient_pubkey, amount, timestamp);

        // Sign the message
        let verifier = SchnorrVerifier;
        let signature = verifier.sign_message(&message, &secret_key.secret_bytes(), &recipient_pubkey)
            .expect("Signing should succeed");

        // Tamper with the message
        let mut tampered_message = message.clone();
        tampered_message[0] ^= 0x01; // Flip one bit

        // Verify with tampered message (should fail)
        let result = verifier.verify_signature(&signature, &tampered_message, &recipient_pubkey);

        assert!(result.is_err(), "Verification should fail with tampered message");
    }
}

/// Generate a new keypair for testing and development
pub fn generate_keypair() -> ([u8; 32], PubKey) {
    use secp256k1::{Secp256k1, SecretKey};
    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
    let pubkey_bytes = public_key.serialize();

    (secret_key.secret_bytes(), pubkey_bytes)
}

/// Convert a public key to hexadecimal string
pub fn pubkey_to_hex(pubkey: &PubKey) -> String {
    hex::encode(pubkey)
}

/// Convert a hexadecimal string to a public key
pub fn pubkey_from_hex(hex_str: &str) -> Result<PubKey, CryptoError> {
    let bytes = hex::decode(hex_str).map_err(|_| CryptoError::InternalError("Hex decode failed".to_string()))?;

    if bytes.len() != 33 {
        return Err(CryptoError::InvalidPublicKey);
    }

    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&bytes);
    validate_public_key(&pubkey)?;
    Ok(pubkey)
}

/// Convert a signature to hexadecimal string
pub fn signature_to_hex(signature: &Signature) -> String {
    hex::encode(signature)
}

/// Convert a hexadecimal string to a signature
pub fn signature_from_hex(hex_str: &str) -> Result<Signature, CryptoError> {
    let bytes = hex::decode(hex_str).map_err(|_| CryptoError::InternalError("Hex decode failed".to_string()))?;

    if bytes.len() != 65 {
        return Err(CryptoError::InvalidSignatureFormat);
    }

    let mut signature = [0u8; 65];
    signature.copy_from_slice(&bytes);
    validate_signature_format(&signature)?;
    Ok(signature)
}

/// Schnorr signature implementation following chaincash-rs approach
pub fn schnorr_sign(
    message: &[u8],
    secret_key_bytes: &[u8; 32],
    issuer_pubkey: &PubKey,
) -> Result<Signature, CryptoError> {
    use secp256k1::Secp256k1;

    let secp = Secp256k1::new();

    // Parse the secret key
    let secret_key = SecretKey::from_slice(secret_key_bytes)
        .map_err(|_| CryptoError::InvalidSignature)?;

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
        return Err(CryptoError::InvalidSignature);
    }

    // Convert back to bytes
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

    // Create the signature (a || z) - 33 bytes for a, 32 bytes for z
    let mut signature = [0u8; 65];
    signature[0..33].copy_from_slice(&a_bytes);
    signature[33..65].copy_from_slice(&z_bytes);

    Ok(signature)
}

/// Schnorr signature verification following chaincash-rs approach
pub fn schnorr_verify(
    signature: &Signature,
    message: &[u8],
    issuer_pubkey: &PubKey,
) -> Result<(), CryptoError> {
    let verifier = SchnorrVerifier;
    match verifier.verify_signature(signature, message, issuer_pubkey) {
        Ok(()) => Ok(()),
        Err(CryptoError::InvalidSignature) => Err(CryptoError::InvalidSignature),
        Err(CryptoError::InvalidPublicKey) => Err(CryptoError::InvalidPublicKey),
        Err(CryptoError::InvalidSignatureFormat) => Err(CryptoError::InvalidSignatureFormat),
        Err(CryptoError::InternalError(_)) => Err(CryptoError::InternalError("Verification failed".to_string())),
    }
}