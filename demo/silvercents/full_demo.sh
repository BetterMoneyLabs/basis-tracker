#!/bin/bash


SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/utils.sh"


SERVER_URL="${SERVER_URL:-http://localhost:3048}"

VENDOR_PUBKEY="03aaaa000000000000000000000000000000000000000000000000000000000001"
CUSTOMER_PUBKEY="03bbbb000000000000000000000000000000000000000000000000000000000002"

VENDOR_RESERVE_OZ=50
VENDOR_RESERVE_SC=$(oz_to_sc $VENDOR_RESERVE_OZ)

clear
print_header "🪙 SilverCents Demo - Complete Workflow"

cat << 'EOF'
╭──────────────────────────────────────────────────────────────────╮
│  SilverCents are on-chain tokens on the Ergo Platform backed    │
│  1:1 by constitutional silver dimes and quarters.               │
│                                                                  │
│  This demo shows:                                                │
│    1. Vendor creates account with silver reserve                 │
│    2. Customer creates account                                   │
│    3. Vendor issues SilverCents notes to customer               │
│    4. Customer monitors and redeems notes                        │
╰──────────────────────────────────────────────────────────────────╯
EOF

echo ""
print_denomination_guide
echo ""


print_section "Step 1: Server Check"

if ! wait_for_server 5; then
    print_warning "Basis Tracker server not running"
    print_info "Attempting to start server..."
    
    # Try to start the server
    cd "$SCRIPT_DIR/../.." && ./run_server.sh &
    sleep 3
    
    if ! wait_for_server 10; then
        print_error "Could not start server. Please start manually:"
        echo "  cd $(dirname "$SCRIPT_DIR")/.. && ./run_server.sh"
        exit 1
    fi
fi

print_success "Server is ready"
echo ""


print_section "Step 2: Account Setup"

echo "🏪 Vendor Account:"
echo "   PubKey: ${VENDOR_PUBKEY:0:30}..."
echo "   Silver Reserve: $VENDOR_RESERVE_OZ troy oz ($VENDOR_RESERVE_SC SC)"
echo ""

echo "👤 Customer Account:"
echo "   PubKey: ${CUSTOMER_PUBKEY:0:30}..."
echo ""

ISSUANCE_FILE="/tmp/silvercents_issuance.log"
> "$ISSUANCE_FILE"
SUCCESSFUL_ISSUE_COUNT=0
ACTUAL_TOTAL_ISSUED=0

print_success "Accounts configured"
sleep 1


print_section "Step 3: Vendor Issues SilverCents to Customer"

NOTE_1_AMOUNT=10
echo ""
echo "📝 Issuing Note 1: $(format_balance $NOTE_1_AMOUNT)"

if create_note "$VENDOR_PUBKEY" "$CUSTOMER_PUBKEY" "$NOTE_1_AMOUNT"; then
    print_success "Note 1 issued!"
    timestamp=$(date +%s)
    echo "ISSUANCE:$NOTE_1_AMOUNT:$timestamp:$VENDOR_PUBKEY:$CUSTOMER_PUBKEY" >> "$ISSUANCE_FILE"
    SUCCESSFUL_ISSUE_COUNT=$((SUCCESSFUL_ISSUE_COUNT + 1))
    ACTUAL_TOTAL_ISSUED=$((ACTUAL_TOTAL_ISSUED + NOTE_1_AMOUNT))
else
    print_error "Failed to issue Note 1"
fi

sleep 1

NOTE_2_AMOUNT=25
echo ""
echo "📝 Issuing Note 2: $(format_balance $NOTE_2_AMOUNT)"

if create_note "$VENDOR_PUBKEY" "$CUSTOMER_PUBKEY" "$NOTE_2_AMOUNT"; then
    print_success "Note 2 issued!"
    timestamp=$(date +%s)
    echo "ISSUANCE:$NOTE_2_AMOUNT:$timestamp:$VENDOR_PUBKEY:$CUSTOMER_PUBKEY" >> "$ISSUANCE_FILE"
    SUCCESSFUL_ISSUE_COUNT=$((SUCCESSFUL_ISSUE_COUNT + 1))
    ACTUAL_TOTAL_ISSUED=$((ACTUAL_TOTAL_ISSUED + NOTE_2_AMOUNT))
else
    print_error "Failed to issue Note 2"
fi

sleep 1

NOTE_3_AMOUNT=15
echo ""
echo "📝 Issuing Note 3: $(format_balance $NOTE_3_AMOUNT)"

if create_note "$VENDOR_PUBKEY" "$CUSTOMER_PUBKEY" "$NOTE_3_AMOUNT"; then
    print_success "Note 3 issued!"
    timestamp=$(date +%s)
    echo "ISSUANCE:$NOTE_3_AMOUNT:$timestamp:$VENDOR_PUBKEY:$CUSTOMER_PUBKEY" >> "$ISSUANCE_FILE"
    SUCCESSFUL_ISSUE_COUNT=$((SUCCESSFUL_ISSUE_COUNT + 1))
    ACTUAL_TOTAL_ISSUED=$((ACTUAL_TOTAL_ISSUED + NOTE_3_AMOUNT))
else
    print_error "Failed to issue Note 3"
fi

TOTAL_ISSUED=$ACTUAL_TOTAL_ISSUED
echo ""
if [ $SUCCESSFUL_ISSUE_COUNT -gt 0 ]; then
    print_success "Total issued to customer: $(format_balance $TOTAL_ISSUED) ($SUCCESSFUL_ISSUE_COUNT notes)"
else
    print_error "No notes were successfully issued!"
fi

print_section "Step 4: Vendor Reserve Status"

remaining_reserve=$((VENDOR_RESERVE_SC - TOTAL_ISSUED))
collateral_ratio=$(echo "scale=4; $remaining_reserve / $TOTAL_ISSUED" | bc 2>/dev/null || echo "0")

echo "📊 Vendor Status:"
echo "   Total Issued:     $(format_balance $TOTAL_ISSUED)"
echo "   Remaining Reserve: $(format_balance $remaining_reserve)"
echo "   Initial Reserve:  $(format_balance $VENDOR_RESERVE_SC)"
echo -n "   Collateralization: "
display_collateralization $collateral_ratio

# Query server for on-chain status
echo ""
print_info "Querying server for real-time status..."
status=$(get_key_status "$VENDOR_PUBKEY")
if [ -n "$status" ]; then
    echo "Server Response: $status" | head -c 200
    echo "..."
fi

sleep 1

print_section "Step 5: Customer Redeems Portion of Notes"

REDEEM_AMOUNT=20
echo ""
echo "💵 Customer initiates redemption of $(format_balance $REDEEM_AMOUNT)"

redeem_response=$(initiate_redemption "$VENDOR_PUBKEY" "$CUSTOMER_PUBKEY" "$REDEEM_AMOUNT")

if [ -n "$redeem_response" ]; then
    print_success "Redemption initiated!"
    echo "   Response: ${redeem_response:0:150}..."
else
    print_info "Redemption request sent (mock mode)"
fi

sleep 1


print_section "Step 6: Final Summary"

CUSTOMER_REDEEMED=$REDEEM_AMOUNT
CUSTOMER_OUTSTANDING=$((TOTAL_ISSUED - CUSTOMER_REDEEMED))
updated_reserve=$((remaining_reserve + CUSTOMER_REDEEMED))
updated_debt=$((TOTAL_ISSUED - CUSTOMER_REDEEMED))
updated_ratio=$(echo "scale=4; $updated_reserve / $updated_debt" | bc 2>/dev/null || echo "0")

echo ""
echo "╭────────────────────────────────────────────────────────╮"
echo "│                    DEMO RESULTS                        │"
echo "├────────────────────────────────────────────────────────┤"
echo "│                                                        │"
echo "│  🏪 VENDOR (Issuer):                                   │"
printf "│     Initial Reserve:    %-30s│\n" "$(format_balance $VENDOR_RESERVE_SC)"
printf "│     Notes Issued:       %-30s│\n" "$(format_balance $TOTAL_ISSUED)"
printf "│     Notes Redeemed:     %-30s│\n" "$(format_balance $CUSTOMER_REDEEMED)"
printf "│     Outstanding Debt:   %-30s│\n" "$(format_balance $updated_debt)"
echo "│                                                        │"
echo "│  👤 CUSTOMER (Recipient):                              │"
printf "│     Notes Received:     %-30s│\n" "$(format_balance $TOTAL_ISSUED)"
printf "│     Redeemed:           %-30s│\n" "$(format_balance $CUSTOMER_REDEEMED)"
printf "│     Outstanding:        %-30s│\n" "$(format_balance $CUSTOMER_OUTSTANDING)"
echo "│                                                        │"
echo "│  📊 COLLATERALIZATION:                                 │"
printf "│     Current Ratio:      %-30s│\n" "$updated_ratio"
echo -n "│     Status:             "
display_collateralization $updated_ratio
printf "%*s│\n" $((29 - ${#updated_ratio})) ""
echo "│                                                        │"
echo "╰────────────────────────────────────────────────────────╯"

echo ""
print_header "🎉 Demo Complete!"

echo "What happened in this demo:"
echo "  1. ✅ Vendor set up 50 oz silver reserve ($VENDOR_RESERVE_SC SC)"
echo "  2. ✅ Vendor issued 3 notes totaling $TOTAL_ISSUED SC to customer"
echo "  3. ✅ Customer redeemed $CUSTOMER_REDEEMED SC"
echo "  4. ✅ Customer still holds $CUSTOMER_OUTSTANDING SC (redeemable anytime)"
echo ""
echo "This demonstrates how SilverCents enable:"
echo "  • Offchain payments backed by real silver"
echo "  • Collateralization tracking for trust"
echo "  • Seamless redemption when needed"
echo ""

print_info "To run individual scripts:"
echo "  Vendor:   ./vendor_issuer.sh"
echo "  Customer: ./customer_redeemer.sh"
echo ""

print_success "Thank you for trying SilverCents!"
