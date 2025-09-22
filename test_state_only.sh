#!/bin/bash

# Test the state-only GET endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_state_only_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

# Test the state-only route
echo "Testing state-only route..."
response=$(curl -s -w "%{http_code}" http://127.0.0.1:3000/state_only)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null