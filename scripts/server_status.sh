#!/bin/bash

# Basis Tracker Server Status Script
# Checks if the server is running and shows basic information

# Configuration
PID_FILE="server.pid"
LOG_FILE="server.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Check if PID file exists
if [ ! -f "$PID_FILE" ]; then
    print_warning "PID file not found: $PID_FILE"
    print_status "Server is not running (or was started without this script)"
    exit 1
fi

# Read PID from file
PID=$(cat "$PID_FILE")

# Check if process is running
if ps -p "$PID" > /dev/null 2>&1; then
    print_status "Server is running with PID: $PID"
    
    # Get process information
    PROCESS_INFO=$(ps -p "$PID" -o pid,user,pcpu,pmem,etime,comm --no-headers)
    print_info "Process details:"
    echo "  PID    USER    CPU%   MEM%   ELAPSED   COMMAND"
    echo "  $PROCESS_INFO"
    
    # Check log file
    if [ -f "$LOG_FILE" ]; then
        LOG_SIZE=$(du -h "$LOG_FILE" | cut -f1)
        LOG_LINES=$(wc -l < "$LOG_FILE")
        print_info "Log file: $LOG_FILE ($LOG_SIZE, $LOG_LINES lines)"
        
        # Show last few log entries
        print_info "Recent log entries:"
        tail -5 "$LOG_FILE" | while IFS= read -r line; do
            echo "  $line"
        done
    else
        print_warning "Log file not found: $LOG_FILE"
    fi
    
    print_info "To view full logs: tail -f $LOG_FILE"
    print_info "To stop server: ./stop_server.sh"
else
    print_error "Server with PID $PID is not running"
    print_warning "Removing stale PID file..."
    rm -f "$PID_FILE"
    exit 1
fi