#!/bin/bash

# Debug test script for Basis tracker

cd /home/kushti/bml/basis-tracker

echo "Starting server in background..."
target/debug/basis_server > debug_server.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

echo "=== Testing POST /notes ==="
timestamp=$(date +%s)
json_data='{
  "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
  "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
  "amount": 100,
  "timestamp": '$timestamp',
  "signature": "01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101"
}'

post_response=$(curl -s -X POST -H "Content-Type: application/json" -d "$json_data" -w "%{http_code}" http://localhost:3000/notes)
post_http_code=${post_response: -3}
post_response_body=${post_response:0:-3}

echo "POST Response: $post_response_body"
echo "POST HTTP Code: $post_http_code"

sleep 2

echo ""
echo "=== Testing GET /notes/issuer/... ==="

# Test with the correct pubkey
response=$(curl -s -X GET -w "%{http_code}" http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101)
http_code=${response: -3}
response_body=${response:0:-3}

echo "GET Response: $response_body"
echo "GET HTTP Code: $http_code"

sleep 2

echo ""
echo "=== Testing root endpoint ==="
root_response=$(curl -s -X GET -w "%{http_code}" http://localhost:3000/)
root_http_code=${root_response: -3}
root_response_body=${root_response:0:-3}

echo "Root Response: $root_response_body"
echo "Root HTTP Code: $root_http_code"

# Check server logs
echo ""
echo "=== Server logs (last 20 lines) ==="
tail -20 debug_server.log

# Clean up
echo ""
echo "Cleaning up..."
kill $SERVER_PID 2>/dev/null
rm -f debug_server.log