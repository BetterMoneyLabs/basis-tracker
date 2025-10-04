#!/bin/bash

# Test script for real Ergo scanner against actual Ergo nodes
# This script tests connectivity and basic functionality

echo "Testing real Ergo scanner against Ergo nodes..."

# Test against mainnet node
echo ""
echo "=== Testing against mainnet node: 213.239.193.208:9053 ==="
cargo test -p basis_store real_scanner_integration_tests::tests::test_connectivity_only -- --nocapture --ignored

# Test against testnet node
echo ""
echo "=== Testing against testnet node: 213.239.193.208:9052 ==="
cargo test -p basis_store real_scanner_integration_tests::tests::test_real_scanner_against_testnet_node -- --nocapture --ignored

# Run all real scanner tests (may fail due to network issues)
echo ""
echo "=== Running all real scanner tests (may fail due to network) ==="
cargo test -p basis_store real_scanner_integration_tests -- --nocapture --ignored

echo ""
echo "=== Real scanner tests completed ==="
echo "Note: Tests marked with 'ignored' require network connectivity"
echo "To run these tests manually, use: cargo test -- --ignored"