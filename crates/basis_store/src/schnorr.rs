//! Schnorr signature implementation following chaincash-rs approach

use crate::{NoteError, PubKey, Signature};
use blake2::{Blake2b512, Digest};
use secp256k1::{self, PublicKey, SecretKey};
use std::convert::TryInto;

/// Generate the signing message in the same format as chaincash-rs
pub fn signing_message(recipient_pubkey: &PubKey, amount: u64, timestamp: u64) -> Vec<u8> {
    let mut message = Vec::new();
    message.extend_from_slice(recipient_pubkey);
    message.extend_from_slice(&amount.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message
}

/// Schnorr signature implementation following chaincash-rs approach
pub fn schnorr_sign(message: &[u8], secret_key: &secp256k1::SecretKey, issuer_pubkey: &PubKey) -> Signature {
    use secp256k1::Secp256k1;
    
    let secp = Secp256k1::new();
    
    // Generate a random nonce for the Schnorr signature
    let nonce_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
    let a_point = secp256k1::PublicKey::from_secret_key(&secp, &nonce_secret);
    let a_bytes = a_point.serialize();
    
    // Compute challenge e = H(a || message || issuer_pubkey)
    let mut hasher = Blake2b512::new();
    hasher.update(a_bytes);
    hasher.update(message);
    hasher.update(issuer_pubkey);
    let e_bytes = hasher.finalize();
    let e_scalar = secp256k1::Scalar::from_be_bytes(e_bytes[..32].try_into().unwrap())
        .expect("Invalid challenge scalar");
    
    // Compute z = k + e * s (mod n) using proper modular arithmetic
    // We need to use the secp256k1 library's methods for scalar operations
    
    // Convert scalars to their big integer representations for modular arithmetic
    let k_big = num_bigint::BigUint::from_bytes_be(&nonce_secret.secret_bytes());
    let s_big = num_bigint::BigUint::from_bytes_be(&secret_key.secret_bytes());
    let e_big = num_bigint::BigUint::from_bytes_be(&e_scalar.to_be_bytes());
    
    // Curve order for secp256k1
    let n = num_bigint::BigUint::from_bytes_be(&[
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,
        0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B,
        0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x41
    ]);
    
    // Compute z = k + e * s (mod n)
    let e_times_s = (&e_big * &s_big) % &n;
    let z_big = (&k_big + &e_times_s) % &n;
    
    // Ensure z is in the valid range [1, n-1]
    if z_big == num_bigint::BigUint::from(0u32) || z_big >= n {
        panic!("Invalid z value");
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
    
    let z_scalar = secp256k1::Scalar::from_be_bytes(z_bytes)
        .expect("Invalid z scalar");
    
    // Convert z to bytes (32 bytes for Schnorr signatures - following chaincash-rs)
    let z_bytes_full = z_scalar.to_be_bytes();
    
    // Create the signature (a || z) - 33 bytes for a, 32 bytes for z
    let mut signature = [0u8; 65];
    signature[0..33].copy_from_slice(&a_bytes);
    signature[33..65].copy_from_slice(&z_bytes_full);
    
    signature
}

/// Schnorr signature verification following chaincash-rs approach
pub fn schnorr_verify(signature: &Signature, message: &[u8], issuer_pubkey: &PubKey) -> Result<(), NoteError> {
    use secp256k1::Secp256k1;
    
    let secp = Secp256k1::new();
    
    // Split signature into components (a, z)
    let a_bytes = &signature[0..33];
    let z_bytes = &signature[33..65];
    
    // Basic validation: check that signature components are non-zero
    if a_bytes.iter().all(|&b| b == 0) || z_bytes.iter().all(|&b| b == 0) {
        return Err(NoteError::InvalidSignature);
    }
    
    // Parse the compressed public key (issuer key)
    let issuer_key = PublicKey::from_slice(issuer_pubkey)
        .map_err(|_| NoteError::InvalidSignature)?;
    
    // Parse random point a from signature (33 bytes compressed point)
    let a_point = PublicKey::from_slice(a_bytes)
        .map_err(|_| NoteError::InvalidSignature)?;
    
    // Compute challenge e = H(a || message || issuer_pubkey)
    let mut hasher = Blake2b512::new();
    hasher.update(a_bytes);
    hasher.update(message);
    hasher.update(issuer_pubkey);
    let e_bytes = hasher.finalize();
    let e_scalar = secp256k1::Scalar::from_be_bytes(e_bytes[..32].try_into().unwrap())
        .map_err(|_| NoteError::InvalidSignature)?;
    
    // Parse z as a scalar (32 bytes)
    let z_scalar = SecretKey::from_slice(z_bytes)
        .map_err(|_| NoteError::InvalidSignature)?;
    
    // Compute g^z (generator point raised to z power)
    let g_z = secp256k1::PublicKey::from_secret_key(&secp, &z_scalar);
    
    // Compute x^e (issuer public key raised to e power)
    let x_e_tweak = issuer_key.mul_tweak(&secp, &e_scalar)
        .map_err(|_| NoteError::InvalidSignature)?;
    
    // Compute a * x^e (point addition)
    let a_x_e = a_point.combine(&x_e_tweak)
        .map_err(|_| NoteError::InvalidSignature)?;

    // Verify g^z == a * x^e
    if g_z != a_x_e {
        return Err(NoteError::InvalidSignature);
    }
    
    Ok(())
}