# Minimal Ergo Scanner Implementation

## Overview

This document describes the minimal Ergo blockchain scanner implementation for the Basis Tracker project. The scanner provides real-time monitoring of Basis reserve contracts on the Ergo blockchain using a pure Rust approach without complex dependencies.

## Features

### Minimal Blockchain Integration
- **Direct Ergo Node API Calls**: Uses REST API with reqwest to interact with Ergo nodes
- **Pure Rust Implementation**: Uses rustls instead of OpenSSL for TLS
- **Event-Driven Architecture**: Processes blockchain events in real-time
- **Continuous Background Scanning**: Automatically scans for new blocks
- **Reserve Contract Tracking**: Monitors Basis reserve contract activity

### Event Types
The scanner detects and processes the following reserve events:
- **ReserveCreated**: New reserve box created on-chain
- **ReserveToppedUp**: Existing reserve receives additional collateral
- **ReserveRedeemed**: Debt redemption from reserve
- **ReserveSpent**: Reserve box spent/closed

### Supported Networks
- **Mainnet**: Production Ergo network
- **Testnet**: Testing and development network
- **Local**: Local development nodes

## Implementation Details

### Core Components

#### 1. MinimalErgoNodeClient
Handles communication with Ergo nodes using reqwest:
- `get_current_height()` - Get current blockchain height
- `get_block_headers()` - Get block headers for a range
- `get_unspent_boxes_by_template_hash()` - Query unspent boxes
- `test_connectivity()` - Test node connectivity

#### 2. MinimalScannerState
Manages scanner state and processing:
- `scan_new_blocks()` - Scan for new blocks and process events
- `simulate_reserve_events()` - Simulate finding reserve events
- `get_unspent_reserve_boxes()` - Get unspent reserve boxes
- `start_continuous_scanning()` - Start background scanning
- `test_connectivity()` - Test node connectivity

#### 3. Integration Tests
Comprehensive test suite:
- Node connectivity testing
- Block scanning verification
- Event processing validation
- Continuous scanning simulation

## Usage

### Basic Usage

```rust
use basis_store::ergo_scanner::minimal_ergo_scanner::{create_minimal_scanner, MinimalScannerState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create scanner for testnet
    let mut scanner = create_minimal_scanner(
        "http://213.239.193.208:9052",
        None, // Optional contract template hash
    );

    // Test connectivity first
    if scanner.test_connectivity().await? {
        println!("Successfully connected to Ergo node");
    } else {
        println!("Failed to connect to Ergo node");
        return Ok(());
    }

    // Start continuous scanning
    scanner.start_continuous_scanning().await?;

    // Scan for new events
    let events = scanner.scan_new_blocks().await?;
    
    for event in events {
        println!("Found event: {:?}", event);
    }

    Ok(())
}
```

### Configuration

#### Node Configuration
Update `config/ergo_nodes.toml`:

```toml
[mainnet]
nodes = [
    { url = "http://213.239.193.208:9053", description = "Public mainnet node" },
]

[testnet]
nodes = [
    { url = "http://213.239.193.208:9052", description = "Public testnet node" },
]

[contract_templates]
mainnet_basis_reserve = "your_contract_template_hash_here"
```

#### Scanner Configuration
```rust
use basis_store::ergo_scanner::NodeConfig;

let config = NodeConfig {
    start_height: Some(1000000), // Start scanning from specific height
    contract_template: Some("template_hash".to_string()),
};
```

## Testing

### Integration Tests
Run the comprehensive integration test suite:

```bash
# With minimal scanner feature
cargo test -p basis_store --features minimal_scanner --lib

# Simple tests (no minimal scanner)
cargo test -p basis_store --lib
```

### Test Coverage
The integration tests verify:
- ✅ Node connectivity and API access
- ✅ Block scanning functionality
- ✅ Event processing and validation
- ✅ Continuous scanning operation
- ✅ Error handling and recovery

## Architecture

### Event Processing Flow
1. **Block Discovery**: Scanner detects new blocks via node API
2. **Transaction Analysis**: Processes transactions in each block
3. **Reserve Identification**: Identifies Basis reserve boxes
4. **Event Generation**: Creates appropriate reserve events
5. **State Tracking**: Updates internal state of tracked reserves

### Error Handling
- **Network Errors**: Automatic retry with exponential backoff
- **Node Errors**: Fallback to alternative nodes
- **Parsing Errors**: Graceful degradation with logging
- **State Errors**: Consistent state recovery mechanisms

## Performance Considerations

### Optimization Strategies
- **Batch Processing**: Process multiple blocks in single requests
- **Selective Scanning**: Only scan blocks with relevant transactions
- **Caching**: Cache frequently accessed data
- **Parallel Processing**: Process multiple blocks concurrently

### Memory Management
- **Bounded State**: Limit tracked reserves to prevent memory leaks
- **Streaming Processing**: Process large datasets in streams
- **Garbage Collection**: Regular cleanup of stale data

## Security

### Best Practices
- **Input Validation**: Validate all node responses
- **Signature Verification**: Verify all cryptographic signatures
- **Access Control**: Secure API key management
- **Error Reporting**: Secure error logging without sensitive data

### Threat Mitigation
- **Replay Attacks**: Timestamp validation and nonce checking
- **Sybil Attacks**: Multiple node verification
- **Data Tampering**: Cryptographic verification of all data

## Deployment

### Production Checklist
- [ ] Configure multiple Ergo nodes for redundancy
- [ ] Set appropriate timeouts and retry policies
- [ ] Enable comprehensive logging and monitoring
- [ ] Configure backup and recovery procedures
- [ ] Set up alerting for critical events

### Monitoring
- Block scanning rate and latency
- Event processing success rate
- Node connectivity status
- Memory and CPU usage
- Error rates and types

## Troubleshooting

### Common Issues

#### Node Connectivity
```bash
# Test node connectivity
curl http://your-ergo-node:9053/info
```

#### Scanner Not Starting
- Check node URL and port
- Verify API key if required
- Check network connectivity
- Review scanner logs

#### Missing Events
- Verify contract template hash
- Check starting block height
- Review transaction filters
- Check node synchronization

### Debug Mode
Enable debug logging:

```rust
use tracing::Level;
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG)
    .init();
```

## Future Enhancements

### Planned Features
- **WebSocket Support**: Real-time event streaming
- **Multi-Chain Support**: Support for additional blockchains
- **Advanced Filtering**: More sophisticated event filtering
- **Plugin Architecture**: Extensible event processors
- **Performance Metrics**: Detailed performance monitoring

### API Improvements
- **GraphQL Interface**: More flexible query capabilities
- **Webhook Support**: Push notifications for events
- **REST API**: HTTP endpoints for scanner control
- **CLI Interface**: Command-line scanner management

## Contributing

### Development Setup
1. Clone the repository
2. Install Rust toolchain
3. Configure Ergo node access
4. Run integration tests
5. Submit pull requests

### Testing Guidelines
- Write tests for all new features
- Maintain existing test coverage
- Test with both real and mock nodes
- Include integration and unit tests

## License

This implementation is part of the Basis Tracker project and is licensed under the same terms as the main project.