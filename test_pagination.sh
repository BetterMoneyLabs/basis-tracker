#!/bin/bash

# Test script for Basis Tracker pagination API with multiple event types

echo "Testing Basis Tracker pagination API with multiple event types..."
echo "==============================================================="

# Test basic health check
echo "1. Testing health check:"
curl -s http://localhost:3000/
echo -e "\n"

# Test paginated events with default parameters
echo "2. Testing paginated events (default page 0, size 20):"
echo "   Expected event types: NoteUpdated, ReserveCreated, ReserveToppedUp, CollateralAlert, ReserveRedeemed, Commitment"
curl -s "http://localhost:3000/events" | jq '.data[].type' 2>/dev/null || curl -s "http://localhost:3000/events" | grep -o '"type":"[^"]*"'
echo -e "\n"

# Test paginated events with custom parameters
echo "3. Testing paginated events (page 1, size 2):"
curl -s "http://localhost:3000/events?page=1&page_size=2" | jq '.data[] | {id, type, timestamp, amount, collateral_amount}' 2>/dev/null || curl -s "http://localhost:3000/events?page=1&page_size=2"
echo -e "\n"

# Test paginated events with just page parameter
echo "4. Testing paginated events (page 0, size 5):"
curl -s "http://localhost:3000/events?page=0&page_size=5" | jq '.data[] | {id, type, issuer_pubkey, recipient_pubkey, reserve_box_id}' 2>/dev/null || curl -s "http://localhost:3000/events?page=0&page_size=5"
echo -e "\n"

echo "Pagination API test completed! Multiple event types should be visible in the responses."