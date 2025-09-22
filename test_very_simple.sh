#!/bin/bash

# Test the very simple GET endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_very_simple_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

# Test the very simple route
echo "Testing very simple route..."
response=$(curl -s -w "%{http_code}" http://localhost:3000/very_simple)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null