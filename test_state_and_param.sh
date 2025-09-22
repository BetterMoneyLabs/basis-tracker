#!/bin/bash

# Test the state-and-param GET endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_state_param_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

# Test the state-and-param route
echo "Testing state-and-param route..."
response=$(curl -s -w "%{http_code}" http://127.0.0.1:3000/state_and_param/test123)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null