#!/usr/bin/env python3

import subprocess
import time
import requests
import json

def test_reserve_api():
    print("Testing Basis Reserve API endpoint...")
    
    # Start the server in background
    server_process = subprocess.Popen(
        ["cargo", "run", "-p", "basis_server"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    
    # Wait for server to start
    time.sleep(3)
    
    try:
        # Test the API endpoint
        test_pubkey = "010101010101010101010101010101010101010101010101010101010101010101"
        url = f"http://localhost:3000/reserves/issuer/{test_pubkey}"
        
        print(f"Making request to: {url}")
        
        response = requests.get(url, timeout=5)
        
        print(f"Status Code: {response.status_code}")
        print("Response:")
        print(json.dumps(response.json(), indent=2))
        
        # Verify the response structure
        data = response.json()
        assert data["success"] == True
        assert "data" in data
        assert isinstance(data["data"], list)
        
        if data["data"]:
            reserve = data["data"][0]
            assert "box_id" in reserve
            assert "owner_pubkey" in reserve
            assert "collateral_amount" in reserve
            assert "total_debt" in reserve
            assert reserve["owner_pubkey"] == test_pubkey
        
        print("✅ API test passed!")
        
    except Exception as e:
        print(f"❌ API test failed: {e}")
        
    finally:
        # Stop the server
        server_process.terminate()
        server_process.wait()

if __name__ == "__main__":
    test_reserve_api()