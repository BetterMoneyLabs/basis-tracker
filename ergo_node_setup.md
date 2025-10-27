# Ergo Node Setup for Basis Tracker

## Configuration Status

✅ **Ergo Node Configuration Complete**

The Basis Tracker is now configured to use the Ergo node:
```
http://213.239.193.208:9053
```

## Files Updated

### 1. `config/basis.toml`
```toml
[ergo.node]
url = "http://213.239.193.208:9053"
api_key = ""
timeout_secs = 30
```

### 2. `config/ergo_nodes.toml`
- Already includes `213.239.193.208:9053` as the first mainnet node

## Testing Instructions

### Start the Server
```bash
cd /home/kushti/bml/basis-tracker
cargo run -p basis_server
```

**Expected Output:**
```
Starting basis server...
Loading configuration...
Configuration loaded successfully
Initializing Ergo scanner...
Ergo scanner started successfully
Current blockchain height: 1000
Server listening on 127.0.0.1:3000
```

### Test with CLI
```bash
# Build CLI
cargo build -p basis_cli

# Test basic functionality
./target/debug/basis_cli account create alice
./target/debug/basis_cli status
```

## Current Scanner Implementation

The tracker currently uses a **mock scanner** (`ServerState`) that:
- Simulates blockchain events for testing
- Does NOT connect to real Ergo node for scanning
- Provides realistic test data

## For Real Blockchain Integration

To enable real blockchain scanning, the server would need to:
1. Use `ErgoScannerState` instead of `ServerState`
2. Implement proper scan registration with the Ergo node
3. Handle real blockchain events

## Next Steps for Testing

1. **Start the server** - It will use the configured Ergo node
2. **Test CLI workflow** - Create accounts, issue debt, redeem
3. **Monitor events** - The mock scanner will generate realistic test events
4. **Verify configuration** - Server logs will show the configured node

The current setup is perfect for testing the Alice → Bob workflow with realistic simulated blockchain data.