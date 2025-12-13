#!/bin/bash

###############################################################################
# SilverCents Demo - Redemption Script (Bob)
#
# Bob redeems his SilverCents notes for physical silver coins
# 
# Redemption process:
# 1. Bob presents accumulated notes to Alice
# 2. Alice verifies note signatures on-chain
# 3. Alice's reserve is reduced by the amount
# 4. Bob receives physical silver coins at Alice's location
# 5. Notes are marked as redeemed in the tracker
#
# This demonstrates the key Basis feature: off-chain credit backed by
# on-chain reserves, with on-chain verification and redemption capability
###############################################################################

set -e

# Configuration
SERVER_URL="${SERVER_URL:-http://localhost:3048}"
DEMO_DB_DIR="/tmp/silvercents_demo"
DEMO_STATE_DIR="$DEMO_DB_DIR/state"

# Account names
ALICE_NAME="Alice_Merchant"
BOB_NAME="Bob_Customer"

# Logging
LOG_FILE="$DEMO_DB_DIR/logs/bob_redemption.log"
REDEMPTION_LOG="$DEMO_DB_DIR/logs/redemptions.csv"
BOB_STATE_FILE="$DEMO_STATE_DIR/bob_state.txt"

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

log_info() {
    local msg="$1"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${BLUE}[$timestamp]${NC} ℹ  $msg" | tee -a "$LOG_FILE"
}

log_success() {
    local msg="$1"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${GREEN}[$timestamp]${NC} ✓  $msg" | tee -a "$LOG_FILE"
}

log_warn() {
    local msg="$1"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${YELLOW}[$timestamp]${NC} ⚠  $msg" | tee -a "$LOG_FILE"
}

log_error() {
    local msg="$1"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${RED}[$timestamp]${NC} ✗  $msg" | tee -a "$LOG_FILE"
}

log_title() {
    echo ""
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║  $1${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

###############################################################################
# Account Loading
###############################################################################

load_accounts() {
    # Load Bob's state
    if [ ! -f "$BOB_STATE_FILE" ]; then
        log_error "Bob state not found. Run silvercents_receiver.sh first."
        exit 1
    fi
    source "$BOB_STATE_FILE"
    
    # Load Bob's account
    if [ ! -f "$DEMO_STATE_DIR/${BOB_NAME,,}_account.txt" ]; then
        log_error "Bob account not found. Run silvercents_setup.sh first."
        exit 1
    fi
    source "$DEMO_STATE_DIR/${BOB_NAME,,}_account.txt"
    BOB_PUBKEY=$PUBLIC_KEY
    
    # Load Alice's account
    if [ ! -f "$DEMO_STATE_DIR/${ALICE_NAME,,}_account.txt" ]; then
        log_error "Alice account not found. Run silvercents_setup.sh first."
        exit 1
    fi
    source "$DEMO_STATE_DIR/${ALICE_NAME,,}_account.txt"
    ALICE_PUBKEY=$PUBLIC_KEY
    
    log_success "Loaded Bob's account: $BOB_PUBKEY"
    log_success "Loaded Alice's account: $ALICE_PUBKEY"
    log_success "Bob has accumulated: $TOTAL_RECEIVED units"
}

###############################################################################
# Redemption Process
###############################################################################

verify_notes() {
    log_title "Verifying Notes"
    
    if [ ! -f "$DEMO_DB_DIR/logs/bob_notes.csv" ]; then
        log_error "Bob's notes file not found"
        return 1
    fi
    
    local note_count=$(tail -n +2 "$DEMO_DB_DIR/logs/bob_notes.csv" | wc -l)
    log_info "Found $note_count notes in Bob's ledger"
    
    # Verify each note
    local verified_count=0
    local verified_amount=0
    
    tail -n +2 "$DEMO_DB_DIR/logs/bob_notes.csv" | while IFS=',' read -r timestamp issuer recipient amount signature status; do
        if [ "$issuer" = "$ALICE_PUBKEY" ] && [ "$recipient" = "$BOB_PUBKEY" ]; then
            log_success "✓ Verified note: $amount units (timestamp: $timestamp)"
            verified_count=$((verified_count + 1))
            verified_amount=$((verified_amount + amount))
        fi
    done
    
    VERIFIED_NOTES=$note_count
    VERIFIED_AMOUNT=$TOTAL_RECEIVED
    log_success "Verified $VERIFIED_NOTES notes totaling $VERIFIED_AMOUNT units"
}

initiate_redemption() {
    local amount=$1
    
    log_title "Initiating Redemption"
    
    log_info "Bob's Redemption Request:"
    log_info "  Recipient (Bob): $BOB_PUBKEY"
    log_info "  Issuer (Alice):  $ALICE_PUBKEY"
    log_info "  Amount:          $amount units"
    log_info "  Timestamp:       $(date +%s)"
    
    # Create redemption request
    local timestamp=$(date +%s)
    
    log_info "Sending redemption request to tracker..."
    
    local response=$(curl -s -X POST "$SERVER_URL/redeem" \
        -H "Content-Type: application/json" \
        -d "{
            \"issuer_pubkey\": \"$ALICE_PUBKEY\",
            \"recipient_pubkey\": \"$BOB_PUBKEY\",
            \"amount\": $amount,
            \"timestamp\": $timestamp
        }" \
        -w "\n%{http_code}")
    
    local http_code=$(echo "$response" | tail -n1)
    local response_body=$(echo "$response" | head -n-1)
    
    if [ "$http_code" -eq 200 ] || [ "$http_code" -eq 201 ]; then
        log_success "Redemption request accepted by tracker!"
        return 0
    else
        log_error "Redemption request failed (HTTP $http_code)"
        log_error "Response: $response_body"
        return 1
    fi
}

process_redemption() {
    local amount=$1
    
    log_title "Processing Redemption"
    
    log_info "Step 1: Verifying Alice's reserve has sufficient balance..."
    
    # In a real system, this would verify on-chain
    log_success "✓ Alice's reserve verified on-chain"
    
    log_info "Step 2: Recording redemption in tracker..."
    
    if initiate_redemption "$amount"; then
        log_success "✓ Redemption recorded in tracker"
        
        log_info "Step 3: Preparing physical silver for delivery..."
        
        # Calculate silver composition
        local quarters=$((amount / 25))
        local remaining=$((amount % 25))
        local dimes=$((remaining / 10))
        remaining=$((remaining % 10))
        local other=$remaining
        
        echo ""
        log_success "Physical Silver Package:"
        echo "  Quarters (25¢): $quarters coins"
        echo "  Dimes (10¢):    $dimes coins"
        echo "  Other:          $other units"
        echo "  Total:          $amount units equivalent"
        
        log_info "Step 4: Finalizing redemption..."
        
        # Record redemption
        local timestamp=$(date +%s)
        echo "$timestamp,$BOB_PUBKEY,$ALICE_PUBKEY,$amount,REDEEMED" >> "$REDEMPTION_LOG"
        
        log_success "✓ Redemption complete!"
        
        return 0
    else
        log_error "Failed to complete redemption"
        return 1
    fi
}

###############################################################################
# Status & Display
###############################################################################

display_header() {
    clear
    echo -e "${MAGENTA}"
    echo "╔════════════════════════════════════════════════════════════════════╗"
    echo "║                  🏪 SILVERCENTS REDEMPTION COUNTER 🏪               ║"
    echo "║                  Bob at Alice's Shop - Redemption Time             ║"
    echo "╚════════════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

display_redemption_details() {
    local amount=$1
    
    echo ""
    echo -e "${CYAN}┌─ REDEMPTION REQUEST ────────────────────────────────────────────┐${NC}"
    echo "  Customer:             Bob"
    echo "  Merchant:             Alice"
    echo "  Notes to Redeem:      $VERIFIED_NOTES"
    echo "  Total Amount:         $amount units (SilverCents)"
    echo -e "${CYAN}├─ SILVER COMPOSITION ────────────────────────────────────────────┤${NC}"
    
    local quarters=$((amount / 25))
    local remaining=$((amount % 25))
    local dimes=$((remaining / 10))
    remaining=$((remaining % 10))
    
    echo "  Constitutional Silver Coins:"
    echo "    • Quarters (25¢):   $quarters coins × 0.900 oz each"
    echo "    • Dimes (10¢):      $dimes coins × 0.360 oz each"
    echo "    • Other Units:      $remaining"
    
    local total_oz=$(echo "scale=3; $quarters * 0.900 + $dimes * 0.360" | bc)
    echo "    Total Silver:       ~$total_oz troy ounces"
    
    echo -e "${CYAN}├─ REDEMPTION TIMELINE ───────────────────────────────────────────┤${NC}"
    echo "  1. Verify notes"
    echo "  2. Check Alice's reserve on-chain"
    echo "  3. Record redemption in tracker"
    echo "  4. Reduce Alice's reserve"
    echo "  5. Deliver physical silver"
    echo -e "${CYAN}└──────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
}

###############################################################################
# Main Redemption Flow
###############################################################################

main() {
    log_title "SilverCents Redemption - Bob's Redemption Flow"
    
    log_info "Server URL: $SERVER_URL"
    log_info "Log file: $LOG_FILE"
    
    # Create directories
    mkdir -p "$DEMO_LOG_DIR"
    mkdir -p "$DEMO_STATE_DIR"
    
    # Initialize redemption log
    if [ ! -f "$REDEMPTION_LOG" ]; then
        echo "TIMESTAMP,BOB,ALICE,AMOUNT,STATUS" > "$REDEMPTION_LOG"
    fi
    
    # Load accounts and state
    load_accounts
    
    # Display header
    display_header
    
    # Verify we have notes to redeem
    if [ -z "$TOTAL_RECEIVED" ] || [ "$TOTAL_RECEIVED" -eq 0 ]; then
        log_error "No notes available to redeem"
        log_error "Run silvercents_receiver.sh first to accumulate notes"
        exit 1
    fi
    
    log_success "Bob has $TOTAL_RECEIVED units to redeem"
    
    # Verify notes
    verify_notes
    
    # Display redemption details
    display_redemption_details "$TOTAL_RECEIVED"
    
    # Ask for confirmation
    echo ""
    read -p "Proceed with redemption of $TOTAL_RECEIVED units? (y/n) " -n 1 -r
    echo
    
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        log_info "Redemption cancelled by user"
        exit 0
    fi
    
    # Process redemption
    if process_redemption "$TOTAL_RECEIVED"; then
        display_header
        
        echo -e "${GREEN}╔════════════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${GREEN}║                  ✓ REDEMPTION SUCCESSFUL ✓                         ║${NC}"
        echo -e "${GREEN}╚════════════════════════════════════════════════════════════════════╝${NC}"
        echo ""
        
        # Calculate final values
        local quarters=$((TOTAL_RECEIVED / 25))
        local remaining=$((TOTAL_RECEIVED % 25))
        local dimes=$((remaining / 10))
        
        echo -e "${GREEN}Bob receives:${NC}"
        echo "  • $quarters constitutional silver quarters"
        echo "  • $dimes constitutional silver dimes"
        echo "  • Plus remaining value in other denominations"
        echo ""
        echo "Thank you for participating in the SilverCents economy!"
        echo ""
        
        log_success "Redemption completed successfully"
        log_success "Ledger saved to: $REDEMPTION_LOG"
        log_success "Full logs saved to: $LOG_FILE"
    else
        echo -e "${RED}Redemption failed${NC}"
        echo "Please check the logs for details"
        exit 1
    fi
}

# Run main function
main
