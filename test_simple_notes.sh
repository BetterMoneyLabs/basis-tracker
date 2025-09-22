#!/bin/bash

# Test the simple test notes issuer endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_simple_notes_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

# Test the simple test route
echo "Testing simple test notes route..."
response=$(curl -s -w "%{http_code}" http://127.0.0.1:3000/test_simple/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null