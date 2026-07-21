#!/bin/bash

# Basis Tracker Server Startup Script
# Runs the server in the background with nohup and logs to a file

# Configuration
SERVER_BINARY="target/release/basis_server"
LOG_FILE="server.log"
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

# Check if server binary exists
if [ ! -f "$SERVER_BINARY" ]; then
    print_error "Server binary not found: $SERVER_BINARY"
    print_status "Building server..."
    cargo build -p basis_server --release
    
    if [ $? -ne 0 ]; then
        print_error "Failed to build server"
        exit 1
    fi
fi

# Check if server is already running
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if ps -p "$PID" > /dev/null 2>&1; then
        print_warning "Server is already running with PID: $PID"
        echo "To stop it, run: kill $PID"
        exit 1
    else
        print_warning "Stale PID file found, removing..."
        rm -f "$PID_FILE"
    fi
fi

# Start the server
print_status "Starting Basis Tracker Server..."
print_status "Log file: $LOG_FILE"
print_status "PID file: $PID_FILE"

# Start server with nohup and redirect output to log file
nohup "$SERVER_BINARY" > "$LOG_FILE" 2>&1 &
SERVER_PID=$!

# Save PID to file
echo "$SERVER_PID" > "$PID_FILE"

# Wait a moment for server to start
sleep 2

# Check if server is running
if ps -p "$SERVER_PID" > /dev/null 2>&1; then
    print_status "Server started successfully with PID: $SERVER_PID"
    print_status "Server is running in the background"
    print_status "To view logs: tail -f $LOG_FILE"
    print_status "To stop server: kill $SERVER_PID"
    print_status "Or use: ./stop_server.sh"
else
    print_error "Server failed to start"
    print_status "Check the log file for details: $LOG_FILE"
    rm -f "$PID_FILE"
    exit 1
fi