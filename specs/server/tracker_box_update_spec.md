# Tracker Box Update Mechanism Specification

## Overview

This document specifies the implementation of a periodic tracker box update mechanism that runs every 10 minutes to update the R4 and R5 register values of the tracker box. This mechanism is implemented as a background service within the Basis Tracker server that submits actual transactions to the Ergo blockchain to update the tracker box commitment.

## Design Requirements

1. **Periodic Execution**: Run every 10 minutes (600 seconds) to periodically update tracker state commitment
2. **Register Updates**: Update R4 (tracker public key as GroupElement) and R5 (AVL tree root digest) registers in tracker box
3. **Blockchain Submission**: Submit actual transactions to update tracker box on Ergo blockchain
4. **Background Task**: Run as a dedicated background task to avoid blocking main server operations
5. **Thread Safety**: Ensure safe concurrent access to shared resources
6. **Error Handling**: Implement proper error handling and logging for failed update attempts
7. **Configuration**: Make update interval configurable via server configuration
8. **State Synchronization**: Maintain synchronization between tracker state changes and blockchain commitment

## Tracker Box Registers

The tracker box uses the following registers:

- **R4**: Tracker's public key (GroupElement / 33-byte compressed secp256k1 point)
  - Identifies the tracker server
  - Used for verifying tracker signatures on redemption transactions
- **R5**: AVL tree root digest (33 bytes inside a 37-byte serialized `SAvlTree` constant)
  - Commitment to all debt records in the tracker's state
  - Stores: `hash(issuer_pubkey || recipient_pubkey) -> totalDebt`
  - Updated whenever notes are added, modified, or transferred
  - See "R5 Register Serialization Format" below for exact byte layout
- **R6**: Tracker NFT ID (bytes) - identifies which tracker server this tracker box is linked to; must be preserved in the tracker box assets and output registers

## AVL Tree Commitment

The tracker's AVL tree (R5) stores cumulative debt records:

- **Key**: `blake2b256(issuer_pubkey_bytes || recipient_pubkey_bytes)` = 32 bytes
- **Value**: `longToByteArray(totalDebt)` = 8 bytes (big-endian encoded)

This on-chain commitment allows the reserve contract to verify that the tracker is attesting to a debt amount that is actually recorded in its state during redemption.

## Component Architecture

### Tracker Box Updater Service

The updater service is implemented as a stateless component with the following functionality:

1. **Timer Component**: Uses tokio's interval functionality to schedule updates every 10 minutes
2. **Shared State Access**: Interface to retrieve current AVL tree root and tracker public key from shared state
3. **Logger**: For outputting R4 and R5 register values in hex format
4. **Shutdown Handling**: Support for graceful shutdown via broadcast channels

### Configuration Parameters

The updater service is configurable with the following parameters:

```rust
pub struct TrackerBoxUpdateConfig {
    /// Interval in seconds between tracker box updates (default: 600 seconds = 10 minutes)
    pub update_interval_seconds: u64,
    /// Ergo node URL for API requests (required, no default provided)
    pub node_url: String,
    /// API key for Ergo node authentication (optional)
    pub api_key: Option<String>,
    /// Transaction fee in nanoERG paid by wallet inputs for each tracker update
    pub fee: u64,
    /// Optional change address for the fee-input change output
    pub change_address: Option<String>,
    /// Optional tracker secret key (32 bytes) used as a dlog secret when signing
    pub tracker_secret_key: Option<[u8; 32]>,
}
```

**Critical Requirement**: The `ergo_node_url` must be explicitly provided in the configuration. If it's not provided (empty string), the tracker will abort on startup with exit code 1. No default localhost value is used. This ensures the tracker cannot operate without proper connection to an Ergo node.`

### Shared State Structure

The system uses a thread-safe shared state to allow the updater to access necessary information:

```rust
pub struct SharedTrackerState {
    pub avl_root_digest: Arc<RwLock<[u8; 33]>>,
    pub tracker_pubkey: Arc<RwLock<[u8; 33]>>,
    pub tracker_box_id: Arc<RwLock<Option<String>>>,
    pub tracker_nft_id: Arc<RwLock<Option<String>>>,
}

impl SharedTrackerState {
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new([0x02u8; 33])), // Initialize with compressed pubkey marker
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new_with_tracker_key(tracker_pubkey: [u8; 33]) -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new(tracker_pubkey)),
            tracker_box_id: Arc::new(RwLock::new(None)),
            tracker_nft_id: Arc::new(RwLock::new(None)),
        }
    }
    
    pub fn set_avl_root_digest(&self, digest: [u8; 33]) {
        if let Ok(mut root_lock) = self.avl_root_digest.write() {
            *root_lock = digest;
        }
    }
    
    pub fn set_tracker_pubkey(&self, pubkey: [u8; 33]) {
        if let Ok(mut pubkey_lock) = self.tracker_pubkey.write() {
            *pubkey_lock = pubkey;
        }
    }

    pub fn set_tracker_box_id(&self, box_id: String) {
        if let Ok(mut id_lock) = self.tracker_box_id.write() {
            *id_lock = Some(box_id);
        }
    }

    pub fn set_tracker_nft_id(&self, nft_id: String) {
        if let Ok(mut id_lock) = self.tracker_nft_id.write() {
            *id_lock = Some(nft_id);
        }
    }
    
    pub fn get_avl_root_digest(&self) -> [u8; 33] {
        if let Ok(root_lock) = self.avl_root_digest.read() {
            *root_lock
        } else {
            [0u8; 33] // fallback
        }
    }
    
    pub fn get_tracker_pubkey(&self) -> [u8; 33] {
        if let Ok(pubkey_lock) = self.tracker_pubkey.read() {
            *pubkey_lock
        } else {
            [0x02u8; 33] // fallback with compressed pubkey marker
        }
    }

    pub fn get_tracker_box_id(&self) -> Option<String> {
        if let Ok(id_lock) = self.tracker_box_id.read() {
            id_lock.clone()
        } else {
            None
        }
    }

    pub fn get_tracker_nft_id(&self) -> Option<String> {
        if let Ok(id_lock) = self.tracker_nft_id.read() {
            id_lock.clone()
        } else {
            None
        }
    }
}
```

## Algorithm Flow

### Main Update Loop

The background task executes the following algorithm in a continuous loop:

1. **Wait for Interval**: Use tokio::time::interval to wait for the configured update period (10 minutes)
2. **Check Pending Transaction**: If a transaction was previously submitted but not yet confirmed, check its confirmation status via `/blockchain/transaction/byId`. Only update `last_submitted_digest` after confirmation.
3. **Access Shared State**: Read the current AVL tree root digest and tracker public key
4. **Skip Unchanged State**: If the AVL root digest hasn't changed since the last confirmed submission, skip the update cycle
5. **Find Tracker Box**: Query the blockchain for the tracker box using the tracker NFT ID via `/blockchain/box/unspent/byTokenId`
6. **Check On-Chain State**: If the on-chain tracker box already has the current AVL root digest in R5, skip the update
7. **Create Register Constants**:
   - R4: Tracker public key as EcPoint constant (33 bytes, compressed secp256k1 point) - identifies the tracker server
   - R5: Serialized `SAvlTree` constant containing the current AVL tree root digest (37 bytes total; see "R5 Register Serialization Format" below)
   - R6: Serialized `Coll[Byte]` constant containing the tracker NFT ID (preserved from the input tracker box)
8. **Build Unsigned Transaction**:
   - **Inputs**: the current tracker box (spends it) plus one or more wallet-owned P2PK/no-token boxes to pay the configured fee
   - **Outputs**: new tracker box with the same value and updated R4/R5/R6; fee output to the standard fee contract; optional change output to the change address
   - **inputsRaw**: serialized bytes of the tracker box and all fee inputs
   - **secrets.dlog**: the configured tracker secret key (hex) so the node can satisfy `proveDlog(trackerPubkey)`; may be omitted if the tracker key is already in the node wallet
9. **Submit Transaction**:
   - POST the unsigned transaction to `/wallet/transaction/sign` to obtain a signed transaction
   - POST the signed transaction to `/transactions` to broadcast it
   - Log the transaction ID on successful broadcast and mark it as pending confirmation
10. **Error Handling**:
    - If any step fails, log an appropriate ERROR message
    - Continue with the scheduled interval regardless of failures

### Asset Serialization in Payment Request

The tracker NFT token is preserved in the new tracker box output using the `assets` array with **camelCase** field names as required by the Ergo node API:

```json
{
  "tokenId": "000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f",
  "amount": 1
}
```

Using `token_id` instead of `tokenId` will result in a node error such as:
```
Attempt to decode value on failed cursor: DownField(tokenId),DownArray,DownField(assets),DownArray
```

### Transaction Confirmation Flow

To prevent submitting redundant updates during blockchain propagation:

1. After broadcasting a transaction, store its ID and the expected digest as `pending_tx`
2. On each subsequent cycle, check if the transaction is confirmed via `/blockchain/transaction/byId`
3. Only update `last_submitted_digest` after the transaction is confirmed on-chain
4. If the transaction is not yet found (404), continue waiting
5. If checking the transaction status fails, keep it pending and retry next cycle

### State Update Process

The tracker thread updates the shared state when tracker changes occur:

1. **Tracker Operations**: When notes are added or redeemed through the main tracker thread
2. **AVL Tree Updates**: After successful tracker operations, the AVL tree is updated and root digest recalculated
3. **State Synchronization**: The shared AVL root digest is updated to match the current AVL tree state using RwLock for thread safety
4. **Proof Generation**: Generate AVL tree proofs after each operation to ensure state is properly updated

## R5 Register Serialization Format

The tracker box R5 register contains a serialized `SAvlTree` constant. The exact byte layout follows the Sigma serialization produced by Scala's `ValueSerializer.serialize(AvlTreeConstant(tree))` (used in `scala/demo/src/TrackerBoxSetup.scala`):

| Field | Size | Description |
|-------|------|-------------|
| Type identifier | 1 byte | `0x64` (SAvlTree type code) |
| Root digest | 33 bytes | AVL tree root digest |
| Flags | 1 byte | AVL tree flags (e.g. `0x01` for insert-only) |
| Key length | VLQ | Key length in bytes (`0x20` for 32) |
| Value length | VLQ | `0x00` for variable / `None` |

For a tracker tree with `PlasmaParameters(32, None)` (32-byte keys, variable values) the serialized R5 is **37 bytes** (74 hex characters). The first 33 bytes after the type byte are the root digest.

### Example R5 Values

Empty tree (digest all zeros):
```
64000000000000000000000000000000000000000000000000000000000000000000012000
```

Real on-chain example observed during live testing:
```
64d5d44e152c7e42673dea178b918d9195c2ba689da94046384dc40c55a64c836a01012000
```

Bytes 1-33 of this example are the digest:
```
d5d44e152c7e42673dea178b918d9195c2ba689da94046384dc40c55a64c836a01
```

## Implementation Details

### Background Service Structure

The `TrackerBoxUpdater` is implemented as a stateless struct with a static `start` method:

```rust
pub struct TrackerBoxUpdater;

impl TrackerBoxUpdater {
    /// Start the periodic update service
    pub async fn start(
        config: TrackerBoxUpdateConfig,
        shared_tracker_state: SharedTrackerState,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), TrackerBoxUpdaterError> {
        // Implementation details as described in algorithm flow
        // - Uses tokio::time::interval for scheduling
        // - Tracks pending transactions for confirmation
        // - Uses tokio::select! for graceful shutdown
    }

    /// Find the tracker box on chain using the tracker NFT ID
    async fn find_tracker_box(
        config: &TrackerBoxUpdateConfig,
        tracker_nft_id: &str,
    ) -> Result<ErgoBoxApi, TrackerBoxUpdaterError> {
        // Query /blockchain/box/unspent/byTokenId/{tracker_nft_id}
        // Returns at most one box (NFT is unique)
        // Logs warning if multiple boxes found (indicates inconsistent state)
    }

    /// Submit a tracker box update transaction via /wallet/transaction/sign and broadcast via /transactions
    async fn submit_tracker_update(
        config: &TrackerBoxUpdateConfig,
        tracker_box: &ErgoBoxApi,
        tracker_pubkey: &[u8; 33],
        avl_root_digest: &[u8; 33],
    ) -> Result<String, TrackerBoxUpdaterError> {
        // Build R4 (GroupElement), R5 (SAvlTree), and R6 (Coll[Byte]) registers
        // Select wallet fee inputs covering config.fee
        // Fetch raw bytes for the tracker box and all fee inputs
        // Assemble UnsignedErgoTransaction and sign with /wallet/transaction/sign
        // Broadcast signed transaction with /transactions
        // Returns transaction ID
    }

    /// Check if a transaction has been confirmed on-chain
    pub async fn check_transaction_confirmation(
        config: &TrackerBoxUpdateConfig,
        tx_id: &str,
    ) -> Result<bool, TrackerBoxUpdaterError> {
        // Query /blockchain/transaction/byId/{tx_id}
        // Returns true if found (200), false if not yet confirmed (404)
    }
}
```
```

### AVL Tree State Management

The AVL tree state is properly maintained with proof generation after each operation:

```rust
impl AvlTreeState {
    /// Create a new AVL tree state with proper initialization
    pub fn new() -> Self {
        let tree = AVLTree::new(simple_resolver, 64, None);
        let mut prover = BatchAVLProver::new(tree, true);

        // Generate an initial proof to establish the empty tree state
        // This ensures the prover has an initial digest even for an empty tree
        let _ = prover.generate_proof();

        Self { prover }
    }

    /// Insert a key-value pair into the AVL tree
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        let operation = Operation::Insert(KeyValue {
            key: key.into(),
            value: value.into(),
        });

        self.prover
            .perform_one_operation(&operation)
            .map_err(|e| format!("AVL tree insert failed: {:?}", e))?;

        // Generate proof to commit changes to tree state and update root digest
        let _ = self.prover.generate_proof();

        Ok(())
    }

    /// Update an existing key-value pair
    pub fn update(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), String> {
        let operation = Operation::Update(KeyValue {
            key: key.into(),
            value: value.into(),
        });

        self.prover
            .perform_one_operation(&operation)
            .map_err(|e| format!("AVL tree update failed: {:?}", e))?;

        // Generate proof to commit changes to tree state and update root digest
        let _ = self.prover.generate_proof();

        Ok(())
    }

    /// Remove a key from the AVL tree
    pub fn remove(&mut self, key: Vec<u8>) -> Result<(), String> {
        let operation = Operation::Remove(key.into());

        self.prover
            .perform_one_operation(&operation)
            .map_err(|e| format!("AVL tree remove failed: {:?}", e))?;

        // Generate proof to commit changes to tree state and update root digest
        let _ = self.prover.generate_proof();

        Ok(())
    }

    /// Get the root digest of the AVL tree
    pub fn root_digest(&self) -> [u8; 33] {
        // Return the current root digest
        // After the fix to generate an initial proof in constructor,
        // the prover should always have a valid digest
        match self.prover.digest() {
            Some(digest) => {
                let mut result = [0u8; 33];
                result.copy_from_slice(&digest);
                result
            },
            None => {
                // This should not happen after our fix to generate initial proof in constructor
                // but we provide a fallback for safety
                let mut empty_digest = [0u8; 33];
                empty_digest[0] = 0x64; // SAvlTree type identifier
                empty_digest
            }
        }
    }
}
```

This ensures that the AVL tree root digest is properly updated after each operation, which is critical for the R5 register value. The AVL tree is now properly initialized with an initial proof to ensure it has a valid root digest even when empty.

### Redemption Transaction Builder

The redemption transaction builder now properly implements the transaction building logic:

```rust
pub struct RedemptionTransactionBuilder;

impl RedemptionTransactionBuilder {
    /// Build unsigned Ergo redemption transaction data with complete validation
    pub fn build_unsigned_redemption_transaction(
        reserve_box_id: &str,
        tracker_box_id: &str,
        tracker_nft_id: &str,
        note: &crate::IouNote,
        recipient_address: &str,
        avl_proof: &[u8],
        issuer_sig: &[u8],
        tracker_sig: &[u8],
        issuer_pubkey: &crate::PubKey,
        context: &TxContext,
        reserve_lookup_proof: Option<Vec<u8>>,
        tracker_lookup_proof: Vec<u8>,
    ) -> Result<RedemptionTransactionData, TransactionBuilderError> {
        // Implementation that creates proper Ergo transaction structure
        // with full validation, inputs, outputs, data inputs, and context extensions
        // including R6 register preservation with tracker NFT ID
    }

    /// Build actual Ergo redemption transaction JSON from transaction data
    pub fn build_redemption_transaction(
        tx_data: &RedemptionTransactionData,
    ) -> Result<Vec<u8>, TransactionBuilderError> {
        // Implementation that creates proper Ergo transaction JSON
        // with inputs, outputs, data inputs, and context extensions
        // including R6 register preservation with tracker NFT ID
    }
}
```

The redemption transaction builder now includes proper validation, transaction structure creation, and serialization of all required components for the Basis redemption process, including R6 register preservation with tracker NFT ID.

### Integration with Server Startup

The tracker box updater is integrated into the server startup flow:

1. **Node Configuration Validation**: Verify that `ergo.node.node_url` is provided in config; abort with exit code 1 if missing
2. **Shared State Creation**: Create `SharedTrackerState` instance during server initialization
3. **Tracker Thread Integration**: Update shared state whenever tracker operations occur
4. **Updater Service Startup**: Spawn the updater task as a background tokio task with proper node configuration
5. **Shutdown Handling**: Use broadcast channels for graceful shutdown coordination
6. **Tracker NFT ID Initialization**: Set `tracker_nft_id` in shared state from `config.ergo.tracker_nft_id` if configured
7. **Tracker Box ID Tracking**: Set `tracker_box_id` in shared state from scanner-discovered boxes for reference

### Tracker Thread Integration

The main tracker thread is enhanced to update the shared state:

1. **AddNote Command**: After successfully adding a note to the tracker, update the shared AVL root digest via update_state() call
2. **CompleteRedemption Command**: After successfully completing a redemption, update the shared AVL root digest via update_state() call
3. **AVL Tree Operations**: Each AVL tree operation (insert/update/delete) triggers proof generation to update internal tree state
4. **State Consistency**: Ensure the shared state remains consistent with the main tracker state and AVL tree root

## Logging Specifications

### Log Messages

The service outputs the following log messages:

1. **Transaction Submitted** (INFO level):
   - Message: "Tracker box update submitted. Transaction ID: {tx_id}, Box ID: {box_id}. Waiting for confirmation..."
   - Context: Transaction ID and box ID

2. **Transaction Confirmed** (INFO level):
   - Message: "Transaction {tx_id} confirmed on chain. Update complete."
   - Context: Transaction ID

3. **State Unchanged** (INFO level):
   - Message: "AVL root digest unchanged, skipping redundant update"
   - Context: Current digest value

4. **On-Chain State Current** (INFO level):
   - Message: "On-chain tracker box already has current AVL root digest"
   - Context: Current digest value

5. **Service Startup** (INFO level):
   - Message: "Tracker box updater started with {interval_seconds}s interval"
   - Context: Configuration parameters

6. **Service Shutdown** (INFO level):
   - Message: "Tracker box updater shutdown signal received" / "Tracker box updater stopped"
   - Context: None

7. **Errors** (ERROR level):
   - Message: "Failed to submit tracker box update: {error_message}"
   - Context: Error details

### Log Format

All log messages follow the standard application logging format with timestamp, level, and structured fields.

## Error Handling

### Expected Errors

The service handles the following error conditions:

1. **State Access Errors**: Failures to read from shared state RwLock
2. **Configuration Errors**: Invalid configuration parameters (missing node URL, invalid tracker NFT ID)
3. **HTTP Errors**: Network failures, node unreachable, API errors (5xx, 4xx)
4. **No Tracker Box Found**: Tracker NFT ID not found on chain
5. **No Fee Inputs**: Wallet has no suitable P2PK/no-token boxes to pay the update fee
6. **Insufficient Fee Inputs**: Wallet boxes don't cover the configured fee
7. **Signing Failed**: `/wallet/transaction/sign` rejected the unsigned transaction (bad inputs, missing secrets, etc.)
8. **Broadcast Failed**: `/transactions` rejected the signed transaction
9. **Transaction Not Found**: Submitted transaction ID not found on chain after extended waiting
10. **Serialization Errors**: Failed to decode ergoTree hex, parse ergoTree bytes, or encode addresses

### Error Recovery

- All errors are logged but do not terminate the background service
- The service continues running and attempting updates at the next scheduled interval
- The service gracefully handles RwLock access failures with fallback values
- Pending transactions are retried until confirmed or a new update is needed

## Security Considerations

1. **Thread Safety**: Proper use of RwLock for concurrent access to shared state
2. **Resource Management**: Proper handling of async resources and channels
3. **Log Security**: No sensitive cryptographic information exposed in logs
4. **Rate Limiting**: Built-in 10-minute interval prevents excessive resource usage
5. **Secret Handling**: The configured `tracker_secret_key` is passed only to the node's `/wallet/transaction/sign` endpoint as a `dlog` secret; it is never logged or broadcast
6. **Pending Transaction State**: Prevents duplicate submissions while waiting for confirmation

## Performance Characteristics

1. **Execution Frequency**: Once every 10 minutes (configurable)
2. **Resource Usage**: Minimal - only reads state and submits transactions, uses efficient RwLock for state access
3. **Non-blocking Operations**: Uses `tokio::select!` for concurrent interval and shutdown handling
4. **Memory Usage**: Constant - no accumulation of data between executions
5. **Transaction Confirmation Wait**: Up to 10 minutes between confirmation checks (configurable via interval)

## Monitoring and Observability

1. **Logging**: Comprehensive logging for debugging and monitoring the periodic updates
2. **Tracing**: Integration with existing tracing infrastructure using INFO level for updates
3. **Configuration**: Interval configuration allows for adjustment based on monitoring needs

## Integration Points

### Main Server Integration

1. **State Initialization**: Create shared tracker state before tracker thread initialization
2. **Thread Sharing**: Pass shared state to both tracker thread and updater service
3. **Update Coordination**: Tracker thread updates shared state on successful operations
4. **Tracker Scanner Integration**: The tracker scanner discovers the latest tracker box and sets `tracker_box_id` in shared state
5. **NFT ID Configuration**: The `tracker_nft_id` from `config.ergo.tracker_nft_id` is set in shared state on startup

### Tracker Thread Integration

1. **State Updates**: Update shared AVL root digest after successful `AddNote` operations
2. **Redemption Handling**: Update shared AVL root digest after successful `CompleteRedemption` operations
3. **Synchronization**: Use thread-safe access to shared state to prevent data races
4. **Initialization**: The tracker thread initializes the shared AVL root digest with the empty tree state on startup
5. **Event Store**: Tracker events (AddNote, CompleteRedemption) are stored in the event store for audit trail

## Future Extensions

This implementation provides a foundation for future extensions including:

1. **Retry Logic with Exponential Backoff**: Retry failed submissions with increasing delays for transient errors (network timeouts, node temporarily unavailable)
2. **Fee Validation**: Add minimum value checks or fee estimation to ensure the tracker box value is sufficient to cover miner fees
3. **Configuration Management**: Add runtime configuration updates for interval and other parameters
4. **Metrics Collection**: Add metrics for monitoring update frequency, success rates, and confirmation latency
5. **Multiple Tracker Box Handling**: Implement proper resolution strategy if multiple tracker boxes are found (indicates chain reorg or race condition)

This specification accurately reflects the implemented tracker box update mechanism that periodically submits transactions to the Ergo blockchain to update R4 and R5 register values, with proper transaction confirmation tracking and error handling.