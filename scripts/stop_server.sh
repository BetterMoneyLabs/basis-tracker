#!/bin/bash

# Basis Tracker Server Stop Script
# Gracefully stops the server running in the background

# Configuration
PID_FILE="server.pid"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[STATUS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if PID file exists
if [ ! -f "$PID_FILE" ]; then
    print_warning "PID file not found: $PID_FILE"
    print_status "Server may not be running or was started without this script"
    exit 1
fi

# Read PID from file
PID=$(cat "$PID_FILE")

# Check if process is running
if ps -p "$PID" > /dev/null 2>&1; then
    print_status "Stopping server with PID: $PID"
    
    # Send SIGTERM signal
    kill "$PID"
    
    # Wait for process to terminate
    TIMEOUT=10
    COUNTER=0
    while ps -p "$PID" > /dev/null 2>&1 && [ $COUNTER -lt $TIMEOUT ]; do
        sleep 1
        COUNTER=$((COUNTER + 1))
    done
    
    # Check if process is still running
    if ps -p "$PID" > /dev/null 2>&1; then
        print_warning "Server did not stop gracefully, forcing termination..."
        kill -9 "$PID"
        sleep 1
    fi
    
    # Remove PID file
    rm -f "$PID_FILE"
    
    if ps -p "$PID" > /dev/null 2>&1; then
        print_error "Failed to stop server with PID: $PID"
        exit 1
    else
        print_status "Server stopped successfully"
    fi
else
    print_warning "Server with PID $PID is not running"
    print_status "Removing stale PID file..."
    rm -f "$PID_FILE"
fi