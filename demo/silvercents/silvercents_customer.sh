#!/bin/bash

# ============================================================================
# SilverCents Customer CLI
# ============================================================================
# Command-line interface for customers to receive, manage, and redeem
# SilverCents from vendors
# ============================================================================

# Load utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/silvercents_utils.sh"

# ============================================================================
# Commands
# ============================================================================

cmd_init() {
    local customer_name=$1
    
    if [ -z "${customer_name}" ]; then
        log_error "Usage: $0 init <customer_name>"
        exit 1
    fi
    
    log_header "Initializing Customer: ${customer_name}"
    
    # Generate keypair
    local pubkey=$(generate_keypair "${customer_name}")
    
    # Save account
    local account_file=$(save_account "${customer_name}" "${pubkey}" "type=customer")
    
    log_success "Customer account created!"
    echo -e "  Name:       ${GREEN}${customer_name}${NC}"
    echo -e "  Public Key: ${CYAN}${pubkey}${NC}"
    echo -e "  Account:    ${account_file}"
    echo ""
    log_info "You can now receive SilverCents from vendors"
}

cmd_balance() {
    local customer_name=$1
    
    if [ -z "${customer_name}" ]; then
        log_error "Usage: $0 balance <customer_name>"
        exit 1
    fi
    
    # Load customer account
    local pubkey=$(load_account "${customer_name}")
    if [ $? -ne 0 ]; then
        log_error "Customer '${customer_name}' not found. Run 'init' first."
        exit 1
    fi
    
    log_header "Balance: ${customer_name}"
    
    # Calculate total balance from all vendors
    local total_balance=0
    local vendor_count=0
    
    # List all notes received (from temp storage)
    local notes_file="/tmp/silvercents_notes_${customer_name}.dat"
    
    if [ -f "${notes_file}" ]; then
        while IFS='|' read -r vendor_pubkey amount timestamp; do
            total_balance=$(echo "${total_balance} + ${amount}" | bc)
            vendor_count=$((vendor_count + 1))
        done < "${notes_file}"
    fi
    
    local balance_sc=$(nanoerg_to_silvercents "${total_balance}")
    local silver=$(get_silver_breakdown "${balance_sc}")
    
    echo -e "${CYAN}Total Balance${NC}"
    print_divider
    echo -e "  SilverCents:         ${GREEN}${balance_sc} SC${NC}"
    echo -e "  Silver Equivalent:   ${silver}"
    echo -e "  Notes from Vendors:  ${vendor_count}"
    print_divider
    echo ""
}

cmd_list() {
    local customer_name=$1
    local vendor_filter=${2:-""}
    
    if [ -z "${customer_name}" ]; then
        log_error "Usage: $0 list <customer_name> [vendor_pubkey]"
        exit 1
    fi
    
    # Load customer account
    local pubkey=$(load_account "${customer_name}")
    if [ $? -ne 0 ]; then
        log_error "Customer '${customer_name}' not found"
        exit 1
    fi
    
    log_header "Received SilverCents: ${customer_name}"
    
    local notes_file="/tmp/silvercents_notes_${customer_name}.dat"
    
    if [ ! -f "${notes_file}" ]; then
        log_warning "No SilverCents received yet"
        return
    fi
    
    local count=0
    while IFS='|' read -r vendor_pubkey amount timestamp vendor_name memo; do
        # Filter by vendor if specified
        if [ -n "${vendor_filter}" ] && [ "${vendor_pubkey}" != "${vendor_filter}" ]; then
            continue
        fi
        
        count=$((count + 1))
        local amount_sc=$(nanoerg_to_silvercents "${amount}")
        local silver=$(get_silver_breakdown "${amount_sc}")
        local date=$(date -d "@${timestamp}" "+%Y-%m-%d %H:%M:%S" 2>/dev/null || date -r "${timestamp}" "+%Y-%m-%d %H:%M:%S")
        
        echo -e "${CYAN}Note #${count}${NC}"
        print_divider
        echo -e "  Vendor:             ${GREEN}${vendor_name}${NC}"
        echo -e "  Vendor Pubkey:      $(format_pubkey ${vendor_pubkey})"
        echo -e "  Amount:             ${amount_sc} SC"
        echo -e "  Silver Equivalent:  ${silver}"
        if [ -n "${memo}" ]; then
            echo -e "  Memo:               ${memo}"
        fi
        echo -e "  Received:           ${date}"
        print_divider
        echo ""
    done < "${notes_file}"
    
    if [ ${count} -eq 0 ]; then
        log_warning "No matching SilverCents found"
    else
        log_success "Found ${count} note(s)"
    fi
}

cmd_redeem() {
    local customer_name=$1
    local vendor_pubkey=$2
    local amount=$3
    local prefer=${4:-"dimes"}
    
    if [ -z "${customer_name}" ] || [ -z "${vendor_pubkey}" ] || [ -z "${amount}" ]; then
        log_error "Usage: $0 redeem <customer_name> <vendor_pubkey> <amount> [prefer]"
        exit 1
    fi
    
    # Validate
    validate_amount "${amount}" || exit 1
    validate_pubkey "${vendor_pubkey}" || exit 1
    
    # Load customer account
    local customer_pubkey=$(load_account "${customer_name}")
    if [ $? -ne 0 ]; then
        log_error "Customer '${customer_name}' not found"
        exit 1
    fi
    
    log_header "Redeeming SilverCents"
    
    # Calculate silver to receive
    local silver=$(get_silver_breakdown "${amount}" "${prefer}")
    
    log_info "Requesting redemption from vendor..."
    echo -e "  Customer:           ${GREEN}${customer_name}${NC}"
    echo -e "  Vendor:             $(format_pubkey ${vendor_pubkey})"
    echo -e "  Amount:             ${amount} SilverCents"
    echo -e "  Silver to Receive:  ${silver}"
    echo ""
    
    log_success "Redemption request created!"
    log_info "Present this request to the vendor to receive ${silver}"
    echo ""
    log_warning "Note: Vendor must approve and provide physical silver"
}

cmd_transfer() {
    local customer_name=$1
    local to_pubkey=$2
    local amount=$3
    
    if [ -z "${customer_name}" ] || [ -z "${to_pubkey}" ] || [ -z "${amount}" ]; then
        log_error "Usage: $0 transfer <customer_name> <to_pubkey> <amount>"
        exit 1
    fi
    
    # Validate
    validate_amount "${amount}" || exit 1
    validate_pubkey "${to_pubkey}" || exit 1
    
    # Load customer account
    local customer_pubkey=$(load_account "${customer_name}")
    if [ $? -ne 0 ]; then
        log_error "Customer '${customer_name}' not found"
        exit 1
    fi
    
    log_header "Transferring SilverCents"
    
    # Check balance
    local notes_file="/tmp/silvercents_notes_${customer_name}.dat"
    local total_balance=0
    
    if [ -f "${notes_file}" ]; then
        while IFS='|' read -r vendor_pubkey note_amount timestamp; do
            total_balance=$(echo "${total_balance} + ${note_amount}" | bc)
        done < "${notes_file}"
    fi
    
    local balance_sc=$(nanoerg_to_silvercents "${total_balance}")
    local amount_nanoerg=$(echo "${amount} * 1000000000" | bc | cut -d'.' -f1)
    
    if [ $(echo "${amount_nanoerg} > ${total_balance}" | bc) -eq 1 ]; then
        log_error "Insufficient balance!"
        echo -e "  Available: ${balance_sc} SC"
        echo -e "  Requested: ${amount} SC"
        exit 1
    fi
    
    log_success "Transfer initiated!"
    echo -e "  From:               ${GREEN}${customer_name}${NC}"
    echo -e "  To:                 $(format_pubkey ${to_pubkey})"
    echo -e "  Amount:             ${amount} SilverCents"
    echo ""
    log_info "Transfer completed (peer-to-peer)"
}

cmd_help() {
    cat <<EOF
${MAGENTA}SilverCents Customer CLI${NC}

${CYAN}USAGE:${NC}
  $0 <command> [options]

${CYAN}COMMANDS:${NC}
  ${GREEN}init${NC} <customer_name>
      Initialize a new customer account
      
  ${GREEN}balance${NC} <customer_name>
      Check total SilverCents balance
      
  ${GREEN}list${NC} <customer_name> [vendor_pubkey]
      List all received SilverCents (optionally filter by vendor)
      
  ${GREEN}redeem${NC} <customer_name> <vendor_pubkey> <amount> [prefer]
      Request redemption for physical silver
      prefer: "dimes" or "quarters" (default: dimes)
      
  ${GREEN}transfer${NC} <customer_name> <to_pubkey> <amount>
      Transfer SilverCents to another customer
      
  ${GREEN}help${NC}
      Show this help message

${CYAN}EXAMPLES:${NC}
  # Initialize customer
  $0 init "Alice"
  
  # Check balance
  $0 balance "Alice"
  
  # List all received SilverCents
  $0 list "Alice"
  
  # Redeem 25 SilverCents for quarters
  $0 redeem "Alice" 02abc... 25 quarters
  
  # Transfer 10 SilverCents to another customer
  $0 transfer "Alice" 02def... 10

${CYAN}NOTES:${NC}
  - SilverCents are received from vendors
  - 1 SilverCent = 1 silver dime equivalent
  - Redemption requires vendor approval
  - Transfers are peer-to-peer

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
        balance)
            cmd_balance "$@"
            ;;
        list)
            cmd_list "$@"
            ;;
        redeem)
            cmd_redeem "$@"
            ;;
        transfer)
            cmd_transfer "$@"
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
