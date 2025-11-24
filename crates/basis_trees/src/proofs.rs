//! Proof structures for Basis tree verification

use crate::state::TrackerState;
use crate::errors::TreeError;

/// Membership proof for a specific note
#[derive(Debug, Clone)]
pub struct MembershipProof {
    /// The note being proven
    pub note_data: Vec<u8>,
    /// AVL tree proof bytes
    pub avl_proof: Vec<u8>,
    /// Operations performed to generate the proof
    pub operations: Vec<u8>,
    /// Current tree root for verification
    pub root_digest: Vec<u8>,
}

/// Non-membership proof
#[derive(Debug, Clone)]
pub struct NonMembershipProof {
    /// The key being proven non-existent
    pub key: Vec<u8>,
    /// AVL tree proof bytes
    pub avl_proof: Vec<u8>,
    /// Closest existing keys (predecessor/successor)
    pub neighbors: Vec<Vec<u8>>,
    /// Current tree root for verification
    pub root_digest: Vec<u8>,
}

/// State commitment proof
#[derive(Debug, Clone)]
pub struct StateProof {
    /// The claimed tree root
    pub root_digest: Vec<u8>,
    /// Cryptographic proof of root validity
    pub proof_data: Vec<u8>,
    /// Tree height at time of commitment
    pub height: u8,
    /// When the state was committed
    pub timestamp: u64,
}

impl MembershipProof {
    /// Create a new membership proof
    pub fn new(note_data: Vec<u8>, avl_proof: Vec<u8>, operations: Vec<u8>, root_digest: Vec<u8>) -> Self {
        Self {
            note_data,
            avl_proof,
            operations,
            root_digest: root_digest.to_vec(),
        }
    }

    /// Verify this proof against a state commitment
    pub fn verify(&self, state: &TrackerState) -> Result<bool, TreeError> {
        // Verify root matches
        if self.root_digest != state.avl_root_digest {
            return Ok(false);
        }

        // In real implementation, this would:
        // 1. Verify AVL proof cryptographically
        // 2. Verify note signature and validity
        // 3. Verify operations sequence consistency
        
        // Placeholder implementation
        Ok(!self.avl_proof.is_empty())
    }

    /// Serialize proof to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Note data length + data
        bytes.extend_from_slice(&(self.note_data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.note_data);
        
        // AVL proof length + data
        bytes.extend_from_slice(&(self.avl_proof.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.avl_proof);
        
        // Operations length + data
        bytes.extend_from_slice(&(self.operations.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.operations);
        
        // Root digest
        bytes.extend_from_slice(&self.root_digest);
        
        bytes
    }

    /// Deserialize proof from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, TreeError> {
        let mut offset = 0;
        
        // Read note data
        if data.len() < offset + 4 {
            return Err(TreeError::InvalidProof);
        }
        let note_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        if data.len() < offset + note_len {
            return Err(TreeError::InvalidProof);
        }
        let note_data = data[offset..offset + note_len].to_vec();
        offset += note_len;
        
        // Read AVL proof
        if data.len() < offset + 4 {
            return Err(TreeError::InvalidProof);
        }
        let avl_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        if data.len() < offset + avl_len {
            return Err(TreeError::InvalidProof);
        }
        let avl_proof = data[offset..offset + avl_len].to_vec();
        offset += avl_len;
        
        // Read operations
        if data.len() < offset + 4 {
            return Err(TreeError::InvalidProof);
        }
        let ops_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        if data.len() < offset + ops_len {
            return Err(TreeError::InvalidProof);
        }
        let operations = data[offset..offset + ops_len].to_vec();
        offset += ops_len;
        
        // Read root digest
        if data.len() < offset + 33 {
            return Err(TreeError::InvalidProof);
        }
        let root_digest = data[offset..offset + 33].to_vec();
        
        Ok(Self {
            note_data,
            avl_proof,
            operations,
            root_digest,
        })
    }
}

impl NonMembershipProof {
    /// Create a new non-membership proof
    pub fn new(key: Vec<u8>, avl_proof: Vec<u8>, neighbors: Vec<Vec<u8>>, root_digest: Vec<u8>) -> Self {
        Self {
            key,
            avl_proof,
            neighbors,
            root_digest: root_digest.to_vec(),
        }
    }

    /// Verify this proof against a state commitment
    pub fn verify(&self, state: &TrackerState) -> Result<bool, TreeError> {
        // Verify root matches
        if self.root_digest != state.avl_root_digest {
            return Ok(false);
        }

        // In real implementation, this would:
        // 1. Verify AVL proof shows key absence
        // 2. Verify neighbor keys are valid
        // 3. Verify proof against current root
        
        // Placeholder implementation
        Ok(!self.avl_proof.is_empty())
    }

    /// Serialize proof to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Key length + data
        bytes.extend_from_slice(&(self.key.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.key);
        
        // AVL proof length + data
        bytes.extend_from_slice(&(self.avl_proof.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.avl_proof);
        
        // Number of neighbors
        bytes.extend_from_slice(&(self.neighbors.len() as u32).to_be_bytes());
        
        // Each neighbor
        for neighbor in &self.neighbors {
            bytes.extend_from_slice(&(neighbor.len() as u32).to_be_bytes());
            bytes.extend_from_slice(neighbor);
        }
        
        // Root digest
        bytes.extend_from_slice(&self.root_digest);
        
        bytes
    }

    /// Deserialize proof from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, TreeError> {
        let mut offset = 0;
        
        // Read key
        if data.len() < offset + 4 {
            return Err(TreeError::InvalidProof);
        }
        let key_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        if data.len() < offset + key_len {
            return Err(TreeError::InvalidProof);
        }
        let key = data[offset..offset + key_len].to_vec();
        offset += key_len;
        
        // Read AVL proof
        if data.len() < offset + 4 {
            return Err(TreeError::InvalidProof);
        }
        let avl_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        if data.len() < offset + avl_len {
            return Err(TreeError::InvalidProof);
        }
        let avl_proof = data[offset..offset + avl_len].to_vec();
        offset += avl_len;
        
        // Read neighbors
        if data.len() < offset + 4 {
            return Err(TreeError::InvalidProof);
        }
        let neighbors_count = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        let mut neighbors = Vec::new();
        for _ in 0..neighbors_count {
            if data.len() < offset + 4 {
                return Err(TreeError::InvalidProof);
            }
            let neighbor_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            
            if data.len() < offset + neighbor_len {
                return Err(TreeError::InvalidProof);
            }
            let neighbor = data[offset..offset + neighbor_len].to_vec();
            offset += neighbor_len;
            neighbors.push(neighbor);
        }
        
        // Read root digest
        if data.len() < offset + 33 {
            return Err(TreeError::InvalidProof);
        }
        let root_digest = data[offset..offset + 33].to_vec();
        
        Ok(Self {
            key,
            avl_proof,
            neighbors,
            root_digest,
        })
    }
}

impl StateProof {
    /// Create a new state proof
    pub fn new(root_digest: Vec<u8>, proof_data: Vec<u8>, height: u8, timestamp: u64) -> Self {
        Self {
            root_digest: root_digest.to_vec(),
            proof_data,
            height,
            timestamp,
        }
    }

    /// Verify this proof against a state commitment
    pub fn verify(&self, _state: &TrackerState) -> Result<bool, TreeError> {
        // In real implementation, this would:
        // 1. Verify proof data cryptographically
        // 2. Verify height and timestamp consistency
        // 3. Cross-verify with on-chain commitments
        
        // Placeholder implementation
        Ok(!self.proof_data.is_empty())
    }

    /// Serialize proof to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Root digest
        bytes.extend_from_slice(&self.root_digest);
        
        // Proof data length + data
        bytes.extend_from_slice(&(self.proof_data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.proof_data);
        
        // Height and timestamp
        bytes.push(self.height);
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        
        bytes
    }

    /// Deserialize proof from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, TreeError> {
        if data.len() < 33 + 4 + 1 + 8 {
            return Err(TreeError::InvalidProof);
        }
        
        let mut offset = 0;
        
        // Read root digest
        let root_digest = data[offset..offset + 33].to_vec();
        offset += 33;
        
        // Read proof data
        let proof_len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        
        if data.len() < offset + proof_len + 1 + 8 {
            return Err(TreeError::InvalidProof);
        }
        let proof_data = data[offset..offset + proof_len].to_vec();
        offset += proof_len;
        
        // Read height and timestamp
        let height = data[offset];
        offset += 1;
        
        let timestamp = u64::from_be_bytes(data[offset..offset + 8].try_into().unwrap());
        
        Ok(Self {
            root_digest,
            proof_data,
            height,
            timestamp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_proof_serialization() {
        let proof = MembershipProof::new(
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
            vec![10u8; 33],
        );

        let bytes = proof.to_bytes();
        let restored = MembershipProof::from_bytes(&bytes).unwrap();

        assert_eq!(proof.note_data, restored.note_data);
        assert_eq!(proof.avl_proof, restored.avl_proof);
        assert_eq!(proof.operations, restored.operations);
        assert_eq!(proof.root_digest, restored.root_digest);
    }

    #[test]
    fn test_non_membership_proof_serialization() {
        let proof = NonMembershipProof::new(
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![vec![7, 8], vec![9, 10]],
            vec![11u8; 33],
        );

        let bytes = proof.to_bytes();
        let restored = NonMembershipProof::from_bytes(&bytes).unwrap();

        assert_eq!(proof.key, restored.key);
        assert_eq!(proof.avl_proof, restored.avl_proof);
        assert_eq!(proof.neighbors, restored.neighbors);
        assert_eq!(proof.root_digest, restored.root_digest);
    }

    #[test]
    fn test_state_proof_serialization() {
        let proof = StateProof::new(
            vec![12u8; 33],
            vec![13, 14, 15],
            5,
            1234567890,
        );

        let bytes = proof.to_bytes();
        let restored = StateProof::from_bytes(&bytes).unwrap();

        assert_eq!(proof.root_digest, restored.root_digest);
        assert_eq!(proof.proof_data, restored.proof_data);
        assert_eq!(proof.height, restored.height);
        assert_eq!(proof.timestamp, restored.timestamp);
    }

    #[test]
    fn test_invalid_proof_deserialization() {
        let short_data = vec![1u8; 10];
        assert!(MembershipProof::from_bytes(&short_data).is_err());
        assert!(NonMembershipProof::from_bytes(&short_data).is_err());
        assert!(StateProof::from_bytes(&short_data).is_err());
    }
}