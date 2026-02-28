#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/utils.sh"

SERVER_URL="${SERVER_URL:-http://localhost:3048}"

VENDOR_PUBKEY="${VENDOR_PUBKEY:-03aaaa000000000000000000000000000000000000000000000000000000000001}"

CUSTOMER_PUBKEY="${CUSTOMER_PUBKEY:-03bbbb000000000000000000000000000000000000000000000000000000000002}"

SILVER_RESERVE_OZ="${SILVER_RESERVE_OZ:-100}"
SILVER_RESERVE_SC=$(oz_to_sc $SILVER_RESERVE_OZ)

ISSUE_INTERVAL="${ISSUE_INTERVAL:-30}"

AMOUNT_MIN="${AMOUNT_MIN:-5}"
AMOUNT_MAX="${AMOUNT_MAX:-50}"
SILVER_RESERVE_OZ="${SILVER_RESERVE_OZ:-100}"
SILVER_RESERVE_SC=$(oz_to_sc $SILVER_RESERVE_OZ)

ISSUE_INTERVAL="${ISSUE_INTERVAL:-30}"

AMOUNT_MIN="${AMOUNT_MIN:-5}"
AMOUNT_MAX="${AMOUNT_MAX:-50}"


print_header "🏪 SilverCents Vendor Demo"

echo "Server URL:      $SERVER_URL"
echo "Vendor PubKey:   ${VENDOR_PUBKEY:0:20}..."
echo "Customer PubKey: ${CUSTOMER_PUBKEY:0:20}..."
echo ""
print_section "Silver Reserve"
echo "Reserve: $SILVER_RESERVE_OZ troy oz"
echo "Equivalent: $SILVER_RESERVE_SC SilverCents"
echo ""

print_denomination_guide
if ! wait_for_server 10; then
    print_error "Cannot connect to Basis Tracker server"
    print_info "Start the server with: ./run_server.sh"
    exit 1
fi


print_section "Starting Issuance Loop"
print_info "Issuing notes every ${ISSUE_INTERVAL}s (Ctrl+C to stop)"
echo ""

total_issued=0
available_reserve=$SILVER_RESERVE_SC
note_count=0

ISSUANCE_FILE="/tmp/silvercents_issuance.log"
> "$ISSUANCE_FILE"  

while [ $available_reserve -ge $AMOUNT_MIN ]; do
    amount=$(( RANDOM % (AMOUNT_MAX - AMOUNT_MIN + 1) + AMOUNT_MIN ))
    
    if [ $amount -gt $available_reserve ]; then
        amount=$available_reserve
    fi
    
    if [ $amount -eq 0 ]; then
        break
    fi
    
    timestamp=$(date +%s)
    oz_amount=$(sc_to_oz $amount)
    
    echo ""
    print_section "Issuing Note #$((note_count + 1))"
    echo "Amount: $(format_balance $amount)"
    
    if create_note "$VENDOR_PUBKEY" "$CUSTOMER_PUBKEY" "$amount"; then
        total_issued=$((total_issued + amount))
        available_reserve=$((available_reserve - amount))
        note_count=$((note_count + 1))
        
        collateralization=$(echo "scale=4; $available_reserve / $total_issued" | bc 2>/dev/null || echo "0")
        
        print_success "Note issued successfully!"
        echo ""
        echo "📊 Current Status:"
        echo "   Total Issued:     $(format_balance $total_issued)"
        echo "   Remaining Reserve: $(format_balance $available_reserve)"
        echo -n "   Collateralization: "
        display_collateralization $collateralization
        
        echo "ISSUANCE:$amount:$timestamp:$VENDOR_PUBKEY:$CUSTOMER_PUBKEY" >> "$ISSUANCE_FILE"
        
        if (( $(echo "$collateralization < 1.0" | bc -l) )); then
            print_warning "Under-collateralized! Consider adding more silver reserve."
        elif (( $(echo "$collateralization < 1.5" | bc -l) )); then
            print_warning "Low collateralization. Consider slowing issuance."
        fi
    else
        print_error "Failed to issue note"
    fi
    
    if [ $available_reserve -ge $AMOUNT_MIN ]; then
        echo ""
        print_info "Next issuance in ${ISSUE_INTERVAL}s..."
        sleep $ISSUE_INTERVAL
    fi
done

print_section "Issuance Complete"
echo ""
echo "📋 Summary:"
echo "   Notes Issued:      $note_count"
echo "   Total Issued:      $(format_balance $total_issued)"
echo "   Remaining Reserve: $(format_balance $available_reserve)"
echo ""

if [ $available_reserve -lt $AMOUNT_MIN ]; then
    print_warning "Reserve depleted. Add more silver to continue issuing."
fi

print_success "Vendor demo completed!"
