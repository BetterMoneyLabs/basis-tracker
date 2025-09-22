#!/bin/bash

# Bob Receiver Demo Script - Fixed version that tracks Alice's issuance
SERVER_URL="http://localhost:3000"
# 33-byte public keys encoded as 66 hex characters
BOB_PUBKEY="020202020202020202020202020202020202020202020202020202020202020202"
ALICE_PUBKEY="010101010101010101010101010101010101010101010101010101010101010101"
ALICE_RESERVE=1000000
POLL_INTERVAL=10
MIN_COLLATERALIZATION=1.0

echo "Starting Bob receiver demo"
echo "Server: $SERVER_URL"
echo "Bob pubkey: $BOB_PUBKEY"
echo "Alice's reserve: $ALICE_RESERVE"
echo "Min collateralization: $(echo "$MIN_COLLATERALIZATION * 100" | bc)%"
echo "Poll interval: ${POLL_INTERVAL}s"

total_received=0
notes_received=0
last_timestamp=""
accepting_notes=true

# Shared file to track Alice's issuance (created by Alice)
ISSUANCE_FILE="/tmp/alice_issuance.log"

# Cleanup function
cleanup() {
    rm -f "$ISSUANCE_FILE"
    exit 0
}

trap cleanup EXIT INT TERM

echo "Bob is now monitoring Alice's note issuance..."
echo "Using shared file tracking since GET endpoint is not available"
echo ""

while $accepting_notes; do
    echo "Checking for new notes from Alice..."
    
    # Try to get notes via HTTP API first (in case it gets fixed)
    response=$(curl -s -X GET "$SERVER_URL/notes/issuer/$ALICE_PUBKEY" -w "%{http_code}" 2>/dev/null)
    http_code=${response: -3}
    response_body=${response:0:-3}
    
    if [ "$http_code" -eq 200 ]; then
        # API is working - use proper API response
        if command -v jq >/dev/null 2>&1; then
            note_count=$(echo "$response_body" | jq '.data | length' 2>/dev/null || echo "0")
            
            if [ "$note_count" -gt 0 ]; then
                for i in $(seq 0 $((note_count - 1))); do
                    amount=$(echo "$response_body" | jq -r ".data[$i].amount" 2>/dev/null)
                    timestamp=$(echo "$response_body" | jq -r ".data[$i].timestamp" 2>/dev/null)
                    
                    if [ -z "$last_timestamp" ] || [ "$timestamp" -gt "$last_timestamp" ]; then
                        total_received=$((total_received + amount))
                        notes_received=$((notes_received + 1))
                        last_timestamp=$timestamp
                        
                        if [ $total_received -gt 0 ]; then
                            collateralization=$(echo "scale=4; $ALICE_RESERVE / $total_received" | bc)
                            collateralization_pct=$(echo "scale=2; $collateralization * 100" | bc)
                        else
                            collateralization=1.0
                            collateralization_pct=100.00
                        fi
                        
                        echo "Received note #$notes_received: $amount units (Total: $total_received)"
                        echo "Collateralization: ${collateralization_pct}%"
                        
                        if [ $(echo "$collateralization < $MIN_COLLATERALIZATION" | bc -l) -eq 1 ]; then
                            accepting_notes=false
                            echo "WARNING: Collateralization below minimum! ${collateralization_pct}% < $(echo "$MIN_COLLATERALIZATION * 100" | bc)%"
                            echo "Stopping note acceptance"
                            break
                        fi
                    fi
                done
            else
                echo "No notes found via API"
            fi
        else
            echo "API response received but jq not available for parsing"
        fi
    else
        # GET endpoint not available - use shared file tracking
        if [ -f "$ISSUANCE_FILE" ]; then
            # Read Alice's issuance log
            while IFS= read -r line; do
                if [[ "$line" == ISSUANCE:* ]]; then
                    # Parse issuance line: ISSUANCE:amount:timestamp
                    amount=$(echo "$line" | cut -d: -f2)
                    timestamp=$(echo "$line" | cut -d: -f3)
                    
                    # Process all notes in the file
                    total_received=$((total_received + amount))
                    notes_received=$((notes_received + 1))
                    # Update last_timestamp to the latest timestamp
                    if [ -z "$last_timestamp" ] || [ "$timestamp" -gt "$last_timestamp" ]; then
                        last_timestamp=$timestamp
                    fi
                        
                        # Calculate collateralization
                        if [ $total_received -gt 0 ]; then
                            collateralization=$(echo "scale=4; $ALICE_RESERVE / $total_received" | bc)
                            collateralization_pct=$(echo "scale=2; $collateralization * 100" | bc)
                        else
                            collateralization=1.0
                            collateralization_pct=100.00
                        fi
                        
                        echo "Received note #$notes_received: $amount units (Total: $total_received)"
                        echo "Collateralization: ${collateralization_pct}%"
                        
                        # Check if we should stop accepting
                        if [ $(echo "$collateralization < $MIN_COLLATERALIZATION" | bc -l) -eq 1 ]; then
                            accepting_notes=false
                            echo "WARNING: Collateralization below minimum! ${collateralization_pct}% < $(echo "$MIN_COLLATERALIZATION * 100" | bc)%"
                            echo "Stopping note acceptance"
                            break 2
                        fi
                    fi
            done < "$ISSUANCE_FILE"
            
            # Clear the file after reading to avoid duplicate processing
            > "$ISSUANCE_FILE"
        else
            echo "Waiting for Alice to start issuing notes..."
        fi
    fi
    
    if $accepting_notes; then
        sleep $POLL_INTERVAL
    fi
done

final_collateralization=$(echo "scale=2; $ALICE_RESERVE * 100 / $total_received" | bc)
echo ""
echo "Demo completed."
echo "Total notes received: $notes_received"
echo "Total amount received: $total_received"
echo "Final collateralization: ${final_collateralization}%"