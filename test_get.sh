#!/bin/bash

# Test GET /notes/issuer/{pubkey} endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_get_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

# First create a note
echo "Creating a test note..."
timestamp=$(date +%s)

json_data='{
  "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
  "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
  "amount": 100,
  "timestamp": '$timestamp',
  "signature": "01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101"
}'

curl -s -X POST -H "Content-Type: application/json" -d "$json_data" http://localhost:3000/notes > /dev/null

# Test GET /notes/issuer/{pubkey}
echo "Testing GET /notes/issuer/{pubkey}..."
response=$(curl -s -w "%{http_code}" http://localhost:3000/test_issuer/010101010101010101010101010101010101010101010101010101010101010101)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null