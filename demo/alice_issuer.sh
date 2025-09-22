#!/bin/bash

# Alice Issuer Demo Script
SERVER_URL="http://localhost:3000"
# 33-byte public keys encoded as 66 hex characters
ALICE_PUBKEY="010101010101010101010101010101010101010101010101010101010101010101"
BOB_PUBKEY="020202020202020202020202020202020202020202020202020202020202020202"
RESERVE_BALANCE=1000000
ISSUE_INTERVAL=30
AMOUNT_MIN=100
AMOUNT_MAX=1000

echo "Starting Alice issuer demo"
echo "Server: $SERVER_URL"
echo "Alice pubkey: $ALICE_PUBKEY"
echo "Bob pubkey: $BOB_PUBKEY"
echo "Initial reserve: $RESERVE_BALANCE"
echo "Issue interval: ${ISSUE_INTERVAL}s"

total_issued=0
available_reserve=$RESERVE_BALANCE

while [ $available_reserve -ge $AMOUNT_MIN ]; do
    # Generate random amount
    amount=$(( RANDOM % (AMOUNT_MAX - AMOUNT_MIN + 1) + AMOUNT_MIN ))
    
    if [ $amount -gt $available_reserve ]; then
        echo "Amount $amount exceeds available reserve $available_reserve. Skipping."
        sleep $ISSUE_INTERVAL
        continue
    fi
    
    # Create timestamp and hex-encoded signature for demo
    timestamp=$(date +%s)
    # 64-byte signature encoded as 128 hex characters (non-zero to pass validation)
    # Use different signature for each note to avoid AVL tree key collisions
    signature=$(printf "%0128d" $timestamp | sed 's/0/1/g')
    
    echo "Issuing note to Bob: $amount units"
    
    # Create note via HTTP API
    response=$(curl -s -X POST "$SERVER_URL/notes" \
        -H "Content-Type: application/json" \
        -d "{\"issuer_pubkey\":\"$ALICE_PUBKEY\",\"recipient_pubkey\":\"$BOB_PUBKEY\",\"amount\":$amount,\"timestamp\":$timestamp,\"signature\":\"$signature\"}" \
        -w "%{http_code}")
    
    http_code=${response: -3}
    response_body=${response:0:-3}
    
    if [ "$http_code" -eq 201 ]; then
        total_issued=$((total_issued + amount))
        available_reserve=$((available_reserve - amount))
        
        collateralization=$(echo "scale=2; $available_reserve * 100 / $total_issued" | bc)
        echo "Note issued successfully! Total issued: $total_issued, Available reserve: $available_reserve, Collateralization: ${collateralization}%"
        
        # Record issuance for Bob to track
        ISSUANCE_FILE="/tmp/alice_issuance.log"
        echo "ISSUANCE:$amount:$timestamp" >> "$ISSUANCE_FILE"
    else
        echo "Failed to issue note: HTTP $http_code - $response_body"
    fi
    
    sleep $ISSUE_INTERVAL
done

echo "Insufficient reserve: $available_reserve. Stopping issuance."
echo "Demo completed. Total issued: $total_issued, Final reserve: $available_reserve"