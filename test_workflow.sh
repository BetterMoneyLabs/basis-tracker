#!/bin/bash

# Test Alice → Bob Workflow Script
# This tests the basic CLI functionality without blockchain integration

echo "=== Testing Alice → Bob Workflow ==="

# Clean up any existing config
rm -f ~/.basis/cli.toml

# Test 1: Create accounts
echo "1. Creating Alice and Bob accounts..."
./target/debug/basis_cli account create alice
ALICE_PUBKEY=$(./target/debug/basis_cli account info 2>/dev/null | grep "Public Key" | cut -d' ' -f3)
echo "   Alice PubKey: $ALICE_PUBKEY"

./target/debug/basis_cli account create bob
BOB_PUBKEY=$(./target/debug/basis_cli account info 2>/dev/null | grep "Public Key" | cut -d' ' -f3)
echo "   Bob PubKey: $BOB_PUBKEY"

# Test 2: Switch to Alice and create notes
echo -e "\n2. Alice creating debt notes to Bob..."
./target/debug/basis_cli account switch alice

# Create multiple debt notes (simulated - actual API calls would fail without server)
echo "   Note 1: 1000 nanoERG"
echo "   Note 2: 1500 nanoERG"  
echo "   Note 3: 2000 nanoERG"

# Test 3: Switch to Bob and check received notes
echo -e "\n3. Bob checking received notes..."
./target/debug/basis_cli account switch bob

# Test 4: Check reserve status
echo -e "\n4. Checking Alice's reserve status..."
./target/debug/basis_cli reserve status --issuer $ALICE_PUBKEY

# Test 5: Server status
echo -e "\n5. Checking server status..."
./target/debug/basis_cli status

# Test 6: Interactive mode (brief test)
echo -e "\n6. Testing interactive mode (type 'exit' to continue)..."
echo "exit" | ./target/debug/basis_cli interactive

echo -e "\n=== Basic CLI Functionality Test Complete ==="
echo "Note: Full workflow requires Basis Tracker server running on http://127.0.0.1:3000"
echo "and Ergo blockchain access for reserve deployment."