# Basis Trees Persistence Strategy

## Overview

This document specifies the persistence strategy for the Basis trees module, focusing on how the AVL+ tree state is stored, recovered, and maintained across application restarts.

## Persistence Requirements

### Functional Requirements
- **Crash Recovery**: Tree state must survive application crashes
- **Consistency**: Tree state must remain consistent with note storage
- **Performance**: Persistence operations should not significantly impact tree operations
- **Scalability**: Support for large numbers of notes (thousands to millions)
- **Auditability**: Maintain complete operation history

### Non-Functional Requirements
- **Reliability**: 99.9% data integrity guarantee
- **Performance**: Sub-second recovery for typical workloads
- **Storage Efficiency**: Minimize storage overhead
- **Backup Support**: Enable easy backup and restore operations

## Architecture

### Multi-Layer Persistence Strategy

```
┌─────────────────┐
│   Tree State    │ ← Primary: Node-level storage
│   (In-Memory)   │
└─────────────────┘
        │
        ▼
┌─────────────────┐
│  Node Storage   │ ← Persistent node database
│   (Fjall)       │
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ Operation Log   │ ← Recovery: Operation sequence
│   (Fjall)       │
└─────────────────┘
        │
        ▼
┌─────────────────┐
│  Checkpoints    │ ← Backup: Periodic snapshots
│   (Fjall)       │
└─────────────────┘
```

## Implementation Details

### 1. Node-Level Persistence

#### Storage Schema
```rust
// Tree nodes stored by their digest
struct TreeNode {
    digest: [u8; 32],      // Node identifier
    node_type: NodeType,   // Leaf, Branch, etc.
    key: Vec<u8>,          // For leaf nodes
    value: Vec<u8>,        // For leaf nodes
    left_digest: [u8; 32], // Left child digest
    right_digest: [u8; 32], // Right child digest
    height: u8,            // Node height
}

// Storage key: node digest (32 bytes)
// Storage value: serialized TreeNode
```

#### Resolver Implementation
```rust
fn tree_resolver(digest: &[u8; 32]) -> ergo_avltree_rust::batch_node::Node {
    // Fetch node from fjall storage by digest
    // Deserialize and return node structure
    // Return error node if not found (should not happen in valid trees)
}
```

### 2. Operation Log

#### Log Structure
```rust
struct TreeOperation {
    sequence_number: u64,      // Monotonically increasing
    operation_type: OperationType, // Insert, Update
    timestamp: u64,           // Operation timestamp
    key: Vec<u8>,             // Note key
    value: Vec<u8>,           // Note value (for insert/update)
    previous_value: Option<Vec<u8>>, // For updates
    tree_root_before: [u8; 33], // Tree state before operation
    tree_root_after: [u8; 33],  // Tree state after operation
}

enum OperationType {
    Insert,
    Update,
}
```

#### Log Management
- **Append-only**: Operations are never modified
- **Sequential**: Operations stored in sequence number order
- **Compaction**: Old operations can be archived after checkpoints
- **Verification**: Each operation includes tree state for validation

### 3. Checkpoint System

#### Checkpoint Structure
```rust
struct TreeCheckpoint {
    checkpoint_id: u64,           // Unique identifier
    timestamp: u64,              // Checkpoint creation time
    tree_root: [u8; 33],         // Tree state at checkpoint
    operation_sequence: u64,     // Last included operation
    node_count: u64,             // Total nodes in tree
    serialized_tree: Vec<u8>,    // Optional: full tree serialization
    metadata: HashMap<String, String>, // Additional metadata
}
```

#### Checkpoint Strategy
- **Frequency**: Configurable (e.g., every 1000 operations or 1 hour)
- **Retention**: Keep last N checkpoints (configurable)
- **Verification**: Validate checkpoint integrity on creation
- **Compression**: Optional compression for storage efficiency

## Recovery Process

### Normal Recovery
```
1. Load latest checkpoint
2. Initialize tree from checkpoint
3. Replay operations since checkpoint from operation log
4. Verify final tree state matches expected
5. Resume normal operations
```

### Corruption Recovery
```
1. Detect corruption (invalid proofs, missing nodes)
2. Fall back to previous checkpoint
3. Replay operations with validation
4. If still corrupted, rebuild from operation log
5. If operation log corrupted, rebuild from note storage
```

### Full Rebuild
```
1. Create empty tree
2. Iterate through all notes in persistent storage
3. Insert each note into tree
4. Generate new operation log entries
5. Create checkpoint
```

## Storage Layout

### Fjall Database Structure
```
/data/trees/
├── nodes/              # Tree node storage
│   ├── partitions/
│   │   └── tree_nodes/
│   │       ├── config
│   │       ├── segments/
│   │       ├── levels
│   │       └── manifest
│   └── journals/
├── operations/         # Operation log
│   ├── partitions/
│   │   └── tree_operations/
│   └── journals/
├── checkpoints/        # Checkpoint storage
│   ├── partitions/
│   │   └── tree_checkpoints/
│   └── journals/
└── metadata/           # Tree metadata
    ├── partitions/
    │   └── tree_metadata/
    └── journals/
```

### Key Formats
- **Nodes**: `nodes/{digest_hex}`
- **Operations**: `operations/{sequence_number:016x}`
- **Checkpoints**: `checkpoints/{checkpoint_id:016x}`
- **Metadata**: `metadata/{key}`

## Performance Considerations

### Memory Management
- **Node Cache**: LRU cache for frequently accessed nodes
- **Batch Operations**: Group storage operations
- **Lazy Loading**: Load nodes on demand via resolver

### Storage Optimization
- **Compression**: Optional compression for large values
- **Deduplication**: Avoid storing duplicate node data
- **Compaction**: Regular storage compaction

### Recovery Performance
- **Checkpoint Frequency**: Balance between recovery time and storage
- **Operation Batching**: Batch replay operations
- **Parallel Recovery**: Parallel node loading where possible

## Error Handling

### Storage Errors
- **Node Not Found**: Trigger tree rebuild
- **Corruption**: Validate and repair
- **Full Storage**: Implement storage limits and cleanup

### Consistency Errors
- **State Mismatch**: Cross-verify with note storage
- **Proof Failure**: Rebuild tree from operation log
- **Sequence Gaps**: Detect and handle missing operations

## Monitoring and Maintenance

### Health Checks
- **Tree Integrity**: Regular proof validation
- **Storage Health**: Monitor disk usage and performance
- **Recovery Testing**: Periodic recovery drills

### Maintenance Operations
- **Backup**: Regular tree state backups
- **Compaction**: Storage optimization
- **Cleanup**: Remove old checkpoints and operations
- **Migration**: Version upgrade support

## Configuration

### Persistence Settings
```toml
[persistence]
# Node storage
node_cache_size = "1GB"
node_compression = true

# Operation log
operation_log_retention = "30 days"
operation_batch_size = 100

# Checkpoints
checkpoint_interval = "1000 operations"
checkpoint_retention = 10
checkpoint_compression = true

# Recovery
recovery_parallelism = 4
recovery_validation = true
```

## Integration with Basis Store

### Data Consistency
- **Atomic Operations**: Tree updates and note storage updates must be atomic
- **Cross-Verification**: Regular verification between tree and note storage
- **Recovery Coordination**: Coordinated recovery across all storage layers

### Backup Strategy
- **Coordinated Backups**: Backup tree state with note storage
- **Point-in-Time Recovery**: Support for consistent state recovery
- **Incremental Backups**: Efficient backup of changes only

## Testing Strategy

### Unit Tests
- Node storage operations
- Resolver functionality
- Operation log management
- Checkpoint creation and recovery

### Integration Tests
- End-to-end persistence
- Crash recovery scenarios
- Performance under load
- Corruption recovery

### Property Tests
- Tree invariants after persistence
- Recovery consistency
- Storage efficiency properties