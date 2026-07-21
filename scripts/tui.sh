#!/bin/bash

# Basis TUI Wallet Startup Script
# Builds (if needed) and runs the interactive terminal wallet in the foreground

# Configuration
TUI_BINARY="target/release/basis-ui"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[STATUS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Ensure we are in the project root
if [ -f "Cargo.toml" ] && grep -q "basis-tracker\|members" Cargo.toml 2>/dev/null; then
    PROJECT_ROOT="."
else
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
    cd "$PROJECT_ROOT" || exit 1
fi

# Check if TUI binary exists
if [ ! -f "$TUI_BINARY" ]; then
    print_warning "TUI wallet binary not found: $TUI_BINARY"
    print_status "Building TUI wallet..."
    cargo build -p basis_app --release

    if [ $? -ne 0 ]; then
        print_error "Failed to build TUI wallet"
        exit 1
    fi
fi

print_status "Starting Basis TUI Wallet..."
print_info "Make sure the Basis server is running (./scripts/run_server.sh)"
print_info "Press Ctrl+C or use the TUI quit option to exit"

# Run the TUI wallet in the foreground
exec "$TUI_BINARY"
