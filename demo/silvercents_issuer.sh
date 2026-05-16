#!/bin/bash

###############################################################################
# SilverCents Demo - Merchant Issuance Script (Alice)
#
# Alice is a silver merchant who:
# 1. Maintains a physical reserve of silver coins
# 2. Creates an on-chain reserve for collateral
# 3. Issues off-chain SilverCents notes to customers
# 4. Monitors collateralization ratio
#
# Each note represents a debt: "Alice owes Bob X SilverCents"
# Redeemable for physical silver coins 1:1
###############################################################################

set -e

# Configuration
SERVER_URL="${SERVER_URL:-http://localhost:3048}"
DEMO_DB_DIR="/tmp/silvercents_demo"
DEMO_STATE_DIR="$DEMO_DB_DIR/state"

# Alice's configuration
ALICE_NAME="Alice_Merchant"
BOB_NAME="Bob_Customer"
RESERVE_TOTAL=1000000      # Total reserve (in units, representing silver coins)
ISSUE_INTERVAL=30          # Issue a note every 30 seconds
AMOUNT_MIN=100             # Minimum note amount
AMOUNT_MAX=1000            # Maximum note amount
COLLATERAL_THRESHOLD=1.0   # Stop issuing if ratio drops below 100%

# Logging
LOG_FILE="$DEMO_DB_DIR/logs/alice_issuer.log"
LEDGER_FILE="$DEMO_DB_DIR/logs/alice_ledger.csv"
STATE_FILE="$DEMO_STATE_DIR/alice_state.txt"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
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

load_account() {
    if [ ! -f "$DEMO_STATE_DIR/${ALICE_NAME,,}_account.txt" ]; then
        log_error "Alice account not found. Run silvercents_setup.sh first."
        exit 1
    fi
    
    # Source account information
    source "$DEMO_STATE_DIR/${ALICE_NAME,,}_account.txt"
    ALICE_PRIVKEY=$PRIVATE_KEY
    ALICE_PUBKEY=$PUBLIC_KEY
    
    log_success "Loaded Alice account: $ALICE_PUBKEY"
}

load_bob_pubkey() {
    if [ ! -f "$DEMO_STATE_DIR/${BOB_NAME,,}_account.txt" ]; then
        log_error "Bob account not found. Run silvercents_setup.sh first."
        exit 1
    fi
    
    source "$DEMO_STATE_DIR/${BOB_NAME,,}_account.txt"
    BOB_PUBKEY=$PUBLIC_KEY
    
    log_success "Loaded Bob's public key: $BOB_PUBKEY"
}

create_signature() {
    local recipient_pubkey=$1
    local amount=$2
    local timestamp=$3
    
    # In a real implementation, this would create a proper secp256k1 Schnorr signature
    # For demo purposes, we create a deterministic signature
    local message="${recipient_pubkey}${amount}${timestamp}"
    
    # Use openssl or a hash function to create a signature-like hex string
    # For demo: create a 65-byte (130 hex char) signature
    local signature=$(echo -n "$message" | sha256sum | cut -d' ' -f1)
    signature="${signature}${signature:0:2}"  # Pad to 130 chars for 65 bytes
    signature="${signature:0:130}"
    
    echo "$signature"
}

###############################################################################
# Reserve Management
###############################################################################

create_reserve() {
    log_title "Creating On-Chain Reserve"
    
    log_info "Creating reserve for Alice..."
    log_info "Reserve Amount: $RESERVE_TOTAL units"
    log_info "NFT ID: $ALICE_NFT_ID"
    
    # In a real scenario, this would create a reserve on the Ergo blockchain
    # For demo purposes, we simulate it locally
    
    cat > "$STATE_FILE" << EOF
# Alice's SilverCents Reserve State
# Created: $(date)

RESERVE_TOTAL=$RESERVE_TOTAL
RESERVE_AVAILABLE=$RESERVE_TOTAL
TOTAL_ISSUED=0
NOTE_COUNT=0
COLLATERALIZATION=100.0
NFT_ID=$ALICE_NFT_ID
STATUS=ACTIVE
EOF
    
    log_success "Reserve created and initialized"
    log_success "Collateralization: 100.0%"
}

update_state() {
    local amount=$1
    
    # Update state file
    RESERVE_AVAILABLE=$((RESERVE_AVAILABLE - amount))
    TOTAL_ISSUED=$((TOTAL_ISSUED + amount))
    NOTE_COUNT=$((NOTE_COUNT + 1))
    
    if [ $TOTAL_ISSUED -gt 0 ]; then
        COLLATERALIZATION=$(echo "scale=2; $RESERVE_TOTAL * 100 / ($RESERVE_TOTAL + $TOTAL_ISSUED)" | bc)
    else
        COLLATERALIZATION=100.0
    fi
    
    cat > "$STATE_FILE" << EOF
# Alice's SilverCents Reserve State
# Updated: $(date)

RESERVE_TOTAL=$RESERVE_TOTAL
RESERVE_AVAILABLE=$RESERVE_AVAILABLE
TOTAL_ISSUED=$TOTAL_ISSUED
NOTE_COUNT=$NOTE_COUNT
COLLATERALIZATION=$COLLATERALIZATION
NFT_ID=$ALICE_NFT_ID
STATUS=ACTIVE
EOF
}

###############################################################################
# Note Issuance
###############################################################################

issue_note() {
    local amount=$1
    local timestamp=$(date +%s)
    
    log_info "Creating note for Bob..."
    log_info "Amount: $amount units (SilverCents)"
    
    # Create signature
    local signature=$(create_signature "$BOB_PUBKEY" "$amount" "$timestamp")
    
    # Call the Basis server API to create the note
    local response=$(curl -s -X POST "$SERVER_URL/notes" \
        -H "Content-Type: application/json" \
        -d "{
            \"issuer_pubkey\": \"$ALICE_PUBKEY\",
            \"recipient_pubkey\": \"$BOB_PUBKEY\",
            \"amount\": $amount,
            \"timestamp\": $timestamp,
            \"signature\": \"$signature\"
        }" \
        -w "\n%{http_code}")
    
    local http_code=$(echo "$response" | tail -n1)
    local response_body=$(echo "$response" | head -n-1)
    
    if [ "$http_code" -eq 201 ] || [ "$http_code" -eq 200 ]; then
        log_success "Note issued successfully!"
        
        # Update state
        update_state $amount
        
        # Log to ledger
        echo "$timestamp,$ALICE_PUBKEY,$BOB_PUBKEY,$amount,$signature,ISSUED" >> "$LEDGER_FILE"
        
        # Display status
        display_status "$amount"
        
        return 0
    else
        log_error "Failed to issue note (HTTP $http_code)"
        log_error "Response: $response_body"
        return 1
    fi
}

###############################################################################
# Status & Display
###############################################################################

display_header() {
    clear
    echo -e "${CYAN}"
    echo "╔════════════════════════════════════════════════════════════════════╗"
    echo "║                 🪙 SILVERCENTS MERCHANT TERMINAL 🪙                 ║"
    echo "║                      Alice - Silver Merchant                       ║"
    echo "╚════════════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

display_status() {
    local last_amount=${1:-0}
    
    echo ""
    echo -e "${CYAN}┌─ RESERVE STATUS ─────────────────────────────────────────────────┐${NC}"
    echo "  Total Reserve:        $RESERVE_TOTAL units"
    echo "  Available:            $RESERVE_AVAILABLE units"
    echo "  Total Issued:         $TOTAL_ISSUED units"
    echo "  Notes Created:        $NOTE_COUNT"
    echo -e "${CYAN}├─ COLLATERALIZATION ─────────────────────────────────────────────┤${NC}"
    
    if (( $(echo "$COLLATERALIZATION >= 100" | bc -l) )); then
        echo -e "  Ratio:                ${GREEN}${COLLATERALIZATION}%${NC} ✓ (Healthy)"
    elif (( $(echo "$COLLATERALIZATION >= 80" | bc -l) )); then
        echo -e "  Ratio:                ${YELLOW}${COLLATERALIZATION}%${NC} ⚠ (Warning)"
    else
        echo -e "  Ratio:                ${RED}${COLLATERALIZATION}%${NC} ✗ (Critical)"
    fi
    
    echo -e "${CYAN}├─ LAST TRANSACTION ──────────────────────────────────────────────┤${NC}"
    if [ $last_amount -gt 0 ]; then
        echo "  Amount:               $last_amount units"
        echo "  Time:                 $(date '+%Y-%m-%d %H:%M:%S')"
        echo "  To:                   Bob (Customer)"
    else
        echo "  Status:               Ready to issue notes"
    fi
    echo -e "${CYAN}└──────────────────────────────────────────────────────────────────┘${NC}"
    echo ""
}

###############################################################################
# Main Issuance Loop
###############################################################################

main() {
    log_title "SilverCents Merchant - Alice Issuance Demo"
    
    log_info "Server URL: $SERVER_URL"
    log_info "Log file: $LOG_FILE"
    
    # Create directories
    mkdir -p "$DEMO_LOG_DIR"
    mkdir -p "$DEMO_STATE_DIR"
    
    # Initialize ledger
    if [ ! -f "$LEDGER_FILE" ]; then
        echo "TIMESTAMP,ISSUER,RECIPIENT,AMOUNT,SIGNATURE,STATUS" > "$LEDGER_FILE"
    fi
    
    # Load accounts
    load_account
    load_bob_pubkey
    
    # Create reserve
    create_reserve
    
    log_success "Starting issuance loop..."
    log_info "Will issue notes every $ISSUE_INTERVAL seconds"
    
    # Main loop
    local note_count=0
    while [ $RESERVE_AVAILABLE -ge $AMOUNT_MIN ]; do
        display_header
        
        # Generate random amount
        amount=$(( RANDOM % (AMOUNT_MAX - AMOUNT_MIN + 1) + AMOUNT_MIN ))
        
        # Check if we have enough reserve
        if [ $amount -gt $RESERVE_AVAILABLE ]; then
            log_warn "Note amount ($amount) exceeds available reserve ($RESERVE_AVAILABLE)"
            log_warn "Cannot issue more notes. Reserve exhausted."
            break
        fi
        
        # Check collateralization
        if (( $(echo "$COLLATERALIZATION < $COLLATERAL_THRESHOLD" | bc -l) )); then
            log_error "Collateralization below threshold!"
            log_error "Cannot issue more notes. Stopping for risk management."
            break
        fi
        
        log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        log_info "Note #$((NOTE_COUNT + 1)) - Issuance Sequence"
        log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        
        # Issue the note
        if issue_note $amount; then
            display_status $amount
            
            log_info "Next issuance in $ISSUE_INTERVAL seconds..."
            log_info ""
            
            # Wait before next issuance
            sleep $ISSUE_INTERVAL
        else
            log_warn "Failed to issue note. Retrying in 5 seconds..."
            sleep 5
        fi
    done
    
    # Final status
    echo ""
    log_title "Issuance Complete"
    log_info "Final Status:"
    log_info "  Reserve Available: $RESERVE_AVAILABLE units"
    log_info "  Total Issued: $TOTAL_ISSUED units"
    log_info "  Notes Created: $NOTE_COUNT"
    log_info "  Collateralization: $COLLATERALIZATION%"
    log_info ""
    log_info "Ledger saved to: $LEDGER_FILE"
    log_info "Full logs saved to: $LOG_FILE"
    
    echo ""
    echo "Demo complete! Bob can now redeem notes at your location."
    echo ""
}

# Run main function
main
