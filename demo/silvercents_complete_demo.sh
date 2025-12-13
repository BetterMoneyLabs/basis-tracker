#!/bin/bash

###############################################################################
# SilverCents Complete Demo
#
# This script runs the complete SilverCents demo workflow:
# 1. Setup accounts
# 2. Alice issues silver-backed notes
# 3. Bob receives and monitors notes
# 4. Demonstrates collateralization tracking
# 5. Bob redeems notes for physical silver
#
# Perfect for demonstrations and testing the Basis protocol implementation
###############################################################################

set -e

# Configuration
DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_DIR="$(dirname "$DEMO_DIR")"
SERVER_URL="${SERVER_URL:-http://localhost:3048}"
DEMO_DB_DIR="/tmp/silvercents_demo"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

###############################################################################
# Helper Functions
###############################################################################

log_title() {
    echo ""
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║  $1${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

log_section() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

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

prompt_continue() {
    echo ""
    read -p "Press ENTER to continue..."
    echo ""
}

check_server() {
    log_info "Checking if Basis server is running at $SERVER_URL"
    
    if curl -s "$SERVER_URL/status" > /dev/null 2>&1; then
        log_success "Basis server is running"
        return 0
    else
        log_error "Cannot connect to Basis server at $SERVER_URL"
        log_info "Start the server with: cargo run -p basis_server"
        return 1
    fi
}

###############################################################################
# Demo Sequence
###############################################################################

main() {
    clear
    
    log_title "🪙 SILVERCENTS COMPLETE DEMO 🪙"
    
    echo "This demo demonstrates:"
    echo "  • Silver-backed cryptocurrency using the Basis protocol"
    echo "  • Merchant (Alice) issuing SilverCents notes to customers"
    echo "  • Customer (Bob) receiving notes and tracking collateralization"
    echo "  • On-chain reserve management and collateralization monitoring"
    echo "  • Redemption of notes for physical silver coins"
    echo ""
    
    prompt_continue
    
    # Check server
    if ! check_server; then
        log_error "Demo cannot proceed without the Basis server"
        exit 1
    fi
    
    # Phase 1: Setup
    log_section "PHASE 1: SETUP - Initializing Demo Environment"
    
    log_info "Creating demo directory structure..."
    mkdir -p "$DEMO_DB_DIR/logs"
    mkdir -p "$DEMO_DB_DIR/state"
    log_success "Demo directories created"
    
    log_info "Running setup script..."
    if [ -f "$DEMO_DIR/silvercents_setup.sh" ]; then
        bash "$DEMO_DIR/silvercents_setup.sh"
    else
        log_error "Setup script not found: $DEMO_DIR/silvercents_setup.sh"
        exit 1
    fi
    
    prompt_continue
    
    # Phase 2: Explain the scenario
    log_section "PHASE 2: THE SCENARIO"
    
    echo -e "${CYAN}Setting:${NC} A local community with a silver merchant"
    echo ""
    echo -e "${CYAN}Alice (Merchant):${NC}"
    echo "  • Operates a precious metals shop"
    echo "  • Stores a reserve of 1,000,000 units of constitutional silver"
    echo "  • Issues SilverCents notes to customers as payment"
    echo "  • Each note represents real silver held in her vault"
    echo ""
    echo -e "${CYAN}Bob (Customer):${NC}"
    echo "  • Purchases from Alice using SilverCents notes"
    echo "  • Accumulates notes as he makes purchases"
    echo "  • Can verify notes are backed by Alice's reserve"
    echo "  • Later redeems notes for physical silver coins"
    echo ""
    echo -e "${CYAN}The Protocol:${NC}"
    echo "  • Off-chain notes track debt relationships"
    echo "  • Tracker (Basis server) maintains ledger"
    echo "  • On-chain reserve proves collateral backing"
    echo "  • Signature verification prevents forgery"
    echo "  • Collateralization ratio ensures stability"
    echo ""
    
    prompt_continue
    
    # Phase 3: Alice issues notes
    log_section "PHASE 3: MERCHANT ISSUES NOTES"
    
    echo "Alice will now issue SilverCents notes to Bob"
    echo ""
    echo "Each note:"
    echo "  • Is signed by Alice with her private key"
    echo "  • Represents a specific amount of silver"
    echo "  • Is recorded in the Basis tracker"
    echo "  • Reduces available reserve balance"
    echo "  • Increases tracked debt to Bob"
    echo ""
    
    if [ -f "$DEMO_DIR/silvercents_issuer.sh" ]; then
        log_info "Starting Alice's issuance process (will run for 2 minutes)..."
        echo ""
        
        # Run issuer with timeout
        timeout 120s bash "$DEMO_DIR/silvercents_issuer.sh" || true
        
        log_success "Alice's issuance phase complete"
    else
        log_error "Issuer script not found"
        exit 1
    fi
    
    prompt_continue
    
    # Phase 4: Bob receives notes
    log_section "PHASE 4: CUSTOMER RECEIVES NOTES"
    
    echo "Bob now monitors for new notes from Alice"
    echo ""
    echo "Bob's checks:"
    echo "  • Verifies note authenticity (signature check)"
    echo "  • Tracks accumulated notes"
    echo "  • Monitors Alice's reserve balance"
    echo "  • Calculates collateralization ratio"
    echo "  • Stops accepting if ratio drops below 100%"
    echo ""
    
    if [ -f "$DEMO_DIR/silvercents_receiver.sh" ]; then
        log_info "Starting Bob's reception process..."
        echo ""
        
        # Run receiver with timeout
        timeout 180s bash "$DEMO_DIR/silvercents_receiver.sh" || true
        
        log_success "Bob's reception phase complete"
    else
        log_error "Receiver script not found"
        exit 1
    fi
    
    prompt_continue
    
    # Phase 5: Bob redeems notes
    log_section "PHASE 5: REDEMPTION - CUSTOMER EXCHANGES FOR SILVER"
    
    echo "Bob now redeems his accumulated notes"
    echo ""
    echo "Redemption process:"
    echo "  1. Bob presents notes to Alice (off-chain, at shop)"
    echo "  2. Alice verifies notes using tracker"
    echo "  3. Redemption recorded on-chain"
    echo "  4. Alice's reserve reduced"
    echo "  5. Bob receives physical silver coins"
    echo ""
    
    if [ -f "$DEMO_DIR/silvercents_redeem.sh" ]; then
        log_info "Starting redemption process..."
        echo ""
        
        # Run redemption (non-interactive in batch mode)
        echo "y" | bash "$DEMO_DIR/silvercents_redeem.sh" || true
        
        log_success "Redemption phase complete"
    else
        log_error "Redemption script not found"
        exit 1
    fi
    
    prompt_continue
    
    # Phase 6: Summary
    log_section "SUMMARY - WHAT WE DEMONSTRATED"
    
    echo -e "${GREEN}✓ Complete SilverCents Workflow${NC}"
    echo ""
    echo "  1. Setup:       Created accounts and initialized system"
    echo "  2. Issuance:    Alice created silver-backed notes"
    echo "  3. Reception:   Bob received and verified notes"
    echo "  4. Tracking:    Monitored collateralization"
    echo "  5. Redemption:  Bob exchanged notes for physical silver"
    echo ""
    
    echo -e "${CYAN}Key Basis Protocol Features Demonstrated:${NC}"
    echo "  • Off-chain note creation and tracking"
    echo "  • On-chain reserve for collateral backing"
    echo "  • Cryptographic signature verification"
    echo "  • Tracker maintains truth source"
    echo "  • Automatic collateralization monitoring"
    echo "  • Redemption for physical assets"
    echo ""
    
    # Display logs
    echo -e "${CYAN}Detailed Logs Available:${NC}"
    if [ -d "$DEMO_DB_DIR/logs" ]; then
        ls -lh "$DEMO_DB_DIR/logs/" || true
    fi
    
    echo ""
    
    echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  Demo Complete! ${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "To review the demo in detail:"
    echo "  • Alice's log:  cat $DEMO_DB_DIR/logs/alice_issuer.log"
    echo "  • Bob's log:    cat $DEMO_DB_DIR/logs/bob_receiver.log"
    echo "  • Redemptions:  cat $DEMO_DB_DIR/logs/redemptions.csv"
    echo ""
    echo "For more information:"
    echo "  • See SILVERCENTS_DEMO.md for detailed documentation"
    echo "  • Check specs/ for protocol details"
    echo "  • Review Basis protocol at specs/spec.md"
    echo ""
}

main "$@"
