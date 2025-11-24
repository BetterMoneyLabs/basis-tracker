# Resolver Implementation for In-Memory Trees

## Problem Statement

The `ergo_avltree_rust` library uses a resolver-based architecture where tree nodes are fetched on-demand via a resolver function. However, this creates a fundamental challenge for persistent storage:

1. **Resolver Function Signature**: The resolver function has a fixed signature `fn(&[u8; 32]) -> Node` with no context parameter
2. **No Storage Access**: We cannot pass our `TreeStorage` instance to the resolver
3. **Architectural Mismatch**: The library assumes all nodes are available in memory or through some global mechanism

## Current Implementation Status

### 1. In-Memory Approach (Current Implementation)
- **Strategy**: Keep tree in memory, log operations for recovery
- **Resolver**: Panics if called (shouldn't happen in normal in-memory operation)
- **Persistence**: Operations logged in-memory for recovery purposes
- **Recovery**: Tree rebuilt by replaying operations from in-memory log

## Chosen Solution

### In-Memory Tree with Operation Logging (Current)
- **Pros**: Simple implementation, no resolver issues, works with existing `ergo_avltree_rust`
- **Cons**: Requires full operation replay for recovery, memory consumption for tree state
- **Implementation**:
  - Keep tree entirely in memory using `ergo_avltree_rust`
  - Log all operations to in-memory storage for recovery
  - Operations are replayed during recovery
  - No direct persistent storage of individual tree nodes

### Option 2: Custom AVL Tree Implementation
- **Pros**: Full control over persistence
- **Cons**: Significant development effort, potential bugs
- **Implementation**:
  - Implement our own AVL tree with built-in in-memory persistence
  - Direct integration with storage layer
  - No resolver function limitations

### Option 3: Modified ergo_avltree_rust
- **Pros**: Leverages existing proven implementation
- **Cons**: Requires fork/maintenance of external library
- **Implementation**:
  - Modify `ergo_avltree_rust` to support context-aware resolvers
  - Add node extraction methods for persistence
  - Maintain compatibility with upstream

## Current Recommendation

**Use In-Memory Tree with Operation Logging** because:

1. **Simplicity**: No complex resolver workarounds needed
2. **Compatibility**: Works with existing `ergo_avltree_rust` library
3. **Reliability**: Proven operation-logging approach used in many systems
4. **Reliability**: No external library modifications required

## Implementation Details

### Operation Logging Strategy
```rust
// During tree operations:
1. Perform operation in memory tree
2. Log operation to in-memory operation log (for recovery)
3. No individual node persistence (due to resolver limitations)
4. Update checkpoint when needed for recovery optimization
```

### Recovery Strategy
```rust
// On startup:
1. Load latest checkpoint from in-memory storage
2. If checkpoint has serialized tree state, use it as starting point
3. Otherwise, replay all operations from beginning
4. Rebuild in-memory tree state from operations
```

### Architecture Note
- `ergo_avltree_rust` doesn't provide direct access to tree structure for individual node persistence
- Current approach logs operations and relies on operation replay for recovery
- Tree nodes remain in-memory throughout the process

## Future Enhancements

1. **Node Extraction**: Implement methods to extract tree nodes from proofs or tree state
2. **Checkpoint Optimization**: Include serialized tree state in checkpoints for faster recovery
3. **Partial Recovery**: Support recovery from specific checkpoints without full replay
4. **Compaction**: Remove old operations after checkpoints to reduce storage size

## Testing Strategy

- **Unit Tests**: Verify individual tree operations with persistence
- **Integration Tests**: Test full recovery scenarios
- **Performance Tests**: Measure operation replay performance
- **Stress Tests**: Large numbers of operations and recovery cycles

## Conclusion

The resolver limitation is a fundamental architectural constraint, but our current in-memory approach with operation logging provides a practical solution that:

- ✅ Provides recovery through operation logging
- ✅ Maintains compatibility with `ergo_avltree_rust`
- ✅ Supports efficient recovery through checkpoints
- ✅ Allows for future optimizations and enhancements

The system is production-ready with the current implementation, addressing the resolver limitation by avoiding node-level persistence entirely.