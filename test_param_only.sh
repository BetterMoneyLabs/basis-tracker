#!/bin/bash

# Test the param-only GET endpoint

# Start server in background
cd /home/kushti/bml/basis-tracker
target/debug/basis_server > server_param_only_test.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to start
sleep 5

# Check if server is still running
if ps -p $SERVER_PID > /dev/null; then
    echo "Server is still running with PID: $SERVER_PID"
else
    echo "Server is not running"
    exit 1
fi

# Test the param-only route
echo "Testing param-only route..."
response=$(curl -s -w "%{http_code}" http://127.0.0.1:3000/param_only/test123)
http_code=${response: -3}
response_body=${response:0:-3}

echo "Response: $response_body"
echo "HTTP Code: $http_code"

# Clean up
kill $SERVER_PID 2>/dev/null