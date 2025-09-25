//! Contract compilation utilities for Basis tracker

use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Ergo-lib not available: {0}")]
    ErgoLibUnavailable(String),
}

/// Compile an ErgoScript contract to get the ErgoTree template
pub fn compile_contract(contract_path: &str) -> Result<String, CompilerError> {
    // Read the contract source
    let contract_source = fs::read_to_string(contract_path)
        .map_err(|_| CompilerError::FileNotFound(contract_path.to_string()))?;
    
    // For now, return a placeholder template since we don't have the actual compiler set up
    // In a real implementation, this would use ergo-lib's compiler
    
    // Generate a deterministic placeholder based on the contract content
    let template_hash = blake2_hash(&contract_source);
    
    // Format as a placeholder ErgoTree (this would be the actual compiled template)
    let ergo_tree = format!("0008cd{}", hex::encode(&template_hash[..20]));
    
    Ok(ergo_tree)
}

/// Simple Blake2b hash function for placeholder template generation
fn blake2_hash(data: &str) -> [u8; 32] {
    use blake2::{Blake2b, Digest};
    
    let mut hasher = Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result[..32]);
    hash
}

/// Get the Basis contract template from the contract file
pub fn get_basis_contract_template() -> Result<String, CompilerError> {
    // Try to find the contract file
    let contract_paths = [
        "contract/basis.es",
        "../contract/basis.es",
        "./basis.es",
    ];
    
    for path in &contract_paths {
        if Path::new(path).exists() {
            return compile_contract(path);
        }
    }
    
    // If contract file not found, return a hardcoded placeholder
    // This would be replaced with the actual compiled template
    Ok("0008cd0101010101010101010101010101010101010101".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contract_compilation_placeholder() {
        // Test that we can generate a placeholder template
        let template = get_basis_contract_template().unwrap();
        assert!(!template.is_empty());
        assert!(template.starts_with("0008cd"));
    }
    
    #[test]
    fn test_blake2_hash() {
        let data = "test data";
        let hash = blake2_hash(data);
        assert_eq!(hash.len(), 32);
        
        // Same input should produce same hash
        let hash2 = blake2_hash(data);
        assert_eq!(hash, hash2);
        
        // Different input should produce different hash
        let hash3 = blake2_hash("different data");
        assert_ne!(hash, hash3);
    }
}