#!/bin/bash

# Test if the server is responding
echo "Testing server connection..."

# Try to connect to the server
if timeout 2 bash -c '</dev/tcp/localhost/3000'; then
    echo "✓ Server is listening on port 3000"
    
    # Try a simple HTTP request
    response=$(timeout 5 curl -s -w "%{http_code}" http://localhost:3000/ 2>/dev/null)
    http_code=${response: -3}
    
    if [ "$http_code" = "200" ]; then
        echo "✓ Server is responding to HTTP requests"
        exit 0
    else
        echo "✗ Server not responding properly (HTTP $http_code)"
        exit 1
    fi
else
    echo "✗ Server is not listening on port 3000"
    exit 1
fi