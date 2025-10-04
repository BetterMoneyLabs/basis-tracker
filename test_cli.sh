#!/bin/bash

# Test Basis CLI functionality

echo "=== Testing Basis CLI ==="

# Clean up any existing config
rm -f ~/.basis/cli.toml

# Test 1: Create account
echo "Test 1: Creating account 'alice'"
./target/debug/basis_cli account create alice

# Test 2: Show account info
echo -e "\nTest 2: Showing account info"
./target/debug/basis_cli account info

# Test 3: List accounts
echo -e "\nTest 3: Listing accounts"
./target/debug/basis_cli account list

# Test 4: Server status
echo -e "\nTest 4: Checking server status"
./target/debug/basis_cli status

echo -e "\n=== CLI Test Complete ==="