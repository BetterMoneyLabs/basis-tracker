#!/bin/bash

# Test the notes endpoint directly

cd /home/kushti/bml/basis-tracker

echo "Starting server in foreground..."
timeout 10 target/debug/basis_server > server_test.log 2>&1 &
SERVER_PID=$!

sleep 5

# First create a test note
echo "Creating test note..."
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

echo "Testing notes endpoint for Alice..."
# Use curl to test the notes endpoint
response=$(curl -s -X GET -w "%{http_code}" http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101)
http_code=${response: -3}
response_body=${response:0:-3}

echo "GET Response: $response_body"
echo "GET HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null