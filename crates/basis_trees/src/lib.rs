//! Basis Trees Module
//! 
//! Cryptographic commitment structures for Basis tracker system.
//! Implements authenticated data structures for state commitments
//! and verifiable proofs.

pub mod avl_tree;
pub mod proofs;
pub mod state;
pub mod errors;
pub mod storage;
pub mod fjall_storage;

#[cfg(test)]
pub mod in_memory_recovery;

#[cfg(test)]
pub mod test_helpers;

#[cfg(test)]
pub mod recovery_tests;

#[cfg(test)]
pub mod fjall_storage_tests;

#[cfg(test)]
pub mod fjall_storage_edge_case_tests;

// Re-export main types for easy access
pub use avl_tree::BasisAvlTree;
pub use proofs::{MembershipProof, NonMembershipProof, StateProof};
pub use state::TrackerState;
pub use errors::TreeError;
pub use storage::{TreeStorage, TreeNode, TreeOperation, TreeCheckpoint, NodeType, OperationType};

// Re-export dependencies for external use
pub use ergo_avltree_rust;
pub use fjall;

/// Main tree interface for Basis tracker
pub trait BasisTree {
    /// Insert a new note into the tree
    fn insert_note(&mut self, issuer_pubkey: &[u8; 33], note_data: &[u8]) -> Result<(), TreeError>;
    
    /// Update an existing note
    fn update_note(&mut self, issuer_pubkey: &[u8; 33], note_data: &[u8]) -> Result<(), TreeError>;
    
    /// Generate membership proof for a note
    fn generate_membership_proof(
        &self,
        issuer_pubkey: &[u8; 33],
        recipient_pubkey: &[u8; 33],
    ) -> Result<MembershipProof, TreeError>;
    
    /// Generate non-membership proof
    fn generate_non_membership_proof(
        &self,
        issuer_pubkey: &[u8; 33],
        recipient_pubkey: &[u8; 33],
    ) -> Result<NonMembershipProof, TreeError>;
    
    /// Get current state commitment
    fn get_state_commitment(&self) -> TrackerState;
    
    /// Verify a proof against current state
    fn verify_proof(&self, proof: &dyn Proof) -> Result<bool, TreeError>;
}

/// Common proof trait
pub trait Proof {
    /// Verify this proof against a state commitment
    fn verify(&self, state: &TrackerState) -> Result<bool, TreeError>;
    
    /// Serialize proof to bytes
    fn to_bytes(&self) -> Vec<u8>;
    
    /// Deserialize proof from bytes
    fn from_bytes(data: &[u8]) -> Result<Self, TreeError> where Self: Sized;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_structure() {
        // Basic test to verify module compiles correctly
        assert!(true, "Module structure should be valid");
    }
}