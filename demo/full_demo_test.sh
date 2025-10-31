#!/bin/bash

# Full demo test script

cd /home/kushti/bml/basis-tracker

echo "=== Basis Tracker Full Demo Test ==="
echo ""

# Clean up any existing files
rm -f /tmp/alice_issuance.log

# Build the server
echo "Building server..."
cargo build -p basis_server

echo ""
echo "Starting server in background..."
target/debug/basis_server > server_demo.log 2>&1 &
SERVER_PID=$!

echo "Server started with PID: $SERVER_PID"

# Wait for server to be ready
echo "Waiting for server to start..."
for i in {1..30}; do
    if curl -s http://localhost:3048/ > /dev/null; then
        echo "Server is ready!"
        break
    fi
    sleep 1
    if [ $i -eq 30 ]; then
        echo "Server failed to start within 30 seconds"
        kill $SERVER_PID 2>/dev/null
        exit 1
    fi
done

echo ""
echo "=== Starting Alice Issuer ==="
cd demo
./alice_issuer.sh &
ALICE_PID=$!

echo "Alice started with PID: $ALICE_PID"

echo ""
echo "=== Starting Bob Receiver ==="
./bob_receiver.sh &
BOB_PID=$!

echo "Bob started with PID: $BOB_PID"

echo ""
echo "Demo running for 30 seconds..."
echo "Check the terminal windows for real-time updates"
echo ""

# Let the demo run for 30 seconds
sleep 30

echo ""
echo "=== Stopping Demo ==="

# Stop the processes
kill $ALICE_PID 2>/dev/null
kill $BOB_PID 2>/dev/null
kill $SERVER_PID 2>/dev/null

# Clean up
rm -f /tmp/alice_issuance.log
rm -f server_demo.log

echo "Demo completed!"
