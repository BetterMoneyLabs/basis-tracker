#!/bin/bash

# Test script for the create note endpoint
# This demonstrates how to use the REST API

echo "Testing Basis Tracker API create note endpoint"
echo "============================================="

# Start the server in the background
cd /home/kushti/bml/basis-tracker
cargo run -p basis_server &
SERVER_PID=$!

# Wait for server to start
sleep 2

echo ""
echo "Server is running on http://localhost:3000"
echo ""

# Test the root endpoint
echo "Testing root endpoint:"
curl -s http://localhost:3000/
echo ""
echo ""

# Create a sample note request (using hex-encoded byte arrays)
# Note: In a real application, you'd use proper binary data
# For testing, we'll use simple patterns

cat > /tmp/test_note.json << 'EOF'
{
  "recipient_pubkey": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],
  "amount": 1000,
  "timestamp": 1234567890,
  "signature": [3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3],
  "issuer_pubkey": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]
}
EOF

echo "Testing create note endpoint:"
echo "Request payload:"
cat /tmp/test_note.json
echo ""
echo ""

echo "Response: (curl command would be used here)"
echo "To test manually, run:"
echo "curl -X POST http://localhost:3000/notes \\"
echo "  -H \"Content-Type: application/json\" \\"
echo "  -d '$(cat /tmp/test_note.json)'"

echo ""
echo ""

# Clean up
kill $SERVER_PID 2>/dev/null
rm /tmp/test_note.json

echo "Test completed!"