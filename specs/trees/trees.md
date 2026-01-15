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

#### Insert Operation (via add_note)
**Purpose**: Add a new IOU note to the tree

**Preconditions**:
- Note signature must be valid
- Timestamp must be increasing (compared to any existing note with same issuer/recipient pair)
- Current time must not be in the future compared to note timestamp

**Process**:
- Create a NoteKey from issuer and recipient public keys (hashed with Blake2b256)
- Convert note data to bytes for AVL tree storage
- Call `BasisAvlTree::update()` to insert the key-value pair into the AVL tree
- Update the tracker state with the new AVL root digest
- Store the note in persistent storage

**Postconditions**:
- Tree contains the new note
- Root digest is updated
- Proof can be generated for the new state
- New root digest is available for tracker box updates

**Error Cases**:
- Invalid signature
- Timestamp violation (future timestamp or not increasing compared to existing note)
- AVL tree storage error
- Persistent storage error

#### Update Operation (via update_note)
**Purpose**: Modify an existing IOU note

**Preconditions**:
- Note signature must be valid
- New timestamp > old timestamp (for same issuer/recipient pair)
- Amount changes must be valid
- Current time must not be in the future compared to note timestamp

**Process**:
- Create a NoteKey from issuer and recipient public keys (hashed with Blake2b256)
- Convert updated note data to bytes for AVL tree storage
- Call `BasisAvlTree::update()` to update the key-value pair in the AVL tree
- Update the tracker state with the new AVL root digest
- Store the updated note in persistent storage

**Postconditions**:
- Tree contains updated note
- Root digest is updated
- Proof can be generated for the new state
- New root digest is available for tracker box updates

**Error Cases**:
- Invalid signature
- Timestamp violation
- Invalid amount change
- AVL tree storage error
- Persistent storage error



### 2. Proof Generation Operations

#### Note Proof Generation (via generate_proof)
**Purpose**: Generate proof that a specific note exists in the current tree state

**Input**: Issuer and recipient public keys
**Output**: NoteProof structure containing the IOU note and AVL tree proof
**Process**:
- Creates a NoteKey from the issuer and recipient public keys
- Calls the underlying AVL tree's generate_proof() method to create the authentication path
- Combines the note data with the AVL proof and operations data
**Complexity**: O(log n)

**Note**: The actual implementation generates proofs using the underlying `BasisAvlTree::generate_proof()` method, but the full verification process is handled by the Ergo blockchain's AVL tree verification mechanisms.

#### Legacy Proof Operations (Defined but Not Used)
The following proof generation operations are defined in the codebase but not currently used in the main tracker flow:

**Membership Proof Generation**:
- Defined in the `proofs.rs` module but not actively called in the current implementation

**Non-Membership Proof Generation**:
- Defined in the `proofs.rs` module but not actively called in the current implementation

**State Proof Generation**:
- Defined in the `proofs.rs` module but not actively called in the current implementation

### 3. State Commitment Operations

#### Root Digest Generation
**Purpose**: Generate cryptographic commitment to tree state

**Input**: Current tree state (after each tree operation)
**Output**: 33-byte root digest
**Process**:
- Called internally after each insert/update operation via `BasisAvlTree::root_digest()`
- Returns the current AVL tree root digest as a 33-byte array
- The digest includes 32 bytes of hash plus 1 byte of height information
**Complexity**: O(1)
**Prerequisite**: Tree operations must be completed successfully to update the root
**Initialization**: AVL tree starts with an empty state that has a default zero digest

#### Periodic Commitment
**Purpose**: Post state digest to blockchain

**Frequency**: Configurable (e.g., every 10 minutes - 600 seconds)
**Trigger**: Time-based (implemented as tracker box updater running every 10 minutes)
**Process**:
- The `TrackerBoxUpdater` retrieves the current AVL root digest from shared state
- Serializes the tracker public key as GroupElement in R4 register
- Serializes the AVL tree root as SAvlTree in R5 register
- Submits a transaction to update the tracker commitment box on-chain
**Verification**: Cross-verify with on-chain data
**Requirements**: R4 register contains tracker public key (GroupElement), R5 register contains AVL tree root (SAvlTree)

#### State Verification
**Purpose**: Verify tree state against commitment

**Input**: Tree state and commitment
**Output**: Boolean (valid/invalid)
**Process**:
- In the actual implementation, verification happens through the underlying AVL tree verification mechanisms
- The Ergo blockchain verifies AVL proofs against the committed root digest
- Verification is performed by the Ergo node when validating redemption transactions
**Complexity**: O(log n) for individual proof verification

### 4. Batch Operations

#### Batch Insert/Update
**Purpose**: Process multiple operations atomically

**Benefits**: Improved performance
**Process**:
- Individual operations are processed one at a time using the `update()` method
- Each operation updates both the AVL tree and persistent storage
- The root digest is updated after each operation
- No true batching is implemented in the current version - operations are sequential
**Complexity**: O(k log n) for k operations
**Postcondition**: Root digest is updated after each operation to reflect the current tree state

#### Batch Proof Generation
**Purpose**: Generate multiple proofs efficiently

**Benefits**: Reduced overhead
**Process**:
- Not currently implemented in the main tracker flow
- Each proof is generated individually when requested
- The underlying AVL tree implementation supports batch operations but they are not utilized in the current tracker
**Complexity**: O(k log n) for k proofs if implemented

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
- **Data Consistency**: Tree state is updated in-memory and then persistent storage is updated (failures in AVL tree prevent storage updates)
- **Sequential Operations**: Tree updates and storage updates are sequential (not atomic in the strictest sense, but AVL tree updates happen first)
- **Recovery**: Tree state is in-memory with operation logging for recovery (not rebuilt from persistent storage)
- **Synchronization**: Tree operations are followed by storage operations in the TrackerStateManager

### With Blockchain
- **State Commitment**: Periodically post tree root digest to blockchain via TrackerBoxUpdater
- **Cross-Verification**: Allow verification of off-chain state against on-chain commitments
- **Redemption Proofs**: Generate AVL tree proofs for redemption operations that can be verified on-chain
- **Audit Trail**: Maintain audit trail of state commitments through tracker commitment boxes on-chain

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

The actual implementation uses a simpler interface that focuses on key-value operations rather than the trait described above:

```rust
pub struct BasisAvlTree {
    prover: BatchAVLProver,
    current_state: TrackerState,
}

impl BasisAvlTree {
    /// Create a new in-memory AVL tree
    pub fn new() -> Result<Self, TreeError>;

    /// Insert a key-value pair into the AVL tree
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), TreeError>;

    /// Update an existing key-value pair (or insert if key doesn't exist)
    pub fn update(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), TreeError>;

    /// Generate a proof for the current tree state
    pub fn generate_proof(&mut self) -> Vec<u8>;

    /// Get the root digest of the AVL tree
    pub fn root_digest(&self) -> [u8; 33];

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState;
}
```

The actual usage in the TrackerStateManager combines this with note storage:

```rust
impl TrackerStateManager {
    /// Add a new note to the tracker state
    pub fn add_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError>;

    /// Update an existing note
    pub fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError>;

    /// Generate proof for a specific note
    pub fn generate_proof(
        &mut self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<NoteProof, NoteError>;

    /// Lookup a note by issuer and recipient
    pub fn lookup_note(
        &self,
        issuer_pubkey: &PubKey,
        recipient_pubkey: &PubKey,
    ) -> Result<IouNote, NoteError>;

    /// Get all notes for a specific issuer
    pub fn get_issuer_notes(&self, issuer_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError>;

    /// Get all notes for a specific recipient
    pub fn get_recipient_notes(&self, recipient_pubkey: &PubKey) -> Result<Vec<IouNote>, NoteError>;

    /// Get all notes in the tracker
    pub fn get_all_notes(&self) -> Result<Vec<IouNote>, NoteError>;

    /// Get all notes in the tracker with issuer information
    pub fn get_all_notes_with_issuer(&self) -> Result<Vec<(PubKey, IouNote)>, NoteError>;

    /// Get the current tracker state
    pub fn get_state(&self) -> &TrackerState;
}
```

### Proof Types

#### Note Proof (Actual Implementation)
The actual implementation uses a NoteProof structure that proves a specific note exists in the current tree state.

**Components**:
- **Note Data**: The complete IOU note being proven
- **AVL Proof**: Authentication path from leaf to root (raw bytes)
- **Operations**: Sequence of operations that led to current state (placeholder in current implementation)

**Structure**:
```rust
pub struct NoteProof {
    /// The IOU note being proven
    pub note: IouNote,
    /// AVL tree proof bytes
    pub avl_proof: Vec<u8>,
    /// Operations performed to generate the proof
    pub operations: Vec<u8>,
}
```

**Generation**:
- Generated by the `TrackerStateManager::generate_proof()` method
- Uses the underlying `BasisAvlTree::generate_proof()` method to create the AVL proof
- Includes the actual note data for verification

**Verification**:
- Verify AVL proof against current tree root
- Verify note signature and validity
- In the actual implementation, verification happens through the underlying AVL tree verification mechanisms

#### Legacy Proof Types (Defined but Not Yet Implemented)
The following proof types are defined in the `basis_trees` crate but not yet fully implemented in the main tracker flow:

**Membership Proof**:
- Defined in `proofs.rs` but not actively used in the current implementation
- Would prove that a specific note exists in the tree

**Non-Membership Proof**:
- Defined in `proofs.rs` but not actively used in the current implementation
- Would prove that a specific key does not exist in the tree

**State Proof**:
- Defined in `proofs.rs` but not actively used in the current implementation
- Would prove the validity of a tree state commitment

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
- `ergo_avltree_rust`: Core AVL+ tree implementation (used for BatchAVLProver and tree operations)
- `fjall`: Persistent storage backend (used in basis_store's persistence module, not directly in trees)
- `blake2`: Cryptographic hashing (used for key generation via blake2b256_hash)
- `secp256k1`: Cryptographic operations (used in basis_store for signature verification)

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
- **Crash Recovery**: Tree state is in-memory with operation logging for recovery (actual recovery implementation uses operation replay)
- **Checkpointing**: Periodic state snapshots (implemented in the recovery system)
- **Backup**: Support for tree state backups (through operation logs)
- **Versioning**: Support for tree state version upgrades
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

1. Align specification with actual implementation ✅
2. Document the actual API for tree operations ✅
3. Specify actual proof formats and verification ✅
4. [Design persistence strategy](./persistence.md) ✅
5. Create integration tests ✅
6. Implement cross-verification with blockchain
7. Define performance benchmarks ✅
8. Create monitoring and logging strategy
9. Design backup and recovery procedures ✅
10. Complete in-memory implementation with operation logging ✅

## Implementation Status

### ✅ Completed
- **Core AVL+ Tree Implementation** with cryptographic commitments
- **In-Memory Storage** with operation logging for recovery
- **Comprehensive Recovery System** with checkpoint management using operation replay
- **Enhanced Test Coverage** (60+ total tests)
- **Performance Benchmarks** for large data and concurrent access
- **Tree Resolver Architecture** with in-memory implementation
- **Test Infrastructure** with dedicated test files for in-memory operations
- **Specification alignment** with actual implementation

### 🔄 In Progress
- Cross-verification with blockchain
- Advanced monitoring and logging

### 📋 Future Enhancements
- **Optimized Operation Recovery**: Improve recovery performance from operation logs
- **Distributed storage support** (if architectural constraints change)
- **Advanced compression algorithms** for operation logs
- **Incremental checkpointing** to reduce operation replay time