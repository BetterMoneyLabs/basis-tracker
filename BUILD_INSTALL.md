# Basis Tracker - Build and Installation Guide

## Table of Contents
1. [Prerequisites](#prerequisites)
2. [Building the Server](#building-the-server)
3. [Building the Client](#building-the-client)
4. [Installation Methods](#installation-methods)
5. [Configuration](#configuration)
6. [Running the Server](#running-the-server)
7. [Troubleshooting](#troubleshooting)

## Prerequisites

### System Requirements
- **Operating System**: Linux, macOS, or Windows with WSL
- **Rust**: Latest stable version (1.70 or higher)
- **Cargo**: Included with Rust installation
- **Git**: For source code management
- **Ergo Node**: Access to an Ergo node (local or remote)

### Install Rust
```bash
# Install Rust using rustup (recommended)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### Install Dependencies
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential pkg-config libssl-dev git

# macOS (with Homebrew)
brew install pkg-config openssl
```

## Building the Server

### Clone the Repository
```bash
git clone https://github.com/chaincash/basis-tracker.git
cd basis-tracker
```

### Build the Server Binary
```bash
# Build debug version (faster build, slower execution)
cargo build -p basis_server

# Build release version (slower build, faster execution)
cargo build -p basis_server --release

# The binary will be available at:
# - Debug: target/debug/basis_server
# - Release: target/release/basis_server
```

### Build Specific Features
The server supports various features:

```bash
# Build with Ergo scanner support (default)
cargo build -p basis_server --features ergo_scanner

# Build with all features
cargo build -p basis_server --all-features
```

### Verify Server Build
```bash
# Check if the binary was created
ls -la target/release/basis_server

# Run server with help to verify it works
./target/release/basis_server --help
```

## Building the Client

### Build the CLI Client
```bash
# Build debug version
cargo build -p basis_cli

# Build release version
cargo build -p basis_cli --release

# The binary will be available at:
# - Debug: target/debug/basis_cli
# - Release: target/release/basis_cli
```

### Build with Default Features
The client includes binary building by default:

```bash
cargo build -p basis_cli --features bin
```

### Create an Alias (Optional)
To use the CLI more easily, you can create an alias:

```bash
# Add to your shell profile (.bashrc, .zshrc, etc.)
alias basis-cli='cargo run -p basis_cli --'

# Or copy the binary to a location in your PATH
sudo cp target/release/basis_cli /usr/local/bin/basis-cli
```

## Installation Methods

### Method 1: Build from Source (Recommended)
This method gives you the most control and ensures you have the latest version:

```bash
# Clone and build both server and client
git clone https://github.com/chaincash/basis-tracker.git
cd basis-tracker

# Build release versions
cargo build -p basis_server --release
cargo build -p basis_cli --release

# Binaries will be in target/release/
ls target/release/basis_server target/release/basis_cli
```

### Method 2: Install Using Cargo
```bash
# Install in your local Cargo bin directory
cargo install --path crates/basis_server
cargo install --path crates/basis_cli

# Or run directly without installing
cargo run -p basis_server
cargo run -p basis_cli
```

### Method 3: Using the Run Scripts
The project includes convenient scripts for server management:

```bash
# Make scripts executable
chmod +x run_server.sh server_status.sh stop_server.sh

# Start the server (automatically builds if needed)
./run_server.sh

# Check server status
./server_status.sh

# Stop the server
./stop_server.sh
```

## Configuration

### Server Configuration
Before running the server, you need to set up configuration:

1. **Create configuration file**:
```bash
# Copy example configuration
mkdir -p config
cp config/basis.toml.example config/basis.toml  # if available
```

2. **Configure the Tracker NFT ID**:
```toml
[ergo]
# This NFT identifies your tracker server - required for reserve operations
tracker_nft_id = "your_tracker_nft_token_id_here"
basis_reserve_contract_p2s = "your_reserve_contract_p2s_address"
```

3. **Set Ergo Node Configuration**:
```toml
[ergo.node]
url = "http://your-ergo-node:9053"
api_key = "your_node_api_key"
timeout_secs = 30
```

### Client Configuration
The client stores configuration in `~/.basis/cli.toml` and creates it automatically when first run.

## Running the Server

### Direct Binary Execution
```bash
# Run the server directly
./target/release/basis_server

# Run with custom configuration file
./target/release/basis_server --config /path/to/config.toml

# Run with custom port
BASIS_SERVER_PORT=8080 ./target/release/basis_server
```

### Using Run Scripts (Recommended)
```bash
# Start the server in background with logging
./run_server.sh

# Check if server is running
./server_status.sh

# View server logs
tail -f server.log

# Stop the server
./stop_server.sh
```

### Server Address and Port
- **Default Address**: `http://0.0.0.0:3048`
- **Health Check**: `GET /`
- **API Endpoint**: `http://localhost:3048` (by default)

## Using the Client

### Basic Client Usage
```bash
# Create an account
basis-cli account create my_account

# List accounts
basis-cli account list

# Check server status
basis-cli status

# Get account info
basis-cli account info
```

### Client with Custom Server
```bash
# Connect to a different server
basis-cli --server-url http://my-server:3048 status

# Set default server in configuration
# Edit ~/.basis/cli.toml and modify server_url
```

## Development Setup

### Running in Development Mode
```bash
# Start server in development mode (auto-reload on changes)
cargo watch -x "run -p basis_server"

# Run tests
cargo test

# Run specific crate tests
cargo test -p basis_server
cargo test -p basis_cli
```

### Building with Logging
```bash
# Build with debug symbols and logging
cargo build -p basis_server --features "log debug"
```

## Docker Support (Optional)

If Docker support is available in your version:

```dockerfile
# Dockerfile example
FROM rust:latest

WORKDIR /app
COPY . .
RUN cargo build -p basis_server --release
EXPOSE 3048
CMD ["./target/release/basis_server"]
```

## Troubleshooting

### Common Build Issues

**Issue**: `error: linking with cc failed`
```bash
# Solution: Install build tools
sudo apt install build-essential pkg-config libssl-dev  # Ubuntu/Debian
```

**Issue**: `error: failed to run custom build command`
```bash
# Solution: Install OpenSSL development libraries
sudo apt install libssl-dev  # Ubuntu/Debian
brew install openssl  # macOS
```

**Issue**: `error: could not find .rs file`
```bash
# Solution: Make sure you're in the project root directory
cd basis-tracker
```

### Common Configuration Issues

**Issue**: Server won't start due to missing tracker NFT
```bash
# Solution: Set the tracker_nft_id in config/basis.toml
# Create an NFT on Ergo blockchain and use its token ID
```

**Issue**: Client can't connect to server
```bash
# Solution: Verify server is running
curl http://localhost:3048/
# Check firewall settings and server logs
```

### Verifying Installation

**Verify Server Build**:
```bash
./target/release/basis_server --version
```

**Verify Client Build**:
```bash
./target/release/basis_cli --version
```

**Test Server Connection**:
```bash
# Start server
./run_server.sh

# Test with client
basis-cli status

# Test with curl
curl http://localhost:3048/
```

## Updating

### Update to Latest Version
```bash
# Pull latest changes
git pull origin main

# Clean previous builds
cargo clean

# Build updated versions
cargo build -p basis_server --release
cargo build -p basis_cli --release
```

## Useful Commands Summary

| Command | Description |
|---------|-------------|
| `cargo build -p basis_server --release` | Build server binary |
| `cargo build -p basis_cli --release` | Build client binary |
| `./run_server.sh` | Start server in background |
| `./server_status.sh` | Check server status |
| `./stop_server.sh` | Stop the server |
| `basis-cli --help` | Show client help |
| `./target/release/basis_server` | Run server directly |
| `cargo run -p basis_server` | Run server from cargo |

Your Basis Tracker installation is now complete! The server provides API endpoints for managing IOU notes and reserves, while the client provides a convenient command-line interface for interacting with the system.