//! Schnorr signature implementation using the basis_core crate

use basis_core::impls::SchnorrVerifier;
use basis_core::traits::SignatureVerifier;

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
    basis_core::types::signing_message(recipient_pubkey, amount, timestamp)
}

/// Validate that a public key is a valid compressed secp256k1 point
pub fn validate_public_key(pubkey: &PubKey) -> Result<(), NoteError> {
    match basis_core::impls::validate_public_key(pubkey) {
        Ok(()) => Ok(()),
        Err(basis_core::traits::CryptoError::InvalidPublicKey) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidSignature) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InternalError(_)) => Err(NoteError::InvalidSignature),
    }
}

/// Validate that a signature has the correct format (33-byte a + 32-byte z)
pub fn validate_signature_format(signature: &Signature) -> Result<(), NoteError> {
    match basis_core::impls::validate_signature_format(signature) {
        Ok(()) => Ok(()),
        Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidSignature) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidPublicKey) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InternalError(_)) => Err(NoteError::InvalidSignature),
    }
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

        // Create signature using the core implementation
        let verifier = SchnorrVerifier;
        let signature = verifier.sign_message(&message, &secret_key.secret_bytes(), &issuer_pubkey)
            .expect("Failed to create signature");

        // Verify signature using the core implementation
        verifier.verify_signature(&signature, &message, &issuer_pubkey)
            .expect("Verification should succeed");

        // Test with wrong message
        let wrong_message = signing_message(&recipient_pubkey, amount + 1, timestamp);
        assert!(verifier.verify_signature(&signature, &wrong_message, &issuer_pubkey).is_err());

        // Test with wrong issuer
        let wrong_secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let wrong_public_key = secp256k1::PublicKey::from_secret_key(&secp, &wrong_secret_key);
        let wrong_issuer_pubkey = wrong_public_key.serialize();
        assert!(verifier.verify_signature(&signature, &message, &wrong_issuer_pubkey).is_err());

        // Test with corrupted signature
        let mut corrupted_signature = signature;
        corrupted_signature[50] ^= 0x01; // Flip one bit
        assert!(verifier.verify_signature(&corrupted_signature, &message, &issuer_pubkey).is_err());
    }

    #[test]
    fn test_key_generation() {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let pubkey = public_key.serialize();

        // Validate the generated public key
        assert!(validate_public_key(&pubkey).is_ok());
    }
}

/// Schnorr signature implementation following chaincash-rs approach
pub fn schnorr_sign(
    message: &[u8],
    secret_key: &secp256k1::SecretKey,
    issuer_pubkey: &PubKey,
) -> Result<Signature, NoteError> {
    let verifier = SchnorrVerifier;
    let secret_key_bytes = secret_key.secret_bytes();
    match verifier.sign_message(message, &secret_key_bytes, issuer_pubkey) {
        Ok(signature) => Ok(signature),
        Err(basis_core::traits::CryptoError::InvalidSignature) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidPublicKey) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InternalError(_)) => Err(NoteError::InvalidSignature),
    }
}

/// Schnorr signature verification following chaincash-rs approach
pub fn schnorr_verify(
    signature: &Signature,
    message: &[u8],
    issuer_pubkey: &PubKey,
) -> Result<(), NoteError> {
    let verifier = SchnorrVerifier;
    match verifier.verify_signature(signature, message, issuer_pubkey) {
        Ok(()) => Ok(()),
        Err(basis_core::traits::CryptoError::InvalidSignature) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidPublicKey) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InvalidSignatureFormat) => Err(NoteError::InvalidSignature),
        Err(basis_core::traits::CryptoError::InternalError(_)) => Err(NoteError::InvalidSignature),
    }
}