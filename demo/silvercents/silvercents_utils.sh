#!/bin/bash

# ============================================================================
# SilverCents Utilities
# ============================================================================
# Shared utility functions for SilverCents demo
# Provides API interaction, calculations, and formatting helpers
# ============================================================================

# Configuration
SERVER_URL="${SERVER_URL:-http://127.0.0.1:3048}"
DEXYSILVER_VALUE_PER_TOKEN=1000000000  # 1 ERG per DexySilver token (mock value)
SILVER_DIME_VALUE=100000000            # 0.1 ERG per silver dime equivalent
MIN_COLLATERAL_RATIO=0.5               # 50% minimum (DexySilver backing)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ============================================================================
# Logging Functions
# ============================================================================

log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

log_header() {
    echo -e "\n${MAGENTA}═══════════════════════════════════════════════════════${NC}"
    echo -e "${MAGENTA}  $1${NC}"
    echo -e "${MAGENTA}═══════════════════════════════════════════════════════${NC}\n"
}

# ============================================================================
# API Interaction Functions
# ============================================================================

api_create_note() {
    local issuer_pubkey=$1
    local recipient_pubkey=$2
    local amount=$3
    local timestamp=$4
    local signature=$5
    
    curl -s -X POST "${SERVER_URL}/notes" \
        -H "Content-Type: application/json" \
        -d "{
            \"issuer_pubkey\": \"${issuer_pubkey}\",
            \"recipient_pubkey\": \"${recipient_pubkey}\",
            \"amount\": ${amount},
            \"timestamp\": ${timestamp},
            \"signature\": \"${signature}\"
        }"
}

api_get_notes() {
    local issuer_pubkey=$1
    local recipient_pubkey=$2
    
    curl -s "${SERVER_URL}/notes?issuer=${issuer_pubkey}&recipient=${recipient_pubkey}"
}

api_get_status() {
    curl -s "${SERVER_URL}/status"
}

# ============================================================================
# Silver Conversion Functions
# ============================================================================

# Convert nanoERG to SilverCents (1:1 with ERG for simplicity)
nanoerg_to_silvercents() {
    local nanoerg=$1
    echo "scale=2; ${nanoerg} / 1000000000" | bc
}

# Convert SilverCents to silver dimes
silvercents_to_dimes() {
    local silvercents=$1
    # 1 SilverCent = 1 silver dime equivalent
    echo "${silvercents}"
}

# Convert dimes to quarters
dimes_to_quarters() {
    local dimes=$1
    # 2.5 dimes = 1 quarter
    echo "scale=1; ${dimes} / 2.5" | bc
}

# Get silver coin breakdown
get_silver_breakdown() {
    local silvercents=$1
    local prefer=${2:-"dimes"}  # "dimes" or "quarters"
    
    local dimes=$(silvercents_to_dimes "${silvercents}")
    local quarters=$(dimes_to_quarters "${dimes}")
    
    if [ "${prefer}" = "quarters" ]; then
        local whole_quarters=$(echo "${quarters}" | cut -d'.' -f1)
        local remaining_dimes=$(echo "scale=0; ${dimes} - (${whole_quarters} * 2.5)" | bc)
        echo "${whole_quarters} quarters, ${remaining_dimes} dimes"
    else
        echo "${dimes} dimes"
    fi
}

# ============================================================================
# Collateralization Functions
# ============================================================================

# Calculate collateralization ratio
calculate_collateral_ratio() {
    local erg_collateral=$1
    local dexysilver_tokens=$2
    local issued_amount=$3
    
    # Total collateral value = ERG + (DexySilver tokens * value per token)
    local dexysilver_value=$(echo "${dexysilver_tokens} * ${DEXYSILVER_VALUE_PER_TOKEN}" | bc)
    local total_collateral=$(echo "${erg_collateral} + ${dexysilver_value}" | bc)
    
    if [ "${issued_amount}" -eq 0 ]; then
        echo "999999.99"  # Infinite collateralization
        return
    fi
    
    # Ratio = total_collateral / issued_amount
    echo "scale=2; ${total_collateral} / ${issued_amount}" | bc
}

# Get collateralization status
get_collateral_status() {
    local ratio=$1
    
    local ratio_int=$(echo "${ratio}" | cut -d'.' -f1)
    
    if [ "${ratio_int}" -ge 200 ]; then
        echo "EXCELLENT"
    elif [ "${ratio_int}" -ge 150 ]; then
        echo "GOOD"
    elif [ "${ratio_int}" -ge 100 ]; then
        echo "ADEQUATE"
    elif [ "${ratio_int}" -ge 80 ]; then
        echo "WARNING"
    elif [ "${ratio_int}" -ge 50 ]; then
        echo "CRITICAL"
    else
        echo "UNDER-COLLATERALIZED"
    fi
}

# Check if collateralization is acceptable
is_collateral_acceptable() {
    local ratio=$1
    
    # Convert to integer for comparison
    local ratio_percent=$(echo "scale=0; ${ratio} * 100" | bc | cut -d'.' -f1)
    local min_percent=$(echo "scale=0; ${MIN_COLLATERAL_RATIO} * 100" | bc | cut -d'.' -f1)
    
    [ "${ratio_percent}" -ge "${min_percent}" ]
}

# ============================================================================
# Signature Functions (Mock for Demo)
# ============================================================================

generate_mock_signature() {
    local issuer_pubkey=$1
    local recipient_pubkey=$2
    local amount=$3
    local timestamp=$4
    
    # Generate deterministic mock signature
    echo -n "${issuer_pubkey}${recipient_pubkey}${amount}${timestamp}" | sha256sum | cut -d' ' -f1
}

# ============================================================================
# Formatting Functions
# ============================================================================

format_nanoerg() {
    local nanoerg=$1
    echo "scale=4; ${nanoerg} / 1000000000" | bc
}

format_pubkey() {
    local pubkey=$1
    echo "${pubkey:0:8}...${pubkey: -8}"
}

print_divider() {
    echo -e "${CYAN}───────────────────────────────────────────────────────${NC}"
}

# ============================================================================
# Account Management
# ============================================================================

# Generate mock keypair (for demo purposes)
generate_keypair() {
    local name=$1
    
    # Generate deterministic pubkey from name
    local pubkey=$(echo -n "${name}" | sha256sum | cut -d' ' -f1)
    # Add secp256k1 prefix (mock)
    pubkey="02${pubkey:0:62}"
    
    echo "${pubkey}"
}

# Save account
save_account() {
    local name=$1
    local pubkey=$2
    local metadata=$3
    
    local account_file="/tmp/silvercents_${name}.account"
    
    cat > "${account_file}" <<EOF
NAME=${name}
PUBKEY=${pubkey}
CREATED=$(date +%s)
METADATA=${metadata}
EOF
    
    echo "${account_file}"
}

# Load account
load_account() {
    local name=$1
    local account_file="/tmp/silvercents_${name}.account"
    
    if [ ! -f "${account_file}" ]; then
        return 1
    fi
    
    source "${account_file}"
    echo "${PUBKEY}"
}

# ============================================================================
# Reserve Management
# ============================================================================

# Save reserve
save_reserve() {
    local vendor_name=$1
    local pubkey=$2
    local erg_collateral=$3
    local dexysilver_tokens=$4
    
    local reserve_file="/tmp/silvercents_reserve_${vendor_name}.dat"
    
    cat > "${reserve_file}" <<EOF
VENDOR_NAME=${vendor_name}
PUBKEY=${pubkey}
ERG_COLLATERAL=${erg_collateral}
DEXYSILVER_TOKENS=${dexysilver_tokens}
ISSUED_AMOUNT=0
CREATED=$(date +%s)
EOF
    
    echo "${reserve_file}"
}

# Load reserve
load_reserve() {
    local vendor_name=$1
    local reserve_file="/tmp/silvercents_reserve_${vendor_name}.dat"
    
    if [ ! -f "${reserve_file}" ]; then
        return 1
    fi
    
    source "${reserve_file}"
}

# Update issued amount
update_issued_amount() {
    local vendor_name=$1
    local new_amount=$2
    
    local reserve_file="/tmp/silvercents_reserve_${vendor_name}.dat"
    
    if [ ! -f "${reserve_file}" ]; then
        return 1
    fi
    
    # Read current values
    source "${reserve_file}"
    
    # Update issued amount
    cat > "${reserve_file}" <<EOF
VENDOR_NAME=${VENDOR_NAME}
PUBKEY=${PUBKEY}
ERG_COLLATERAL=${ERG_COLLATERAL}
DEXYSILVER_TOKENS=${DEXYSILVER_TOKENS}
ISSUED_AMOUNT=${new_amount}
CREATED=${CREATED}
EOF
}

# ============================================================================
# Display Functions
# ============================================================================

display_reserve_status() {
    local vendor_name=$1
    
    load_reserve "${vendor_name}"
    
    local ratio=$(calculate_collateral_ratio "${ERG_COLLATERAL}" "${DEXYSILVER_TOKENS}" "${ISSUED_AMOUNT}")
    local status=$(get_collateral_status "${ratio}")
    local erg_display=$(format_nanoerg "${ERG_COLLATERAL}")
    local issued_display=$(format_nanoerg "${ISSUED_AMOUNT}")
    
    echo -e "${CYAN}Reserve Status: ${vendor_name}${NC}"
    print_divider
    echo -e "  ERG Collateral:      ${erg_display} ERG"
    echo -e "  DexySilver Tokens:   ${DEXYSILVER_TOKENS}"
    echo -e "  Issued SilverCents:  ${issued_display} SC"
    echo -e "  Collateral Ratio:    ${ratio}x"
    echo -e "  Status:              ${status}"
    print_divider
}

# ============================================================================
# Validation Functions
# ============================================================================

validate_amount() {
    local amount=$1
    
    if ! [[ "${amount}" =~ ^[0-9]+$ ]]; then
        log_error "Invalid amount: ${amount}"
        return 1
    fi
    
    if [ "${amount}" -le 0 ]; then
        log_error "Amount must be positive"
        return 1
    fi
    
    return 0
}

validate_pubkey() {
    local pubkey=$1
    
    if [ ${#pubkey} -ne 66 ]; then
        log_error "Invalid pubkey length: ${#pubkey} (expected 66)"
        return 1
    fi
    
    return 0
}

# ============================================================================
# Export Functions
# ============================================================================

export -f log_info log_success log_warning log_error log_header
export -f api_create_note api_get_notes api_get_status
export -f nanoerg_to_silvercents silvercents_to_dimes dimes_to_quarters get_silver_breakdown
export -f calculate_collateral_ratio get_collateral_status is_collateral_acceptable
export -f generate_mock_signature
export -f format_nanoerg format_pubkey print_divider
export -f generate_keypair save_account load_account
export -f save_reserve load_reserve update_issued_amount
export -f display_reserve_status
export -f validate_amount validate_pubkey
