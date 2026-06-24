#!/bin/bash

###############################################################################
# SilverCents Demo - Setup Script
# 
# This script sets up the demo environment for SilverCents:
# - Creates merchant (Alice) and customer (Bob) accounts
# - Initializes reserve tracking
# - Configures server connectivity
###############################################################################

set -e

# Configuration
SERVER_URL="${SERVER_URL:-http://localhost:3048}"
ERGO_NODE_URL="${ERGO_NODE_URL:-http://localhost:9053}"

# Demo database directory
DEMO_DB_DIR="/tmp/silvercents_demo"
DEMO_LOG_DIR="$DEMO_DB_DIR/logs"
DEMO_STATE_DIR="$DEMO_DB_DIR/state"

# Account details
ALICE_NAME="Alice_Merchant"
BOB_NAME="Bob_Customer"

# Reserve configuration
ALICE_RESERVE_AMOUNT=1000000
ALICE_NFT_ID="0000000000000000000000000000000000000000000000000000000000000001"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

###############################################################################
# Helper Functions
###############################################################################

log_info() {
    echo -e "${BLUE}ℹ${NC}  $1"
}

log_success() {
    echo -e "${GREEN}✓${NC}  $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC}  $1"
}

log_error() {
    echo -e "${RED}✗${NC}  $1"
}

log_title() {
    echo ""
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
}

###############################################################################
# Main Setup
###############################################################################

main() {
    log_title "SilverCents Demo - Setup"
    
    # Check prerequisites
    check_prerequisites
    
    # Initialize demo directories
    initialize_directories
    
    # Check server connectivity
    check_server_connectivity
    
    # Create accounts
    create_accounts
    
    # Display setup summary
    display_setup_summary
    
    log_success "Setup complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Run Alice (merchant) in one terminal:   ./silvercents_issuer.sh"
    echo "  2. Run Bob (customer) in another terminal:  ./silvercents_receiver.sh"
    echo "  3. Then run redemption when ready:          ./silvercents_redeem.sh"
    echo ""
    echo "Or run the complete demo in one go:          ./silvercents_complete_demo.sh"
    echo ""
}

check_prerequisites() {
    log_title "Checking Prerequisites"
    
    # Check for curl
    if ! command -v curl &> /dev/null; then
        log_error "curl not found. Please install curl."
        exit 1
    fi
    log_success "curl found"
    
    # Check for jq (optional but recommended)
    if ! command -v jq &> /dev/null; then
        log_warn "jq not found. Recommended for better JSON parsing."
    else
        log_success "jq found"
    fi
    
    # Check for bc
    if ! command -v bc &> /dev/null; then
        log_warn "bc not found. Some calculations may not work properly."
    else
        log_success "bc found"
    fi
}

initialize_directories() {
    log_title "Initializing Demo Directories"
    
    mkdir -p "$DEMO_LOG_DIR"
    mkdir -p "$DEMO_STATE_DIR"
    
    log_success "Demo directories created:"
    echo "  Database: $DEMO_DB_DIR"
    echo "  Logs:     $DEMO_LOG_DIR"
    echo "  State:    $DEMO_STATE_DIR"
}

check_server_connectivity() {
    log_title "Checking Server Connectivity"
    
    # Check Basis server
    log_info "Checking Basis server at $SERVER_URL"
    if curl -s "$SERVER_URL/status" > /dev/null 2>&1; then
        log_success "Basis server is running"
    else
        log_error "Cannot connect to Basis server at $SERVER_URL"
        log_info "Make sure to start the server with: cargo run -p basis_server"
        read -p "Continue anyway? (y/n) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
}

create_accounts() {
    log_title "Creating Demo Accounts"
    
    # Generate random keys for Alice and Bob
    # In a real implementation, these would be generated with proper cryptography
    
    # Alice's keys
    ALICE_PRIVKEY=$(printf "%064x" $(od -An -tx1 -N 32 /dev/urandom | tr -d ' '))
    ALICE_PUBKEY=$(printf "%066x" $RANDOM)  # Placeholder - actual key generation needed
    
    # Bob's keys
    BOB_PRIVKEY=$(printf "%064x" $(od -An -tx1 -N 32 /dev/urandom | tr -d ' '))
    BOB_PUBKEY=$(printf "%066x" $RANDOM)  # Placeholder - actual key generation needed
    
    log_success "Generated cryptographic keys:"
    
    # Save account information
    save_account_info "alice" "$ALICE_PRIVKEY" "$ALICE_PUBKEY" "$ALICE_RESERVE_AMOUNT"
    save_account_info "bob" "$BOB_PRIVKEY" "$BOB_PUBKEY" "0"
    
    log_success "Accounts created and saved to $DEMO_STATE_DIR"
}

save_account_info() {
    local name=$1
    local privkey=$2
    local pubkey=$3
    local reserve=$4
    
    local account_file="$DEMO_STATE_DIR/${name}_account.txt"
    
    cat > "$account_file" << EOF
# SilverCents Demo Account - $name
# Generated: $(date)

ACCOUNT_NAME=$name
PRIVATE_KEY=$privkey
PUBLIC_KEY=$pubkey
RESERVE_BALANCE=$reserve
CREATED_AT=$(date +%s)
EOF
    
    log_success "Saved $name account: $account_file"
    
    if [ "$name" == "alice" ]; then
        ALICE_PRIVKEY=$privkey
        ALICE_PUBKEY=$pubkey
    else
        BOB_PRIVKEY=$privkey
        BOB_PUBKEY=$pubkey
    fi
}

display_setup_summary() {
    log_title "Setup Summary"
    
    echo ""
    echo "SilverCents Demo Configuration:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "📍 Server Information:"
    echo "  Basis Server:     $SERVER_URL"
    echo "  Ergo Node:        $ERGO_NODE_URL"
    echo ""
    echo "👤 Alice (Silver Merchant):"
    echo "  Account:          $ALICE_NAME"
    echo "  Role:             Issuer of SilverCents notes"
    echo "  Initial Reserve:  $ALICE_RESERVE_AMOUNT units"
    echo "  NFT ID:           $ALICE_NFT_ID"
    echo ""
    echo "👤 Bob (Customer):"
    echo "  Account:          $BOB_NAME"
    echo "  Role:             Receiver and redeemer of notes"
    echo "  Initial Balance:  0 units"
    echo ""
    echo "📂 Demo Storage:"
    echo "  Directory:        $DEMO_DB_DIR"
    echo "  State Files:      $DEMO_STATE_DIR"
    echo "  Logs:             $DEMO_LOG_DIR"
    echo ""
    echo "⚙️  Default Parameters:"
    echo "  Issue Interval:   30 seconds"
    echo "  Note Amount:      100-1000 units"
    echo "  Poll Interval:    10 seconds"
    echo "  Min Collateral:   100%"
    echo ""
}

# Run main setup
main
