#!/bin/bash

# Minimal test to check server basics without complex threading

# Test if we can compile and run a simple server
echo "Testing server compilation..."

if cargo check -p basis_server; then
    echo "✓ Server compiles successfully"
else
    echo "✗ Server compilation failed"
    exit 1
fi

# Test if the server binary exists
if [ -f "../target/debug/basis_server" ]; then
    echo "✓ Server binary exists"
else
    echo "✗ Server binary not found"
    exit 1
fi

# Test basic socket binding
echo "Testing socket binding..."

# Try to start server and immediately kill it
timeout 1 ../target/debug/basis_server > /dev/null 2>&1 &
SERVER_PID=$!
sleep 0.1

if kill -0 $SERVER_PID 2>/dev/null; then
    echo "✓ Server process started successfully"
    kill $SERVER_PID 2>/dev/null
else
    echo "✗ Server process failed to start"
    exit 1
fi

echo "Basic server test passed!"