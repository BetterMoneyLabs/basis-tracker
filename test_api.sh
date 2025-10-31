#!/bin/bash

# Test the Basis Tracker HTTP API

BASE_URL="http://localhost:3048"

# Test 1: Health check
echo "=== Test 1: Health Check ==="
curl -s "$BASE_URL/"
echo -e "\n"

# Test 2: Get events
echo "=== Test 2: Get Recent Events ==="
curl -s "$BASE_URL/events" | jq .
echo -e "\n"

# Test 3: Get paginated events
echo "=== Test 3: Get Paginated Events ==="
curl -s "$BASE_URL/events/paginated?page=0&page_size=5" | jq .
echo -e "\n"

# Test 4: Get key status (using demo pubkey)
echo "=== Test 4: Get Key Status ==="
curl -s "$BASE_URL/key-status/010101010101010101010101010101010101010101010101010101010101010101" | jq .
echo -e "\n"

# Test 5: Get all reserves
echo "=== Test 5: Get All Reserves ==="
curl -s "$BASE_URL/reserves" | jq .
echo -e "\n"

# Test 6: Get reserves by issuer
echo "=== Test 6: Get Reserves by Issuer ==="
curl -s "$BASE_URL/reserves/issuer/010101010101010101010101010101010101010101010101010101010101010101" | jq .
echo -e "\n"

# Test 7: Get proof
echo "=== Test 7: Get Proof ==="
curl -s "$BASE_URL/proof?issuer_pubkey=010101010101010101010101010101010101010101010101010101010101010101&recipient_pubkey=020202020202020202020202020202020202020202020202020202020202020202" | jq .
echo -e "\n"

echo "=== API Tests Complete ==="
