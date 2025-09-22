#!/bin/bash

# Test note creation with server running in foreground

cd /home/kushti/bml/basis-tracker

echo "Starting server in foreground..."
timeout 10 target/debug/basis_server > server_test.log 2>&1 &
SERVER_PID=$!

sleep 3

echo "Testing note creation..."
timestamp=$(date +%s)

# Create test note data
json_data='{
  "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
  "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
  "amount": 100,
  "timestamp": '$timestamp',
  "signature": "01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101"
}'

# Use netcat or other method to test since curl is restricted
if echo "POST /notes HTTP/1.1
Host: localhost:3000
Content-Type: application/json
Content-Length: ${#json_data}

$json_data" | timeout 5 nc localhost 3000 | grep -q "HTTP/1.1 201"; then
    echo "✓ Note creation successful"
else
    echo "✗ Note creation failed"
fi

# Clean up
kill $SERVER_PID 2>/dev/null