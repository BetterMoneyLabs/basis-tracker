#!/bin/bash

# Test POST /notes endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_post_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 3

# Test POST /notes
echo "Testing POST /notes..."
timestamp=$(date +%s)

# Use the same data as the Alice script
json_data='{
  "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
  "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
  "amount": 100,
  "timestamp": '$timestamp',
  "signature": "01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101"
}'

response=$(curl -s -X POST -H "Content-Type: application/json" -d "$json_data" -w "%{http_code}" http://localhost:3000/notes)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null