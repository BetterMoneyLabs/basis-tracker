#!/bin/bash

# Simple HTTP test to see if the notes endpoint works

cd /home/kushti/bml/basis-tracker

echo "Starting server..."
target/debug/basis_server > server_test.log 2>&1 &
SERVER_PID=$!

sleep 3

echo "Testing /notes/issuer endpoint..."

# Use curl if available, otherwise use netcat
if command -v curl &> /dev/null; then
    response=$(curl -s -w "%{http_code}" http://localhost:3000/notes/issuer/010101010101010101010101010101010101010101010101010101010101010101)
    http_code=${response: -3}
    response_body=${response:0:-3}
else
    # Use netcat
    response=$(echo -e "GET /notes/issuer/010101010101010101010101010101010101010101010101010101010101010101 HTTP/1.1\nHost: localhost:3000\n\n" | timeout 5 nc localhost 3000 | head -1)
    http_code=$(echo "$response" | awk '{print $2}')
    response_body=""
fi

echo "HTTP Status: $http_code"
echo "Response: $response_body"

# Clean up
kill $SERVER_PID 2>/dev/null