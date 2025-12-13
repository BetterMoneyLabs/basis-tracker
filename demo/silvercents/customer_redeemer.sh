#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/utils.sh"

SERVER_URL="${SERVER_URL:-http://localhost:3048}"

CUSTOMER_PUBKEY="${CUSTOMER_PUBKEY:-03bbbb000000000000000000000000000000000000000000000000000000000002}"

VENDOR_PUBKEY="${VENDOR_PUBKEY:-03aaaa000000000000000000000000000000000000000000000000000000000001}"

POLL_INTERVAL="${POLL_INTERVAL:-10}"

MIN_COLLATERALIZATION="${MIN_COLLATERALIZATION:-0.5}"
AUTO_REDEEM_THRESHOLD="${AUTO_REDEEM_THRESHOLD:-0}"

print_header "👤 SilverCents Customer Demo"

echo "Server URL:      $SERVER_URL"
echo "Customer PubKey: ${CUSTOMER_PUBKEY:0:20}..."
echo "Vendor PubKey:   ${VENDOR_PUBKEY:0:20}..."
echo "Min Collateral:  $MIN_COLLATERALIZATION"
echo ""

print_denomination_guide

if ! wait_for_server 10; then
    print_error "Cannot connect to Basis Tracker server"
    print_info "Start the server with: ./run_server.sh"
    exit 1
fi


print_section "Starting Monitoring Loop"
print_info "Polling every ${POLL_INTERVAL}s (Ctrl+C to stop)"
echo ""

ISSUANCE_FILE="/tmp/silvercents_issuance.log"

total_received=0
total_redeemed=0
last_timestamp=0
notes_seen=0

while true; do
    if [ -f "$ISSUANCE_FILE" ]; then
        while IFS= read -r line; do
            if [[ $line == ISSUANCE:* ]]; then
                amount=$(echo "$line" | cut -d: -f2)
                timestamp=$(echo "$line" | cut -d: -f3)
                issuer=$(echo "$line" | cut -d: -f4)
                recipient=$(echo "$line" | cut -d: -f5)
                
                if [ "$recipient" == "$CUSTOMER_PUBKEY" ] && [ "$timestamp" -gt "$last_timestamp" ]; then
                    notes_seen=$((notes_seen + 1))
                    total_received=$((total_received + amount))
                    last_timestamp=$timestamp
                    
                    echo ""
                    print_success "📨 New SilverCents received!"
                    echo "   Amount: $(format_balance $amount)"
                    echo "   From: ${issuer:0:20}..."
                    echo "   Time: $(date -r $timestamp 2>/dev/null || date -d @$timestamp 2>/dev/null || echo $timestamp)"
                fi
            fi
        done < "$ISSUANCE_FILE"
    fi
    
    status_response=$(get_key_status "$VENDOR_PUBKEY")
    
    if [ -n "$status_response" ]; then
        total_debt=$(echo "$status_response" | grep -o '"total_debt":[0-9]*' | grep -o '[0-9]*' | head -1)
        collateral=$(echo "$status_response" | grep -o '"collateral":[0-9]*' | grep -o '[0-9]*' | head -1)
        ratio=$(echo "$status_response" | grep -o '"collateralization_ratio":[0-9.]*' | grep -o '[0-9.]*' | head -1)
        
        total_debt=${total_debt:-0}
        collateral=${collateral:-0}
        ratio=${ratio:-0}
    fi
    
    outstanding=$((total_received - total_redeemed))
    
    echo ""
    print_section "Customer Status"
    echo "📊 Your Holdings:"
    echo "   Total Received: $(format_balance $total_received)"
    echo "   Redeemed:       $(format_balance $total_redeemed)"
    echo "   Outstanding:    $(format_balance $outstanding)"
    echo ""
    echo "🏪 Vendor Status:"
    echo "   Vendor Debt:    $(format_balance ${total_debt:-0})"
    echo "   Collateral:     $(format_balance ${collateral:-0})"
    echo -n "   Collateral Ratio: "
    display_collateralization ${ratio:-0}
    
    if [ -n "$ratio" ] && (( $(echo "$ratio > 0 && $ratio < $MIN_COLLATERALIZATION" | bc -l 2>/dev/null) )); then
        print_warning "Vendor collateralization below minimum! Consider redeeming."
    fi
    
    if [ "$AUTO_REDEEM_THRESHOLD" -gt 0 ] && [ "$outstanding" -ge "$AUTO_REDEEM_THRESHOLD" ]; then
        print_info "Auto-redeem threshold reached. Initiating redemption..."
        
        redeem_amount=$((outstanding / 2)) 
        
        response=$(initiate_redemption "$VENDOR_PUBKEY" "$CUSTOMER_PUBKEY" "$redeem_amount")
        
        if [ -n "$response" ]; then
            total_redeemed=$((total_redeemed + redeem_amount))
            print_success "Redeemed $(format_balance $redeem_amount)"
        else
            print_error "Redemption failed"
        fi
    fi
    
    print_info "Next poll in ${POLL_INTERVAL}s..."
    sleep $POLL_INTERVAL
done
