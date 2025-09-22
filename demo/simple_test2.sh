#!/bin/bash

# Test with simpler URL pattern

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_test2.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 3

# Test basic endpoint
echo "Testing root endpoint..."
if curl -s http://localhost:3000/ > /dev/null; then
    echo "✓ Root endpoint works"
else
    echo "✗ Root endpoint failed"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi

# Test with URL-encoded path
echo "Testing notes endpoint with URL encoding..."
response=$(curl -s -w "%{http_code}" "http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101")
http_code=${response: -3}

echo "Response: $response"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null