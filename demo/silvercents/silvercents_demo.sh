#!/bin/bash

# ============================================================================
# SilverCents Interactive Demo
# ============================================================================
# Automated demonstration of SilverCents in action
# Scenario: Portland Farmers Market - Saturday Morning
# ============================================================================

# Load utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/silvercents_utils.sh"

VENDOR_CLI="${SCRIPT_DIR}/silvercents_vendor.sh"
CUSTOMER_CLI="${SCRIPT_DIR}/silvercents_customer.sh"

# Demo configuration
DEMO_SPEED=${DEMO_SPEED:-2}  # Seconds between steps

# ============================================================================
# Helper Functions
# ============================================================================

demo_step() {
    local step_num=$1
    local description=$2
    
    echo ""
    log_header "Step ${step_num}: ${description}"
    sleep ${DEMO_SPEED}
}

demo_pause() {
    echo ""
    log_info "Press Enter to continue..."
    read
}

cleanup_demo() {
    log_info "Cleaning up demo data..."
    rm -f /tmp/silvercents_*.account
    rm -f /tmp/silvercents_*.dat
    log_success "Cleanup complete"
}

# ============================================================================
# Main Demo
# ============================================================================

main() {
    log_header "SilverCents Demo: Portland Farmers Market"
    echo -e "${CYAN}Scenario:${NC} Saturday morning at the farmers market"
    echo -e "${CYAN}Vendors:${NC} Bob's Farm Stand, Carol's Bakery, Dave's Coffee Cart"
    echo -e "${CYAN}Customers:${NC} Alice (regular shopper), Eve (new customer)"
    echo ""
    
    log_warning "This demo will create temporary accounts and simulate transactions"
    echo ""
    read -p "Press Enter to start the demo..."
    
    # Cleanup any existing demo data
    cleanup_demo
    
    # ========================================================================
    # PHASE 1: Vendor Setup
    # ========================================================================
    
    demo_step 1 "Vendors Initialize Accounts"
    
    log_info "Bob's Farm Stand initializing..."
    bash "${VENDOR_CLI}" init "Bob's Farm Stand" "Portland Farmers Market"
    
    log_info "Carol's Bakery initializing..."
    bash "${VENDOR_CLI}" init "Carol's Bakery" "Portland Farmers Market"
    
    log_info "Dave's Coffee Cart initializing..."
    bash "${VENDOR_CLI}" init "Dave's Coffee Cart" "Portland Farmers Market"
    
    # ========================================================================
    # PHASE 2: Reserve Creation
    # ========================================================================
    
    demo_step 2 "Vendors Create Silver-Backed Reserves"
    
    log_info "Bob creates reserve: 10 ERG + 1000 DexySilver tokens..."
    bash "${VENDOR_CLI}" create-reserve "Bob's Farm Stand" 10 1000
    
    log_info "Carol creates reserve: 8 ERG + 800 DexySilver tokens..."
    bash "${VENDOR_CLI}" create-reserve "Carol's Bakery" 8 800
    
    log_info "Dave creates reserve: 5 ERG + 500 DexySilver tokens..."
    bash "${VENDOR_CLI}" create-reserve "Dave's Coffee Cart" 5 500
    
    # ========================================================================
    # PHASE 3: Customer Setup
    # ========================================================================
    
    demo_step 3 "Customers Initialize Accounts"
    
    log_info "Alice initializing..."
    bash "${CUSTOMER_CLI}" init "Alice"
    
    log_info "Eve initializing..."
    bash "${CUSTOMER_CLI}" init "Eve"
    
    # Get customer pubkeys
    ALICE_PUBKEY=$(load_account "Alice")
    EVE_PUBKEY=$(load_account "Eve")
    
    # ========================================================================
    # PHASE 4: Transactions
    # ========================================================================
    
    demo_step 4 "Alice Buys Vegetables from Bob (50 SilverCents)"
    
    bash "${VENDOR_CLI}" issue "Bob's Farm Stand" "${ALICE_PUBKEY}" 50 "Fresh tomatoes and lettuce"
    
    # Record note for Alice
    BOB_PUBKEY=$(load_account "Bob's Farm Stand")
    echo "${BOB_PUBKEY}|50000000000|$(date +%s)|Bob's Farm Stand|Fresh tomatoes and lettuce" >> "/tmp/silvercents_notes_Alice.dat"
    
    demo_step 5 "Alice Buys Bread from Carol (30 SilverCents)"
    
    bash "${VENDOR_CLI}" issue "Carol's Bakery" "${ALICE_PUBKEY}" 30 "Sourdough bread"
    
    # Record note for Alice
    CAROL_PUBKEY=$(load_account "Carol's Bakery")
    echo "${CAROL_PUBKEY}|30000000000|$(date +%s)|Carol's Bakery|Sourdough bread" >> "/tmp/silvercents_notes_Alice.dat"
    
    demo_step 6 "Eve Buys Coffee from Dave (15 SilverCents)"
    
    bash "${VENDOR_CLI}" issue "Dave's Coffee Cart" "${EVE_PUBKEY}" 15 "Latte and muffin"
    
    # Record note for Eve
    DAVE_PUBKEY=$(load_account "Dave's Coffee Cart")
    echo "${DAVE_PUBKEY}|15000000000|$(date +%s)|Dave's Coffee Cart|Latte and muffin" >> "/tmp/silvercents_notes_Eve.dat"
    
    demo_step 7 "Alice Transfers 10 SilverCents to Eve"
    
    bash "${CUSTOMER_CLI}" transfer "Alice" "${EVE_PUBKEY}" 10
    
    # Update balances (simplified)
    echo "${BOB_PUBKEY}|10000000000|$(date +%s)|Alice (transfer)|P2P transfer" >> "/tmp/silvercents_notes_Eve.dat"
    
    demo_step 8 "Eve Redeems 20 SilverCents from Dave for Quarters"
    
    bash "${CUSTOMER_CLI}" redeem "Eve" "${DAVE_PUBKEY}" 20 quarters
    bash "${VENDOR_CLI}" redeem "Dave's Coffee Cart" "${EVE_PUBKEY}" 20 quarters
    
    # ========================================================================
    # PHASE 5: Final Status
    # ========================================================================
    
    demo_step 9 "Final Status Check"
    
    log_header "Vendor Status Summary"
    
    echo -e "\n${GREEN}Bob's Farm Stand:${NC}"
    bash "${VENDOR_CLI}" status "Bob's Farm Stand"
    
    echo -e "\n${GREEN}Carol's Bakery:${NC}"
    bash "${VENDOR_CLI}" status "Carol's Bakery"
    
    echo -e "\n${GREEN}Dave's Coffee Cart:${NC}"
    bash "${VENDOR_CLI}" status "Dave's Coffee Cart"
    
    log_header "Customer Balances"
    
    echo -e "\n${GREEN}Alice:${NC}"
    bash "${CUSTOMER_CLI}" balance "Alice"
    bash "${CUSTOMER_CLI}" list "Alice"
    
    echo -e "\n${GREEN}Eve:${NC}"
    bash "${CUSTOMER_CLI}" balance "Eve"
    bash "${CUSTOMER_CLI}" list "Eve"
    
    # ========================================================================
    # Summary
    # ========================================================================
    
    log_header "Demo Complete!"
    
    echo -e "${CYAN}Summary:${NC}"
    echo -e "  ✓ 3 vendors created silver-backed reserves"
    echo -e "  ✓ 2 customers initialized accounts"
    echo -e "  ✓ 3 purchases made with SilverCents"
    echo -e "  ✓ 1 peer-to-peer transfer completed"
    echo -e "  ✓ 1 redemption for physical silver"
    echo ""
    echo -e "${GREEN}SilverCents successfully demonstrated!${NC}"
    echo ""
    
    log_info "Demo data preserved in /tmp/silvercents_*"
    log_info "Run './silvercents_demo.sh cleanup' to remove demo data"
}

# ============================================================================
# Entry Point
# ============================================================================

if [ "$1" = "cleanup" ]; then
    cleanup_demo
else
    main
fi
