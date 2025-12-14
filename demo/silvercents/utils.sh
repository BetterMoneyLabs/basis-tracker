#!/bin/bash

# SilverCents Demo Utilities
# Shared functions for silver/SilverCents conversions and formatting

# ============================================
# CONFIGURATION
# ============================================

# SilverCents conversion rate
# Based on US constitutional silver dime melt value
# 1 dime = 0.0715 troy oz of silver = 1 SilverCent
SC_TO_OZ_RATE="0.0715"

# Display precision for silver weights
PRECISION=4

# Server configuration
SERVER_URL="${SERVER_URL:-http://localhost:3048}"

# ============================================
# COLOR CODES
# ============================================
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ============================================
# CONVERSION FUNCTIONS
# ============================================

# Convert SilverCents to troy ounces
# Usage: sc_to_oz <silvercents>
sc_to_oz() {
    local sc=$1
    echo "scale=$PRECISION; $sc * $SC_TO_OZ_RATE" | bc
}

# Convert troy ounces to SilverCents
# Usage: oz_to_sc <troy_ounces>
oz_to_sc() {
    local oz=$1
    echo "scale=0; $oz / $SC_TO_OZ_RATE" | bc
}

# Convert nanoERG to SilverCents (1 SC = 1 nanoERG for simplicity)
# In real usage, this would use an oracle price
nano_to_sc() {
    local nano=$1
    echo "$nano"
}

# ============================================
# DISPLAY FUNCTIONS
# ============================================

# Format a balance showing both SC and oz
# Usage: format_balance <silvercents>
format_balance() {
    local sc=$1
    local oz=$(sc_to_oz $sc)
    echo "${sc} SC (≈${oz} troy oz)"
}

# Print a header with decoration
print_header() {
    local title=$1
    echo ""
    echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${NC} ${GREEN}$title${NC}"
    echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

# Print a section header
print_section() {
    local title=$1
    echo ""
    echo -e "${YELLOW}━━━ $title ━━━${NC}"
}

# Print success message
print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

# Print error message
print_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Print warning message
print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Print info message
print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

# ============================================
# SERVER FUNCTIONS
# ============================================

# Check if Basis server is running
check_server() {
    # Use --connect-timeout for faster failure detection
    # Use -f to fail silently on HTTP errors
    if curl -s --connect-timeout 2 --max-time 5 -f "$SERVER_URL/" > /dev/null 2>&1; then
        return 0
    else
        return 1
    fi
}

# Wait for server to be ready
wait_for_server() {
    local max_attempts=${1:-30}
    local attempt=1
    
    print_info "Waiting for Basis Tracker server at $SERVER_URL..."
    
    while [ $attempt -le $max_attempts ]; do
        if check_server; then
            print_success "Server is ready!"
            return 0
        fi
        echo -n "."
        sleep 1
        attempt=$((attempt + 1))
    done
    
    print_error "Server not available after $max_attempts seconds"
    return 1
}

# ============================================
# API HELPER FUNCTIONS
# ============================================

# Create a note via HTTP API
# Usage: create_note <issuer_pubkey> <recipient_pubkey> <amount>
create_note() {
    local issuer_pubkey=$1
    local recipient_pubkey=$2
    local amount=$3
    local timestamp=$(date +%s)
    
    # Generate a unique signature for demo (in real usage, this would be cryptographic)
    local signature=$(printf "%0130s" "" | tr ' ' '1' | sed "s/^.\{10\}/${timestamp}/")
    
    local response=$(curl -s -X POST "$SERVER_URL/notes" \
        -H "Content-Type: application/json" \
        -d "{\"issuer_pubkey\":\"$issuer_pubkey\",\"recipient_pubkey\":\"$recipient_pubkey\",\"amount\":$amount,\"timestamp\":$timestamp,\"signature\":\"$signature\"}" \
        -w "\n%{http_code}")
    
    local http_code=$(echo "$response" | tail -n 1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" == "200" ] || [ "$http_code" == "201" ]; then
        return 0
    else
        echo "$body"
        return 1
    fi
}

# Get key status from API
# Usage: get_key_status <pubkey>
get_key_status() {
    local pubkey=$1
    curl -s "$SERVER_URL/key-status/$pubkey"
}

# Initiate redemption via API
# Usage: initiate_redemption <issuer_pubkey> <recipient_pubkey> <amount>
initiate_redemption() {
    local issuer_pubkey=$1
    local recipient_pubkey=$2
    local amount=$3
    local timestamp=$(date +%s)
    
    curl -s -X POST "$SERVER_URL/redeem" \
        -H "Content-Type: application/json" \
        -d "{\"issuer_pubkey\":\"$issuer_pubkey\",\"recipient_pubkey\":\"$recipient_pubkey\",\"amount\":$amount,\"timestamp\":$timestamp}"
}

# ============================================
# COLLATERALIZATION DISPLAY
# ============================================

# Display collateralization status with visual indicator
# Usage: display_collateralization <ratio>
display_collateralization() {
    local ratio=$1
    
    if (( $(echo "$ratio < 1.0" | bc -l) )); then
        echo -e "${RED}🔴 UNDER-COLLATERALIZED ($ratio)${NC}"
    elif (( $(echo "$ratio < 1.5" | bc -l) )); then
        echo -e "${YELLOW}🟡 LOW ($ratio)${NC}"
    elif (( $(echo "$ratio < 2.0" | bc -l) )); then
        echo -e "${YELLOW}🟢 ADEQUATE ($ratio)${NC}"
    else
        echo -e "${GREEN}🟢 EXCELLENT ($ratio)${NC}"
    fi
}

# ============================================
# SILVER-SPECIFIC HELPERS
# ============================================

# Common silver denominations
SILVER_DIME_SC=1      # 1 SilverCent = 1 dime
SILVER_QUARTER_SC=2   # 2.5 SilverCents = 1 quarter (approx)
SILVER_DOLLAR_SC=10   # 10 SilverCents = 1 Morgan/Peace dollar equivalent

# Print silver denomination guide
print_denomination_guide() {
    print_section "SilverCents Denomination Guide"
    echo "  1 SC  = 1 Constitutional Silver Dime  (0.0715 oz)"
    echo "  2 SC  = 1 Constitutional Silver Quarter (approx)"
    echo "  10 SC = 10 Dimes (0.715 oz)"
    echo "  100 SC = 100 Dimes (7.15 oz)"
}
