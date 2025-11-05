#!/bin/bash

# Basis Tracker Database Cleanup Script
# This script safely removes all database files and server runtime files

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script variables
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/data"
SERVER_LOG="$SCRIPT_DIR/server.log"
SERVER_PID="$SCRIPT_DIR/server.pid"
BACKUP_DIR="$SCRIPT_DIR/data_backup_$(date +%Y%m%d_%H%M%S)"
AUTO_CONFIRM=false

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to display usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -y, --yes          Auto-confirm all prompts"
    echo "  -b, --backup       Create backup before cleaning"
    echo "  -h, --help         Show this help message"
    exit 1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -y|--yes)
            AUTO_CONFIRM=true
            shift
            ;;
        -b|--backup)
            CREATE_BACKUP=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            ;;
    esac
done

# Function to check if server is running
check_server_running() {
    if [ -f "$SERVER_PID" ]; then
        local pid=$(cat "$SERVER_PID" 2>/dev/null)
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        else
            # Remove stale PID file
            rm -f "$SERVER_PID"
        fi
    fi
    return 1
}

# Function to stop server
stop_server() {
    print_info "Checking if server is running..."
    if check_server_running; then
        print_warning "Server is running. Stopping it first..."
        if [ -f "./stop_server.sh" ]; then
            ./stop_server.sh
            # Wait a bit for server to stop
            sleep 3
        else
            local pid=$(cat "$SERVER_PID")
            print_warning "Using kill to stop server (PID: $pid)..."
            kill "$pid"
            sleep 2
            if check_server_running; then
                print_warning "Server still running, forcing shutdown..."
                kill -9 "$pid"
                sleep 1
            fi
        fi
        
        if check_server_running; then
            print_error "Failed to stop server. Please stop it manually and run this script again."
            exit 1
        else
            print_success "Server stopped successfully"
        fi
    else
        print_info "Server is not running"
    fi
}

# Function to create backup
create_backup() {
    if [ "$CREATE_BACKUP" = true ]; then
        print_info "Creating backup in: $BACKUP_DIR"
        mkdir -p "$BACKUP_DIR"
        
        if [ -d "$DATA_DIR" ]; then
            cp -r "$DATA_DIR" "$BACKUP_DIR/" 2>/dev/null || true
        fi
        
        if [ -f "$SERVER_LOG" ]; then
            cp "$SERVER_LOG" "$BACKUP_DIR/" 2>/dev/null || true
        fi
        
        print_success "Backup created at: $BACKUP_DIR"
    fi
}

# Function to clean database
clean_database() {
    print_info "Starting database cleanup..."
    
    # Remove data directory
    if [ -d "$DATA_DIR" ]; then
        print_warning "Removing data directory: $DATA_DIR"
        rm -rf "$DATA_DIR"
        print_success "Data directory removed"
    else
        print_info "Data directory not found (already clean?)"
    fi
    
    # Remove server log file
    if [ -f "$SERVER_LOG" ]; then
        print_warning "Removing server log: $SERVER_LOG"
        rm -f "$SERVER_LOG"
        print_success "Server log removed"
    fi
    
    # Remove server PID file
    if [ -f "$SERVER_PID" ]; then
        print_warning "Removing server PID file: $SERVER_PID"
        rm -f "$SERVER_PID"
        print_success "Server PID file removed"
    fi
    
    # Recreate necessary directories
    print_info "Recreating directory structure..."
    mkdir -p "$DATA_DIR/notes"
    print_success "Directory structure recreated"
}

# Function to confirm action
confirm_action() {
    if [ "$AUTO_CONFIRM" = true ]; then
        return 0
    fi
    
    echo
    print_warning "This will permanently delete all database files and server logs!"
    print_warning "Files to be removed:"
    echo "  - $DATA_DIR/ (entire database)"
    echo "  - $SERVER_LOG (server logs)"
    echo "  - $SERVER_PID (server process ID)"
    echo
    
    if [ "$CREATE_BACKUP" = true ]; then
        print_info "A backup will be created in: $BACKUP_DIR"
    fi
    
    read -p "Are you sure you want to continue? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Cleanup cancelled"
        exit 0
    fi
}

# Main execution
main() {
    echo "=========================================="
    echo "   Basis Tracker Database Cleanup"
    echo "=========================================="
    echo
    
    # Confirm action
    confirm_action
    
    # Stop server if running
    stop_server
    
    # Create backup if requested
    create_backup
    
    # Clean database
    clean_database
    
    echo
    print_success "Database cleanup completed successfully!"
    echo
    print_info "You can now start the server with: ./run_server.sh"
    
    if [ "$CREATE_BACKUP" = true ]; then
        echo
        print_info "Backup available at: $BACKUP_DIR"
    fi
}

# Run main function
main "$@"
