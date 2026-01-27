//! Core traits for Basis Tracker system

use crate::types::{PubKey, Signature};
use thiserror::Error;

/// Error types for cryptographic operations
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Invalid signature format")]
    InvalidSignatureFormat,
    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Trait for signature verification
pub trait SignatureVerifier {
    /// Verify a Schnorr signature
    fn verify_signature(&self, signature: &Signature, message: &[u8], public_key: &PubKey) -> Result<(), CryptoError>;
    
    /// Sign a message with a secret key
    fn sign_message(&self, message: &[u8], secret_key: &[u8; 32], public_key: &PubKey) -> Result<Signature, CryptoError>;
}

/// Trait for AVL tree operations
pub trait AvlTree {
    type Error;
    
    /// Insert a key-value pair into the AVL tree
    fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    
    /// Update an existing key-value pair
    fn update(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    
    /// Generate a proof for a specific key
    fn generate_proof(&mut self, key: &[u8]) -> Result<Vec<u8>, Self::Error>;
    
    /// Get the root digest of the AVL tree
    fn root_digest(&self) -> [u8; 33];
}