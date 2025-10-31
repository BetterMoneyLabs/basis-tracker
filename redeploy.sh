#!/bin/bash

# Basis Tracker Server Redeployment Script
# Executes the redeployment process: git pull, cargo clean, and restart server

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

# Start redeployment process
print_status "Starting server redeployment..."

# Step 1: Pull latest code from git
print_status "Pulling latest code from git..."
git pull origin master

if [ $? -ne 0 ]; then
    print_error "Git pull failed"
    exit 1
fi

print_status "Git pull completed successfully"

# Step 2: Clean cargo build
print_status "Cleaning cargo build..."
cargo clean

if [ $? -ne 0 ]; then
    print_error "Cargo clean failed"
    exit 1
fi

print_status "Cargo clean completed successfully"

# Step 3: Start the server
print_status "Starting server..."
./run_server.sh

if [ $? -ne 0 ]; then
    print_error "Server startup failed"
    exit 1
fi

print_status "Server redeployment completed successfully"
