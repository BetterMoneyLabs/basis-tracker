#!/bin/bash

###############################################################################
# SilverCents Demo - Customer Receiver Script (Bob)
#
# Bob is a customer who:
# 1. Receives SilverCents notes from Alice (merchant)
# 2. Monitors note collateralization
# 3. Tracks total accumulated debt/notes
# 4. Verifies note authenticity
#
# Bob polls for new notes and stops accepting if collateralization drops
# This demonstrates risk management and trust verification
###############################################################################

set -e

# Configuration
SERVER_URL="${SERVER_URL:-http://localhost:3048}"
DEMO_DB_DIR="/tmp/silvercents_demo"
DEMO_STATE_DIR="$DEMO_DB_DIR/state"

# Bob's configuration
ALICE_NAME="Alice_Merchant"
BOB_NAME="Bob_Customer"
POLL_INTERVAL=10                # Check for new notes every 10 seconds
MIN_COLLATERALIZATION=1.0       # Stop accepting if ratio drops below 100%

# Logging
LOG_FILE="$DEMO_DB_DIR/logs/bob_receiver.log"
NOTES_FILE="$DEMO_DB_DIR/logs/bob_notes.csv"
STATE_FILE="$DEMO_STATE_DIR/bob_state.txt"

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
# Account & Cryptography
###############################################################################

load_accounts() {
    # Load Bob's account
    if [ ! -f "$DEMO_STATE_DIR/${BOB_NAME,,}_account.txt" ]; then
        log_error "Bob account not found. Run silvercents_setup.sh first."
        exit 1
    fi
    source "$DEMO_STATE_DIR/${BOB_NAME,,}_account.txt"
    BOB_PUBKEY=$PUBLIC_KEY
    
    # Load Alice's account (for reference)
    if [ ! -f "$DEMO_STATE_DIR/${ALICE_NAME,,}_account.txt" ]; then
        log_error "Alice account not found. Run silvercents_setup.sh first."
        exit 1
    fi
    source "$DEMO_STATE_DIR/${ALICE_NAME,,}_account.txt"
    ALICE_PUBKEY=$PUBLIC_KEY
    ALICE_RESERVE=$RESERVE_TOTAL
    
    log_success "Loaded Bob account: $BOB_PUBKEY"
    log_success "Loaded Alice info: Reserve = $ALICE_RESERVE units"
}

###############################################################################
# Note Reception & Verification
###############################################################################

fetch_notes() {
    local issuer_pubkey=$1
    
    log_info "Polling for new notes from Alice..."
    
    # Fetch notes from the tracker
    local response=$(curl -s -X GET "$SERVER_URL/notes/issuer/$issuer_pubkey" \
        -w "\n%{http_code}")
    
    local http_code=$(echo "$response" | tail -n1)
    local response_body=$(echo "$response" | head -n-1)
    
    if [ "$http_code" -eq 200 ]; then
        echo "$response_body"
        return 0
    else
        log_warn "Failed to fetch notes (HTTP $http_code)"
        return 1
    fi
}

parse_notes_jq() {
    local json=$1
    
    if command -v jq >/dev/null 2>&1; then
        # Use jq for proper JSON parsing
        echo "$json" | jq -r '.data[] | @csv' 2>/dev/null
    else
        # Fallback: basic parsing
        echo "$json" | grep -o '"amount":[0-9]*' | cut -d: -f2
    fi
}

process_notes() {
    local json=$1
    local new_notes_received=0
    local total_new_amount=0
    
    # Extract notes using jq if available
    if command -v jq >/dev/null 2>&1; then
        local note_count=$(echo "$json" | jq '.data | length' 2>/dev/null || echo "0")
        
        if [ "$note_count" -gt 0 ]; then
            log_info "Found $note_count notes from Alice"
            
            for i in $(seq 0 $((note_count - 1))); do
                local amount=$(echo "$json" | jq -r ".data[$i].amount" 2>/dev/null)
                local timestamp=$(echo "$json" | jq -r ".data[$i].timestamp" 2>/dev/null)
                local signature=$(echo "$json" | jq -r ".data[$i].signature" 2>/dev/null)
                
                # Check if this is a new note
                if ! grep -q "^$timestamp,$amount" "$NOTES_FILE" 2>/dev/null; then
                    accept_note "$amount" "$timestamp" "$signature"
                    new_notes_received=$((new_notes_received + 1))
                    total_new_amount=$((total_new_amount + amount))
                fi
            done
        fi
    else
        log_warn "jq not available - using basic parsing"
    fi
    
    if [ $new_notes_received -gt 0 ]; then
        log_success "Received $new_notes_received new notes (total: $total_new_amount units)"
        return 0
    else
        log_info "No new notes received"
        return 1
    fi
}

accept_note() {
    local amount=$1
    local timestamp=$2
    local signature=$3
    
    log_success "✓ Accepting note: $amount units"
    
    # Update state
    TOTAL_RECEIVED=$((TOTAL_RECEIVED + amount))
    NOTES_COUNT=$((NOTES_COUNT + 1))
    
    # Calculate collateralization
    if [ $TOTAL_RECEIVED -gt 0 ]; then
        COLLATERALIZATION=$(echo "scale=4; $ALICE_RESERVE / $TOTAL_RECEIVED" | bc)
        COLLATERALIZATION_PCT=$(echo "scale=2; $COLLATERALIZATION * 100" | bc)
    else
        COLLATERALIZATION=1.0
        COLLATERALIZATION_PCT=100.00
    fi
    
    # Log note
    echo "$timestamp,$ALICE_PUBKEY,$BOB_PUBKEY,$amount,$signature,RECEIVED" >> "$NOTES_FILE"
    
    # Save state
    save_state
    
    # Check if collateralization is still acceptable
    if (( $(echo "$COLLATERALIZATION < $MIN_COLLATERALIZATION" | bc -l) )); then
        log_error "⚠ Collateralization below minimum!"
        log_error "   Current: ${COLLATERALIZATION_PCT}% | Minimum: $(echo "$MIN_COLLATERALIZATION * 100" | bc)%"
        ACCEPTING_NOTES=false
    fi
}

###############################################################################
# State Management
###############################################################################

initialize_state() {
    cat > "$STATE_FILE" << EOF
# Bob's SilverCents Note Collection
# Created: $(date)

TOTAL_RECEIVED=0
NOTES_COUNT=0
COLLATERALIZATION=1.0
ACCEPTING_NOTES=true
LAST_UPDATE=$(date +%s)
EOF
    
    # Initialize notes ledger
    if [ ! -f "$NOTES_FILE" ]; then
        echo "TIMESTAMP,ISSUER,RECIPIENT,AMOUNT,SIGNATURE,STATUS" > "$NOTES_FILE"
    fi
}

load_state() {
    if [ -f "$STATE_FILE" ]; then
        source "$STATE_FILE"
    fi
}

save_state() {
    cat > "$STATE_FILE" << EOF
# Bob's SilverCents Note Collection
# Updated: $(date)

TOTAL_RECEIVED=$TOTAL_RECEIVED
NOTES_COUNT=$NOTES_COUNT
COLLATERALIZATION=$COLLATERALIZATION
ACCEPTING_NOTES=$ACCEPTING_NOTES
LAST_UPDATE=$(date +%s)
EOF
}

###############################################################################
# Status & Display
###############################################################################

display_header() {
    clear
    echo -e "${MAGENTA}"
    echo "╔════════════════════════════════════════════════════════════════════╗"
    echo "║                 💳 SILVERCENTS CUSTOMER WALLET 💳                  ║"
    echo "║                       Bob - Customer                              ║"
    echo "╚════════════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

display_status() {
    local status="monitoring"
    
    if [ "$ACCEPTING_NOTES" = "false" ]; then
        status="STOPPED"
    fi
    
    echo ""
    echo -e "${CYAN}┌─ NOTES RECEIVED ─────────────────────────────────────────────────┐${NC}"
    echo "  Total Notes:          $NOTES_COUNT"
    echo "  Total Amount:         $TOTAL_RECEIVED units (SilverCents)"
    echo "  Average Note Size:    $(echo "scale=0; $TOTAL_RECEIVED / ($NOTES_COUNT + 1)" | bc) units"
    echo -e "${CYAN}├─ ISSUER STATUS (Alice) ──────────────────────────────────────────┤${NC}"
    echo "  Reserve:              $ALICE_RESERVE units"
    echo "  Collateralization:    "
    
    if (( $(echo "$COLLATERALIZATION >= 1.0" | bc -l) )); then
        echo -e "    ${GREEN}${COLLATERALIZATION_PCT}%${NC} ✓ (Safe)"
    elif (( $(echo "$COLLATERALIZATION >= 0.8" | bc -l) )); then
        echo -e "    ${YELLOW}${COLLATERALIZATION_PCT}%${NC} ⚠ (Warning)"
    else
        echo -e "    ${RED}${COLLATERALIZATION_PCT}%${NC} ✗ (Risky)"
    fi
    
    echo -e "${CYAN}├─ BOB'S POSITION ──────────────────────────────────────────────────┤${NC}"
    if [ "$ACCEPTING_NOTES" = "true" ]; then
        echo -e "  Status:               ${GREEN}🟢 Accepting notes${NC}"
    else
        echo -e "  Status:               ${RED}🔴 NOT accepting notes${NC}"
        echo "  Reason:               Collateralization below minimum"
    fi
    echo "  Poll Interval:        $POLL_INTERVAL seconds"
    echo -e "${CYAN}└──────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
}

###############################################################################
# Main Reception Loop
###############################################################################

main() {
    log_title "SilverCents Customer - Bob Reception Demo"
    
    log_info "Server URL: $SERVER_URL"
    log_info "Log file: $LOG_FILE"
    log_info "Poll interval: $POLL_INTERVAL seconds"
    log_info "Min collateralization: $(echo "$MIN_COLLATERALIZATION * 100" | bc)%"
    
    # Create directories
    mkdir -p "$DEMO_LOG_DIR"
    mkdir -p "$DEMO_STATE_DIR"
    
    # Load accounts
    load_accounts
    
    # Initialize state
    initialize_state
    load_state
    
    log_success "Starting note reception loop..."
    
    local poll_count=0
    
    while [ "$ACCEPTING_NOTES" = "true" ]; do
        display_header
        display_status
        
        poll_count=$((poll_count + 1))
        
        log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        log_info "Poll #$poll_count - Checking for new notes"
        log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        
        # Fetch and process notes
        if response=$(fetch_notes "$ALICE_PUBKEY"); then
            if process_notes "$response"; then
                display_status
            fi
        fi
        
        # Check if we should continue
        if [ "$ACCEPTING_NOTES" = "true" ]; then
            log_info "Waiting $POLL_INTERVAL seconds for next poll..."
            echo ""
            sleep $POLL_INTERVAL
        fi
    done
    
    # Final status
    echo ""
    log_title "Reception Complete - Collateralization Too Low"
    log_error "Bob is no longer accepting notes due to insufficient collateralization"
    log_info "Final Status:"
    log_info "  Notes Received: $NOTES_COUNT"
    log_info "  Total Amount: $TOTAL_RECEIVED units"
    log_info "  Alice's Collateralization: ${COLLATERALIZATION_PCT}%"
    log_info "  Minimum Required: $(echo "$MIN_COLLATERALIZATION * 100" | bc)%"
    log_info ""
    log_info "Bob should now redeem notes to recover his value."
    log_info "Run: ./silvercents_redeem.sh"
    
    echo ""
    echo "Notes ledger saved to: $NOTES_FILE"
    echo "Full logs saved to: $LOG_FILE"
    echo ""
}

# Run main function
main
