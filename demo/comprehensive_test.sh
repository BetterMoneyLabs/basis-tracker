#!/bin/bash

# Comprehensive test: create note then try to retrieve it

cd /home/kushti/bml/basis-tracker

echo "Starting server in foreground..."
timeout 15 target/debug/basis_server > server_test.log 2>&1 &
SERVER_PID=$!

sleep 5

echo "=== Step 1: Creating a test note ==="
timestamp=$(date +%s)

# Create test note data
json_data='{
  "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
  "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
  "amount": 100,
  "timestamp": '$timestamp',
  "signature": "01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101"
}'

# Create note
response=$(curl -s -X POST -H "Content-Type: application/json" -d "$json_data" -w "%{http_code}" http://localhost:3000/notes)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Create note response: HTTP $http_code"
echo "Response body: $response_body"

sleep 2

echo ""
echo "=== Step 2: Trying to retrieve notes for Alice ==="

# Try to get notes for Alice
response=$(curl -s -w "%{http_code}" http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Get notes response: HTTP $http_code"
echo "Response body: $response_body"

# Clean up
kill $SERVER_PID 2>/dev/null

echo ""
echo "=== Test completed ==="