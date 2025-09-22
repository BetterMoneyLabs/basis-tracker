#!/bin/bash

# Simple test to check if server basic functionality works

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_test.log 2>&1 &
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

# Test notes endpoint
echo "Testing notes endpoint..."
response=$(curl -s -w "%{http_code}" http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101)
http_code=${response: -3}

if [ "$http_code" = "200" ]; then
    echo "✓ Notes endpoint works"
else
    echo "✗ Notes endpoint failed with HTTP $http_code"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi

# Clean up
kill $SERVER_PID 2>/dev/null
echo "Basic server test passed!"