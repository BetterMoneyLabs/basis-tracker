//! Error types for Basis Trees module

use thiserror::Error;

/// Error types for tree operations
#[derive(Error, Debug)]
pub enum TreeError {
    #[error("Key not found in tree")]
    KeyNotFound,
    
    #[error("Duplicate key found")]
    DuplicateKey,
    
    #[error("Invalid proof format")]
    InvalidProof,
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Tree corruption detected")]
    TreeCorruption,
    
    #[error("Cryptographic error")]
    CryptographicError,
    
    #[error("Invalid tree state")]
    InvalidState,
    
    #[error("Operation not supported")]
    UnsupportedOperation,
}

// Note: ergo_avltree_rust doesn't expose a public Error type
// We'll handle AVL tree errors through string conversions

impl From<std::io::Error> for TreeError {
    fn from(err: std::io::Error) -> Self {
        TreeError::StorageError(format!("IO error: {}", err))
    }
}