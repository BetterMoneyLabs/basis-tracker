# Tracker Box Update Mechanism Specification

## Overview

This document specifies the implementation of a periodic tracker box update mechanism that runs every 10 minutes to log the R4 and R5 register values of the tracker box. This mechanism is implemented as a background service within the Basis Tracker server without submitting actual transactions to the Ergo blockchain.

## Design Requirements

1. **Periodic Execution**: Run every 10 minutes (600 seconds) to periodically update tracker state
2. **Register Logging**: Log hex-encoded values of R4 and R5 registers to application logs
3. **No Blockchain Submission**: For the initial implementation, only log values without submitting transactions
4. **Background Task**: Run as a dedicated background task to avoid blocking main server operations
5. **Thread Safety**: Ensure safe concurrent access to shared resources
6. **Error Handling**: Implement proper error handling and logging for failed update attempts
7. **Configuration**: Make update interval configurable via server configuration
8. **State Synchronization**: Maintain synchronization between tracker state changes and logging

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
    /// Flag to enable/disable the tracker box updater (default: true)
    pub enabled: bool,
    /// Flag to enable actual transaction submission (default: false for logging-only mode)
    pub submit_transaction: bool,
}
```

### Shared State Structure

The system uses a thread-safe shared state to allow the updater to access necessary information:

```rust
pub struct SharedTrackerState {
    pub avl_root_digest: Arc<RwLock<[u8; 33]>>,
    pub tracker_pubkey: Arc<RwLock<[u8; 33]>>,
}

impl SharedTrackerState {
    pub fn new() -> Self {
        Self {
            avl_root_digest: Arc::new(RwLock::new([0u8; 33])), // Initialize with zeros
            tracker_pubkey: Arc::new(RwLock::new([0x02u8; 33])), // Initialize with compressed pubkey marker
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
}
```

## Algorithm Flow

### Main Update Loop

The background task executes the following algorithm in a continuous loop:

1. **Wait for Interval**: Use tokio::time::interval to wait for the configured update period (10 minutes)
2. **Access Shared State**: Read the current AVL tree root digest and tracker public key
3. **Construct Register Values**:
   - R4: Tracker public key (33 bytes, compressed secp256k1 point)
   - R5: Hex-encoded AVL tree root digest (33 bytes as returned by tracker state)
4. **Log Register Values**:
   - Construct hex-encoded strings for R4 and R5 values
   - Log these values to the application log with INFO level
   - Include timestamp and tracker state information in log message
5. **Error Handling**:
   - If any step fails, log an appropriate ERROR message
   - Continue with the scheduled interval regardless of failures

### State Update Process

The tracker thread updates the shared state when tracker changes occur:

1. **Tracker Operations**: When notes are added or redeemed through the main tracker thread
2. **State Updates**: After successful tracker operations, update the shared AVL root digest
3. **Synchronization**: Use RwLock for thread-safe access to shared state between threads

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
    }
}
```

### Integration with Server Startup

The tracker box updater is integrated into the server startup flow:

1. **Shared State Creation**: Create `SharedTrackerState` instance during server initialization
2. **Tracker Thread Integration**: Update shared state whenever tracker operations occur
3. **Updater Service Startup**: Spawn the updater task as a background tokio task
4. **Shutdown Handling**: Use broadcast channels for graceful shutdown coordination

### Tracker Thread Integration

The main tracker thread is enhanced to update the shared state:

1. **AddNote Command**: After successfully adding a note to the tracker, update the shared AVL root digest
2. **CompleteRedemption Command**: After successfully completing a redemption, update the shared AVL root digest
3. **State Consistency**: Ensure the shared state remains consistent with the main tracker state

## Logging Specifications

### Log Messages

The service outputs the following log messages:

1. **Periodic Updates** (INFO level):
   - Message: "Tracker Box Update: R4={hex_value}, R5={hex_value}, timestamp={unix_timestamp}, root_digest={digest}"
   - Context: Current time and state values

2. **Service Startup** (INFO level):
   - Message: "Starting tracker box updater with interval {interval_seconds} seconds"
   - Context: Configuration parameters

3. **Service Shutdown** (INFO level):
   - Message: "Tracker box updater shutdown signal received" / "Tracker box updater stopped"
   - Context: None

4. **Errors** (ERROR level):
   - Message: "Failed to update tracker box: {error_message}"
   - Context: Error details

### Log Format

All log messages follow the standard application logging format with timestamp, level, and structured fields.

## Error Handling

### Expected Errors

The service handles the following error conditions:

1. **State Access Errors**: Failures to read from shared state RwLock
2. **Configuration Errors**: Invalid configuration parameters
3. **Logging Errors**: Failures in writing log messages

### Error Recovery

- All errors are logged but do not terminate the background service
- The service continues running and attempting updates at the next scheduled interval
- The service gracefully handles RwLock access failures with fallback values

## Security Considerations

1. **Thread Safety**: Proper use of RwLock for concurrent access to shared state
2. **Resource Management**: Proper handling of async resources and channels
3. **Log Security**: No sensitive cryptographic information exposed in logs
4. **Rate Limiting**: Built-in 10-minute interval prevents excessive resource usage

## Performance Characteristics

1. **Execution Frequency**: Once every 10 minutes (configurable)
2. **Resource Usage**: Minimal - only reads state and writes logs, uses efficient RwLock for state access
3. **Non-blocking Operations**: Uses `tokio::task::spawn_blocking` for state access to prevent blocking
4. **Memory Usage**: Constant - no accumulation of data between executions

## Monitoring and Observability

1. **Logging**: Comprehensive logging for debugging and monitoring the periodic updates
2. **Tracing**: Integration with existing tracing infrastructure using INFO level for updates
3. **Configuration**: Interval configuration allows for adjustment based on monitoring needs

## Integration Points

### Main Server Integration

1. **State Initialization**: Create shared tracker state before tracker thread initialization
2. **Thread Sharing**: Pass shared state to both tracker thread and updater service
3. **Update Coordination**: Tracker thread updates shared state on successful operations

### Tracker Thread Integration

1. **State Updates**: Update shared AVL root digest after successful `AddNote` operations
2. **Redemption Handling**: Update shared AVL root digest after successful `CompleteRedemption` operations
3. **Synchronization**: Use thread-safe access to shared state to prevent data races

## Future Extensions

This implementation provides a foundation for future extensions including:

1. **Actual Transaction Submission**: Implement blockchain transaction submission in addition to logging
2. **Configuration Management**: Add runtime configuration updates for interval and other parameters
3. **Enhanced Logging**: Add more detailed context to log messages
4. **Metrics Collection**: Add metrics for monitoring update frequency and success rates

This specification accurately reflects the implemented tracker box update mechanism that runs every 10 minutes, logging R4 and R5 register values to application logs without submitting actual blockchain transactions.