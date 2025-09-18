#!/bin/bash

# Test script for the reserve API endpoint

echo "Testing Basis Reserve API endpoint..."

# Start server in background
cargo run -p basis_server > server.log 2>&1 &
SERVER_PID=$!

# Wait for server to start
sleep 2

# Test the reserve API endpoint
echo "Making request to /reserves/issuer/testpubkey..."

# Use netcat or similar to test the endpoint
if command -v nc &> /dev/null; then
    echo "GET /reserves/issuer/010101010101010101010101010101010101010101010101010101010101010101 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n" | nc localhost 3000 | head -20
else
    echo "netcat not available, testing with telnet..."
    { echo "GET /reserves/issuer/010101010101010101010101010101010101010101010101010101010101010101 HTTP/1.1"; echo "Host: localhost"; echo "Connection: close"; echo; sleep 1; } | telnet localhost 3000 2>/dev/null | head -20
fi

# Stop server
kill $SERVER_PID 2>/dev/null

echo "Test completed."