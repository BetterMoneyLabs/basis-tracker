# Basis Tracker Configuration

## Overview

This document describes the configuration options for the Basis Tracker server and CLI.

## Configuration File

The main configuration file is `config/basis.toml`. The server will look for this file in the following locations:

1. Current working directory: `config/basis.toml`
2. Environment variables with `BASIS_` prefix
3. Default values

## Configuration Sections

### Server Configuration

```toml
[server]
host = "0.0.0.0"        # Host address to bind to
port = 3048             # Port to listen on
database_url = "sqlite:data/basis.db"  # Database path (optional)
```

### Ergo Blockchain Configuration

```toml
[ergo]
# Basis reserve contract P2S address
basis_reserve_contract_p2s = "AtC4LmBhPrHQJkS4yxCS5pxFoxLvZ7Jhbp4ARvah8LzyXWzRYGXnd7szw6RQiS9npVUidW8nQK6EMHQRfPBFKP7LKxYDw4FVsLDpeArKQ8yk85iJDgDR3QRdVwqSXtQkYVDDsKJA8NXh8caZYBLSdhqAvsejn3bTE2RzLYWdt2xsuB9BF9GJm8GjBwH6WGcBQaJtzPTe4rKzgFqT1nFyHJsAiT6EWv3dPivf519CA6oKBm9deAfe8xqvSRjbBL147E2bJE5MtCu5TmDp3Vv4YQV3AXuQawYemvQxZxQCzyEBCTcYpegZjJaNSpYYBRRFUevjKmvyyBHgwSnLqKHk1BN2gpAh4d2EXxRoXbSLALXoSjHQ3kDUtpvjiRpFh1BvC8YxY5vmTWzhtvpzt6evHcvT7Gqp6FvcHuwKw3m4AxsUVdhgHEuXiXK6qTjKDtdf7X5HjNChLLvKhuwvyjzswweopJnARkqzy2UKwdMQr9VYtJ5qwxngqd9CfJaP3yVjnSLF7jQPThFUvSW7TUijPnmzTHHVH6sPArDhTV7tsqxQifPrUC"

# Starting block height for scanning (legacy)
start_height = 0

# Tracker NFT ID (hex-encoded) - REQUIRED for reserve creation and redemption
# This NFT identifies the tracker server and must be set in reserve contract R6 register
# Example: tracker_nft_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
tracker_nft_id = ""

[ergo.node]
url = "http://159.89.116.15:11088"   # Ergo node URL
api_key = "hello"                    # API key for authenticated nodes
timeout_secs = 30                    # Request timeout in seconds
```

## Tracker NFT Configuration

### What is the Tracker NFT?

The Tracker NFT is a critical component of the Basis system that:

1. **Identifies your tracker server** on the blockchain
2. **Links reserves to your tracker** via the R6 register
3. **Prevents unauthorized redemptions** by verifying tracker identity
4. **Enables multi-tracker support** (future feature)

### How to Set Up Tracker NFT

1. **Create an NFT** on the Ergo blockchain
   - Use any NFT creation tool or wallet
   - The NFT should be unique to your tracker instance

2. **Configure the NFT ID** in `config/basis.toml`
   ```toml
   tracker_nft_id = "your_nft_token_id_here"
   ```

3. **Use the NFT in reserve creation**
   - When creating reserves, the NFT ID must be set in the R6 register
   - This links the reserve to your specific tracker

### Reserve Contract Registers

When creating a reserve contract box, you must set these registers:

- **R4**: Issuer's public key (GroupElement)
- **R5**: AVL tree of redeemed timestamps (initially empty)
- **R6**: Tracker NFT ID (from your configuration)

## Environment Variables

All configuration options can also be set via environment variables with the `BASIS_` prefix:

```bash
export BASIS_SERVER_HOST="0.0.0.0"
export BASIS_SERVER_PORT=3048
export BASIS_ERGO_BASIS_RESERVE_CONTRACT_P2S="your_reserve_contract_p2s"
export BASIS_ERGO_TRACKER_NFT_ID="your_tracker_nft_id"
export BASIS_ERGO_NODE_URL="http://your-node:9053"
```

## Default Configuration

If no configuration file is found, the server uses these defaults:

```toml
[server]
host = "127.0.0.1"
port = 3048
database_url = "sqlite:data/basis.db"

[ergo]
basis_reserve_contract_p2s = ""
start_height = 0
tracker_nft_id = ""

[ergo.node]
url = "http://159.89.116.15:11088"
api_key = "hello"
timeout_secs = 30
```

## Verification

To verify your configuration is working:

1. **Start the server**: `cargo run -p basis_server`
2. **Check server logs** for configuration loading messages
3. **Test with CLI**: `basis-cli status`
4. **Verify tracker NFT**: Check that reserves can be created and redeemed

## Troubleshooting

### Common Issues

1. **"Tracker NFT not configured"**
   - Ensure `tracker_nft_id` is set in your configuration
   - The NFT must exist on the blockchain

2. **"Invalid tracker NFT"**
   - Verify the NFT ID is correctly hex-encoded
   - Check that the NFT exists and is owned by you

3. **"Reserve creation failed"**
   - Ensure R6 register contains the correct tracker NFT ID
   - Verify the reserve contract template is correct

### Logging

Enable debug logging to see configuration details:

```bash
RUST_LOG=debug cargo run -p basis_server
```
