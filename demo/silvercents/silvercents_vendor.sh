#!/bin/bash

# ============================================================================
# SilverCents Vendor CLI
# ============================================================================
# Command-line interface for vendors to manage silver-backed reserves
# and issue SilverCents to customers
# ============================================================================

# Load utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/silvercents_utils.sh"

# ============================================================================
# Commands
# ============================================================================

cmd_init() {
    local vendor_name=$1
    local location=${2:-"Unknown Location"}
    
    if [ -z "${vendor_name}" ]; then
        log_error "Usage: $0 init <vendor_name> [location]"
        exit 1
    fi
    
    log_header "Initializing Vendor: ${vendor_name}"
    
    # Generate keypair
    local pubkey=$(generate_keypair "${vendor_name}")
    
    # Save account
    local account_file=$(save_account "${vendor_name}" "${pubkey}" "location=${location}")
    
    log_success "Vendor account created!"
    echo -e "  Name:       ${GREEN}${vendor_name}${NC}"
    echo -e "  Location:   ${location}"
    echo -e "  Public Key: ${CYAN}${pubkey}${NC}"
    echo -e "  Account:    ${account_file}"
    echo ""
    log_info "Next step: Create a reserve with 'create-reserve' command"
}

cmd_create_reserve() {
    local vendor_name=$1
    local erg_collateral=$2
    local dexysilver_tokens=$3
    
    if [ -z "${vendor_name}" ] || [ -z "${erg_collateral}" ] || [ -z "${dexysilver_tokens}" ]; then
        log_error "Usage: $0 create-reserve <vendor_name> <erg_collateral> <dexysilver_tokens>"
        exit 1
    fi
    
    # Load vendor account
    local pubkey=$(load_account "${vendor_name}")
    if [ $? -ne 0 ]; then
        log_error "Vendor '${vendor_name}' not found. Run 'init' first."
        exit 1
    fi
    
    log_header "Creating Silver-Backed Reserve"
    
    # Convert ERG to nanoERG
    local erg_nanoerg=$(echo "${erg_collateral} * 1000000000" | bc | cut -d'.' -f1)
    
    # Calculate total collateral value
    local dexysilver_value=$(echo "${dexysilver_tokens} * ${DEXYSILVER_VALUE_PER_TOKEN}" | bc)
    local total_value=$(echo "${erg_nanoerg} + ${dexysilver_value}" | bc)
    
    # Save reserve
    save_reserve "${vendor_name}" "${pubkey}" "${erg_nanoerg}" "${dexysilver_tokens}"
    
    log_success "Reserve created successfully!"
    echo -e "  Vendor:             ${GREEN}${vendor_name}${NC}"
    echo -e "  ERG Collateral:     ${erg_collateral} ERG (${erg_nanoerg} nanoERG)"
    echo -e "  DexySilver Tokens:  ${dexysilver_tokens}"
    echo -e "  Total Value:        $(format_nanoerg ${total_value}) ERG"
    echo -e "  Min Collateral:     50% (DexySilver backing)"
    echo ""
    log_info "Reserve is ready! You can now issue SilverCents."
}

cmd_issue() {
    local vendor_name=$1
    local customer_pubkey=$2
    local amount=$3
    local memo=${4:-""}
    
    if [ -z "${vendor_name}" ] || [ -z "${customer_pubkey}" ] || [ -z "${amount}" ]; then
        log_error "Usage: $0 issue <vendor_name> <customer_pubkey> <amount> [memo]"
        exit 1
    fi
    
    # Validate amount
    validate_amount "${amount}" || exit 1
    validate_pubkey "${customer_pubkey}" || exit 1
    
    # Load vendor account
    local vendor_pubkey=$(load_account "${vendor_name}")
    if [ $? -ne 0 ]; then
        log_error "Vendor '${vendor_name}' not found"
        exit 1
    fi
    
    # Load reserve
    load_reserve "${vendor_name}"
    if [ $? -ne 0 ]; then
        log_error "Reserve not found for '${vendor_name}'. Create one first."
        exit 1
    fi
    
    log_header "Issuing SilverCents"
    
    # Convert amount to nanoERG
    local amount_nanoerg=$(echo "${amount} * 1000000000" | bc | cut -d'.' -f1)
    
    # Calculate new issued amount
    local new_issued=$(echo "${ISSUED_AMOUNT} + ${amount_nanoerg}" | bc)
    
    # Check collateralization
    local ratio=$(calculate_collateral_ratio "${ERG_COLLATERAL}" "${DEXYSILVER_TOKENS}" "${new_issued}")
    
    if ! is_collateral_acceptable "${ratio}"; then
        log_error "Insufficient collateralization!"
        echo -e "  Current ratio: ${ratio}x"
        echo -e "  Minimum required: ${MIN_COLLATERAL_RATIO}x"
        exit 1
    fi
    
    # Generate signature
    local timestamp=$(date +%s)
    local signature=$(generate_mock_signature "${vendor_pubkey}" "${customer_pubkey}" "${amount_nanoerg}" "${timestamp}")
    
    # Create note via API
    log_info "Creating note..."
    local response=$(api_create_note "${vendor_pubkey}" "${customer_pubkey}" "${amount_nanoerg}" "${timestamp}" "${signature}")
    
    # Update issued amount
    update_issued_amount "${vendor_name}" "${new_issued}"
    
    # Get silver equivalent
    local silver=$(get_silver_breakdown "${amount}")
    
    log_success "SilverCents issued successfully!"
    echo -e "  Vendor:             ${GREEN}${vendor_name}${NC}"
    echo -e "  Customer:           ${CYAN}$(format_pubkey ${customer_pubkey})${NC}"
    echo -e "  Amount:             ${amount} SilverCents"
    echo -e "  Silver Equivalent:  ${silver}"
    if [ -n "${memo}" ]; then
        echo -e "  Memo:               ${memo}"
    fi
    echo -e "  New Collateral:     ${ratio}x ($(get_collateral_status ${ratio}))"
    echo ""
}

cmd_status() {
    local vendor_name=$1
    
    if [ -z "${vendor_name}" ]; then
        log_error "Usage: $0 status <vendor_name>"
        exit 1
    fi
    
    # Load reserve
    load_reserve "${vendor_name}"
    if [ $? -ne 0 ]; then
        log_error "Reserve not found for '${vendor_name}'"
        exit 1
    fi
    
    log_header "Vendor Status: ${vendor_name}"
    
    display_reserve_status "${vendor_name}"
    
    # Calculate available credit
    local ratio=$(calculate_collateral_ratio "${ERG_COLLATERAL}" "${DEXYSILVER_TOKENS}" "${ISSUED_AMOUNT}")
    local total_collateral=$(echo "${ERG_COLLATERAL} + (${DEXYSILVER_TOKENS} * ${DEXYSILVER_VALUE_PER_TOKEN})" | bc)
    local max_issuable=$(echo "${total_collateral} / ${MIN_COLLATERAL_RATIO}" | bc)
    local available=$(echo "${max_issuable} - ${ISSUED_AMOUNT}" | bc)
    
    echo ""
    echo -e "${CYAN}Available Credit${NC}"
    print_divider
    echo -e "  Max Issuable:        $(format_nanoerg ${max_issuable}) SC"
    echo -e "  Already Issued:      $(format_nanoerg ${ISSUED_AMOUNT}) SC"
    echo -e "  Available:           $(format_nanoerg ${available}) SC"
    print_divider
    echo ""
}

cmd_redeem() {
    local vendor_name=$1
    local customer_pubkey=$2
    local amount=$3
    local silver_type=${4:-"dimes"}
    
    if [ -z "${vendor_name}" ] || [ -z "${customer_pubkey}" ] || [ -z "${amount}" ]; then
        log_error "Usage: $0 redeem <vendor_name> <customer_pubkey> <amount> [silver_type]"
        exit 1
    fi
    
    # Validate
    validate_amount "${amount}" || exit 1
    validate_pubkey "${customer_pubkey}" || exit 1
    
    # Load reserve
    load_reserve "${vendor_name}"
    if [ $? -ne 0 ]; then
        log_error "Reserve not found for '${vendor_name}'"
        exit 1
    fi
    
    log_header "Processing Redemption"
    
    # Convert amount to nanoERG
    local amount_nanoerg=$(echo "${amount} * 1000000000" | bc | cut -d'.' -f1)
    
    # Update issued amount (decrease)
    local new_issued=$(echo "${ISSUED_AMOUNT} - ${amount_nanoerg}" | bc)
    if [ "${new_issued}" -lt 0 ]; then
        new_issued=0
    fi
    
    update_issued_amount "${vendor_name}" "${new_issued}"
    
    # Calculate silver to provide
    local silver=$(get_silver_breakdown "${amount}" "${silver_type}")
    
    log_success "Redemption processed!"
    echo -e "  Vendor:             ${GREEN}${vendor_name}${NC}"
    echo -e "  Customer:           ${CYAN}$(format_pubkey ${customer_pubkey})${NC}"
    echo -e "  Amount Redeemed:    ${amount} SilverCents"
    echo -e "  Silver to Provide:  ${silver}"
    echo -e "  Remaining Debt:     $(format_nanoerg ${new_issued}) SC"
    echo ""
    log_info "Please provide ${silver} to customer"
}

cmd_help() {
    cat <<EOF
${MAGENTA}SilverCents Vendor CLI${NC}

${CYAN}USAGE:${NC}
  $0 <command> [options]

${CYAN}COMMANDS:${NC}
  ${GREEN}init${NC} <vendor_name> [location]
      Initialize a new vendor account
      
  ${GREEN}create-reserve${NC} <vendor_name> <erg_collateral> <dexysilver_tokens>
      Create a silver-backed reserve
      Example: $0 create-reserve "Bob's Farm" 10 1000
      
  ${GREEN}issue${NC} <vendor_name> <customer_pubkey> <amount> [memo]
      Issue SilverCents to a customer
      Example: $0 issue "Bob's Farm" 02abc... 50 "Fresh vegetables"
      
  ${GREEN}status${NC} <vendor_name>
      Check reserve status and available credit
      
  ${GREEN}redeem${NC} <vendor_name> <customer_pubkey> <amount> [silver_type]
      Process redemption request from customer
      silver_type: "dimes" or "quarters" (default: dimes)
      
  ${GREEN}help${NC}
      Show this help message

${CYAN}EXAMPLES:${NC}
  # Initialize vendor
  $0 init "Bob's Farm Stand" "Portland Farmers Market"
  
  # Create reserve with 10 ERG and 1000 DexySilver tokens
  $0 create-reserve "Bob's Farm Stand" 10 1000
  
  # Issue 50 SilverCents to customer
  $0 issue "Bob's Farm Stand" 02e58b5f... 50 "Tomatoes and lettuce"
  
  # Check status
  $0 status "Bob's Farm Stand"
  
  # Process redemption for 25 SilverCents in quarters
  $0 redeem "Bob's Farm Stand" 02e58b5f... 25 quarters

${CYAN}NOTES:${NC}
  - SilverCents require 50% DexySilver token backing
  - 1 SilverCent = 1 silver dime equivalent
  - 2.5 dimes = 1 quarter
  - Collateralization is monitored automatically

EOF
}

# ============================================================================
# Main
# ============================================================================

main() {
    local command=$1
    shift
    
    case "${command}" in
        init)
            cmd_init "$@"
            ;;
        create-reserve)
            cmd_create_reserve "$@"
            ;;
        issue)
            cmd_issue "$@"
            ;;
        status)
            cmd_status "$@"
            ;;
        redeem)
            cmd_redeem "$@"
            ;;
        help|--help|-h|"")
            cmd_help
            ;;
        *)
            log_error "Unknown command: ${command}"
            echo ""
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
