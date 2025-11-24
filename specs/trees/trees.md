# Basis Trees Module Specification

## Overview

The `basis_trees` module provides cryptographic commitment structures for the Basis tracker system. It implements 
authenticated data structure that enable efficient state commitments and verifiable proofs for debt tracking.

## Core Components

### 1. AVL+ Tree for State Commitment

**Purpose**: Store and commit to the complete state of IOU notes in the tracker system.

**Key Properties**:
- **Authenticated**: Every operation generates cryptographic proofs
- **Persistent**: Tree state can be persisted and restored
- **Verifiable**: Third parties can verify state without full data

**Tree Structure**:
- **Keys**: `Hash(issuer_pubkey || recipient_pubkey)` (64 bytes)
- **Values**: Serialized IOU note data
- **Root Digest**: 33-byte commitment (32-byte hash + 1-byte height)

### 2. Note Key Format

```rust
struct NoteKey {
    issuer_hash: [u8; 32],   // Blake2b256(issuer_pubkey)
    recipient_hash: [u8; 32], // Blake2b256(recipient_pubkey)
}
```

### 3. State Commitment

```rust
struct TrackerState {
    avl_root_digest: [u8; 33],    // AVL tree root + height
    last_commit_height: u64,      // Block height of last on-chain commitment
    last_update_timestamp: u64,   // Timestamp of last state update
}
```

## Operations

### 1. Note Management Operations

#### Insert Operation
**Purpose**: Add a new IOU note to the tree

**Preconditions**:
- Note key must not exist in tree
- Note signature must be valid
- Timestamp must be increasing

**Postconditions**:
- Tree contains the new note
- Root digest is updated

**Error Cases**:
- Duplicate key
- Invalid signature
- Timestamp violation

#### Update Operation
**Purpose**: Modify an existing IOU note

**Preconditions**:
- Note key must exist in tree
- New timestamp > old timestamp
- Amount changes must be valid

**Postconditions**:
- Tree contains updated note
- Root digest is updated

**Error Cases**:
- Key not found
- Timestamp violation
- Invalid amount change



### 2. Proof Generation Operations

#### Membership Proof Generation
**Purpose**: Generate proof that a note exists

**Input**: Issuer and recipient public keys
**Output**: Membership proof structure
**Complexity**: O(log n)

#### Non-Membership Proof Generation
**Purpose**: Generate proof that a note doesn't exist

**Input**: Issuer and recipient public keys
**Output**: Non-membership proof structure
**Complexity**: O(log n)

#### State Proof Generation
**Purpose**: Generate proof for current tree state

**Input**: None
**Output**: State proof structure
**Complexity**: O(1)

### 3. State Commitment Operations

#### Root Digest Generation
**Purpose**: Generate cryptographic commitment to tree state

**Input**: Current tree state
**Output**: 33-byte root digest
**Complexity**: O(1)

#### Periodic Commitment
**Purpose**: Post state digest to blockchain

**Frequency**: Configurable (e.g., every 100 blocks)
**Trigger**: Block height or time-based
**Verification**: Cross-verify with on-chain data

#### State Verification
**Purpose**: Verify tree state against commitment

**Input**: Tree state and commitment
**Output**: Boolean (valid/invalid)
**Complexity**: O(1)

### 4. Batch Operations

#### Batch Insert/Update
**Purpose**: Process multiple operations atomically

**Benefits**: Improved performance
**Atomicity**: All operations succeed or fail together
**Complexity**: O(k log n) for k operations

#### Batch Proof Generation
**Purpose**: Generate multiple proofs efficiently

**Benefits**: Reduced overhead
**Optimization**: Shared tree traversal
**Complexity**: O(k log n) for k proofs

## Tree Invariants

### Structural Invariants
- **Balance Property**: Height difference between left and right subtrees ≤ 1
- **Ordering Property**: Left subtree keys < node key < right subtree keys
- **Height Property**: Node height = 1 + max(left.height, right.height)
- **Leaf Property**: All leaf nodes are at the same level

### Data Invariants
- **Key Uniqueness**: No duplicate keys in the tree
- **Value Consistency**: Tree values match persistent storage
- **Root Consistency**: Root digest reflects complete tree state
- **Proof Consistency**: All generated proofs are verifiable

### Validation Rules
- **Pre-insertion**: Verify key doesn't exist (unless update)
- **Post-operation**: Verify tree remains balanced
- **Proof Generation**: Verify proof corresponds to current state
- **State Commitment**: Verify commitment matches tree root

## Integration Patterns

### With Basis Store
- **Data Consistency**: Tree state must match persistent note storage
- **Atomic Operations**: Tree updates and storage updates must be atomic
- **Recovery**: Tree can be rebuilt from persistent storage if needed
- **Synchronization**: Tree operations synchronized with storage operations

### With Blockchain
- **State Commitment**: Periodically post tree root digest to blockchain
- **Cross-Verification**: Allow verification of off-chain state against on-chain commitments
- **Redemption Proofs**: Generate proofs for redemption operations
- **Audit Trail**: Maintain audit trail of state commitments

### Migration Strategy
- **Versioning**: Support for tree state version upgrades
- **Backward Compatibility**: Old proofs should remain verifiable
- **Data Migration**: Tools for migrating tree state between versions
- **Rollback Support**: Ability to rollback to previous tree states

## Deployment Considerations

### Development Environment
- **In-Memory Trees**: For testing and development
- **Mock Storage**: For unit testing without persistence
- **Test Data**: Pre-populated trees for integration testing

### Production Environment
- **Persistent Storage**: Reliable disk-based storage
- **Backup Strategy**: Regular tree state backups
- **Monitoring**: Tree health and performance monitoring
- **Scaling**: Support for large numbers of notes

## API Specification

### Core Tree Interface

```rust
pub trait BasisTree {
    /// Insert a new note into the tree
    fn insert_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), TreeError>;
    
    /// Update an existing note
    fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), TreeError>;
    

    
    /// Generate membership proof for a note
    fn generate_membership_proof(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<MembershipProof, TreeError>;
    
    /// Generate non-membership proof
    fn generate_non_membership_proof(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<NonMembershipProof, TreeError>;
    
    /// Get current state commitment
    fn get_state_commitment(&self) -> TrackerState;
    
    /// Verify a proof against current state
    fn verify_proof(&self, proof: &Proof) -> Result<bool, TreeError>;
}
```

### Proof Types

#### Membership Proof
Proves that a specific note exists in the current tree state.

**Components**:
- **Note Data**: The complete IOU note being proven
- **AVL Proof**: Authentication path from leaf to root
- **Operations**: Sequence of operations that led to current state
- **Root Digest**: Current tree root for verification

**Verification**:
- Verify AVL proof against claimed root
- Verify note signature and validity
- Verify operations sequence consistency

#### Non-Membership Proof
Proves that a specific key does not exist in the current tree state.

**Components**:
- **Key**: The note key being proven non-existent
- **AVL Proof**: Authentication path showing key absence
- **Neighbors**: Closest existing keys (predecessor/successor)
- **Root Digest**: Current tree root for verification

**Verification**:
- Verify AVL proof shows key absence
- Verify neighbor keys are valid
- Verify proof against current root

#### State Proof
Proves that a specific root digest represents a valid tree state.

**Components**:
- **Root Digest**: The claimed tree root
- **Proof Data**: Cryptographic proof of root validity
- **Height**: Tree height at time of commitment
- **Timestamp**: When the state was committed

**Verification**:
- Verify proof data cryptographically
- Verify height and timestamp consistency
- Cross-verify with on-chain commitments

### Cryptographic Properties

#### Hash Function
- **Algorithm**: Blake2b-256
- **Properties**: Collision resistance, preimage resistance
- **Usage**: Key hashing, tree node hashing

#### Proof Security
- **Soundness**: False proofs cannot be generated
- **Completeness**: Valid proofs always verify correctly
- **Zero-Knowledge**: Proofs reveal only minimal information

#### Forward Security
- **Property**: Old proofs remain valid even after tree updates
- **Mechanism**: Tree structure preserves authentication paths
- **Benefit**: Historical state verification remains possible

### Error Types

```rust
enum TreeError {
    KeyNotFound,
    DuplicateKey,
    InvalidProof,
    StorageError(String),
    TreeCorruption,
    CryptographicError,
}
```

## Implementation Details

### Dependencies
- `ergo_avltree_rust`: Core AVL+ tree implementation
- `fjall`: Persistent storage backend
- `blake2`: Cryptographic hashing

### Error Handling
- Tree operation failures
- Storage errors
- Cryptographic verification failures

### Testing Strategy
- Unit tests for tree operations
- Integration tests with storage
- Property-based testing for invariants

## Operational Requirements

### Performance Requirements
- **Insert/Update**: O(log n) time complexity
- **Proof Generation**: O(log n) time complexity
- **Proof Verification**: O(log n) time complexity
- **State Commitment**: O(1) time complexity
- **Memory Usage**: O(n) space complexity
- **Batch Operations**: Support for efficient batch updates

### Security Requirements
- **Cryptographic Security**: All proofs are cryptographically sound
- **Non-repudiation**: State commitments cannot be forged
- **Consistency**: Tree invariants maintained across operations
- **Integrity**: Tamper-evident tree structure
- **Forward Security**: Old proofs remain valid even after tree updates

### Persistence Requirements
- **Crash Recovery**: Tree state can be recovered after crashes
- **Checkpointing**: Periodic state snapshots
- **Backup**: Support for tree state backups
- **Versioning**: Support for multiple tree versions
- **Migration**: Tools for state migration between versions

### Reliability Requirements
- **Atomic Operations**: All tree operations are atomic
- **Consistent State**: Tree state always reflects committed operations
- **Error Recovery**: Graceful handling of storage failures
- **Data Integrity**: Automatic detection of tree corruption

## Tree Lifecycle

### Initialization
- **Empty Tree**: Start with empty AVL tree
- **Initial Root**: Zero digest for empty state
- **Configuration**: Set tree parameters (key size, value size)

### Normal Operation
- **Note Operations**: Insert, update, remove notes
- **Proof Generation**: Generate proofs as needed
- **State Updates**: Update tree state after operations
- **Periodic Commitment**: Post state to blockchain

### Recovery Scenarios
- **Crash Recovery**: Rebuild tree from persistent storage
- **Corruption Recovery**: Detect and repair tree corruption
- **Version Recovery**: Rollback to previous tree version

### Shutdown
- **State Persistence**: Save final tree state
- **Cleanup**: Release resources
- **Backup**: Create final backup if needed

## State Transitions

### Tree State Transitions
```
Empty → Active (first insertion)
Active → Updated (subsequent operations)
Updated → Committed (periodic commitment)
Committed → Active (new operations)
```

### Proof State Transitions
```
Generated → Verified (proof validation)
Verified → Expired (tree updated)
Expired → Regenerated (new proof needed)
```

### Commitment State Transitions
```
Pending → Posted (on-chain commitment)
Posted → Verified (cross-verification)
Verified → Superseded (new commitment)
```

## In-Memory Tree Implementation (Current Architecture)

### Current Architecture

#### In-Memory Tree with Operation Logging
```rust
// In-memory resolver that should not be called (panics if called)
// All tree operations are managed in-memory with operation logging for recovery
fn tree_resolver(_digest: &[u8; 32]) -> ergo_avltree_rust::batch_node::Node {
    // This resolver should never be called with in-memory trees
    // Operations are logged separately for recovery purposes
    panic!("Tree resolver called - this should not happen with in-memory trees");
}
```

#### Storage Integration Points
- **Operation Logging**: All tree operations (insert/update) are logged to in-memory storage
- **Checkpoint Management**: Tree state snapshots stored in in-memory storage for recovery
- **Node Storage**: Tree nodes kept in-memory (no persistent storage due to resolver limitations)

### Implementation Status

#### ✅ Completed
- **In-Memory Tree Architecture**: Proper in-memory tree implementation with operation logging
- **Storage Interface**: Integration with TreeStorage for operation logging
- **Test Infrastructure**: Memory-based resolver for testing scenarios
- **Operation Persistence**: All tree operations logged in-memory for recovery

#### 🔄 Implementation Challenges (Resolved by Design Choice)
- **Static Resolver Constraint**: ergo_avltree_rust requires static resolver function
- **Node Extraction**: Tree nodes stored in-memory rather than extracted for persistent storage
- **Persistence Limitation**: Switched to operation-logging approach due to architectural mismatch

## Next Steps

1. Define detailed API for tree operations ✅
2. Specify proof formats and verification ✅
3. [Design persistence strategy](./persistence.md) ✅
4. Create integration tests ✅
5. Implement cross-verification with blockchain
6. Define performance benchmarks ✅
7. Create monitoring and logging strategy
8. Design backup and recovery procedures ✅
9. Complete in-memory implementation with operation logging

## Implementation Status

### ✅ Completed
- **Core AVL+ Tree Implementation** with cryptographic commitments
- **In-Memory Storage** with operation logging for recovery
- **Comprehensive Recovery System** with checkpoint management using operation replay
- **Enhanced Test Coverage** (60+ total tests)
- **Performance Benchmarks** for large data and concurrent access
- **Tree Resolver Architecture** with in-memory implementation
- **Test Infrastructure** with dedicated test files for in-memory operations

### 🔄 In Progress
- Cross-verification with blockchain
- Advanced monitoring and logging

### 📋 Future Enhancements
- **Optimized Operation Recovery**: Improve recovery performance from operation logs
- **Distributed storage support** (if architectural constraints change)
- **Advanced compression algorithms** for operation logs
- **Incremental checkpointing** to reduce operation replay time