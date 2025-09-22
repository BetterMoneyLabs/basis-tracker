#!/bin/bash

# Robust test script for Basis tracker

cd /home/kushti/bml/basis-tracker

echo "Building server..."
cargo build -p basis_server

echo "Starting server in background..."
target/debug/basis_server > robust_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to be ready
echo "Waiting for server to start..."
for i in {1..30}; do
    if curl -s http://localhost:3000/ > /dev/null; then
        echo "Server is ready!"
        break
    fi
    sleep 1
    if [ $i -eq 30 ]; then
        echo "Server failed to start within 30 seconds"
        kill $SERVER_PID 2>/dev/null
        exit 1
    fi
done

echo ""
echo "=== Testing root endpoint ==="
curl -s http://localhost:3000/

echo ""
echo "=== Testing POST /notes ==="
timestamp=$(date +%s)
json_data='{
  "issuer_pubkey": "010101010101010101010101010101010101010101010101010101010101010101",
  "recipient_pubkey": "020202020202020202020202020202020202020202020202020202020202020202",
  "amount": 100,
  "timestamp": '$timestamp',
  "signature": "01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101"
}'

curl -s -X POST -H "Content-Type: application/json" -d "$json_data" http://localhost:3000/notes

echo ""
echo ""
echo "=== Testing GET /notes/issuer/... ==="
curl -s http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101

echo ""
echo ""
echo "=== Server logs (last 10 lines) ==="
tail -10 robust_test.log

# Clean up
echo ""
echo "Cleaning up..."
kill $SERVER_PID 2>/dev/null
rm -f robust_test.log