#!/usr/bin/env python3
"""
Script to create a test IOU note and attempt redemption
"""

import requests
import json
import time
import binascii

def main():
    # Server URL
    server_url = "http://localhost:3048"
    
    print("Creating test IOU note...")
    
    # Using known test keys from the configuration
    # From the config file: tracker_public_key = "9g7RJLKdrmxgvDg8RzbRx7bNqsdT46vsQx3bYDKuEEVkN5D4DkH"
    # This is a P2PK address, let's use test keys from the test files
    issuer_pub_key_hex = "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7"  # Example test key
    recipient_pub_key_hex = "032ff012ec2a75bc2007aa15c40a0aaf28bbfd98150b3d116b63a3b4c052f41631"  # Another test key
    
    print(f"Issuer public key: {issuer_pub_key_hex}")
    print(f"Recipient public key: {recipient_pub_key_hex}")
    
    # Create a timestamp (current time)
    timestamp = int(time.time())
    
    # For testing, we'll use a mock signature since we don't have the private key
    # In a real scenario, this would be a valid signature from the issuer
    signature = "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102"  # 65-byte mock signature
    
    print(f"Mock signature: {signature}")
    
    # Create the IOU note
    note_data = {
        "recipient_pubkey": recipient_pub_key_hex,
        "amount": 1000,
        "timestamp": timestamp,
        "signature": signature,
        "issuer_pubkey": issuer_pub_key_hex
    }
    
    print("\nCreating IOU note...")
    response = requests.post(f"{server_url}/notes", json=note_data)
    
    print(f"Response status: {response.status_code}")
    print(f"Response body: {response.text}")
    
    if response.status_code in [200, 201]:
        print("\nIOU note created successfully!")
        
        # Now try to get the note to verify it was created
        print(f"\nRetrieving note from issuer {issuer_pub_key_hex} to recipient {recipient_pub_key_hex}...")
        response = requests.get(f"{server_url}/notes/issuer/{issuer_pub_key_hex}/recipient/{recipient_pub_key_hex}")
        print(f"Get note response status: {response.status_code}")
        print(f"Get note response body: {response.text}")
        
        # Now attempt redemption
        print("\nAttempting redemption...")
        redemption_data = {
            "issuer_pubkey": issuer_pub_key_hex,
            "recipient_pubkey": recipient_pub_key_hex,
            "amount": 500,  # Redeeming half of the amount
            "timestamp": timestamp
        }
        
        response = requests.post(f"{server_url}/redeem", json=redemption_data)
        print(f"Redemption response status: {response.status_code}")
        print(f"Redemption response body: {response.text}")
        
    else:
        print("\nFailed to create IOU note.")
        print("This is expected since we used a mock signature.")
        print("Let's try with a proper signature from the Ergo node API...")
        
        # Let's try to use the API to prepare a redemption which should generate a proper signature
        print("\nTrying redemption preparation API which should generate proper signatures...")
        redemption_prep_data = {
            "issuer_pubkey": issuer_pub_key_hex,
            "recipient_pubkey": recipient_pub_key_hex,
            "amount": 500,
            "timestamp": timestamp
        }
        
        response = requests.post(f"{server_url}/redemption/prepare", json=redemption_prep_data)
        print(f"Redemption preparation response status: {response.status_code}")
        print(f"Redemption preparation response body: {response.text}")

if __name__ == "__main__":
    main()