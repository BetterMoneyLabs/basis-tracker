#!/usr/bin/env python3
"""
Mint two fresh NFTs from the local Ergo node wallet using AssetIssueRequest.

Uses the Ergo node's /wallet/transaction/send endpoint with an
AssetIssueRequest. The new token id is the first input box id of the
issuance transaction, which we look up after broadcasting.

Requires:
  * Ergo node at 127.0.0.1:9053 (or BASIS_NODE_URL)
  * Unlocked wallet with an API key in BASIS_NODE_API_KEY
"""

import json
import os
import sys
import time
import urllib.request
import urllib.error
from typing import Any

NODE_URL = os.environ.get("BASIS_NODE_URL", "http://127.0.0.1:9053")
API_KEY = os.environ.get("BASIS_NODE_API_KEY", "")
FEE_NANOERG = 1_000_000  # 0.001 ERG fee
NFT_VALUE_NANOERG = 1_000_000  # ERG value attached to the NFT box


def node_get(path: str) -> Any:
    req = urllib.request.Request(NODE_URL + path)
    if API_KEY:
        req.add_header("api_key", API_KEY)
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode())


def node_post(path: str, payload: Any) -> Any:
    req = urllib.request.Request(
        NODE_URL + path,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    if API_KEY:
        req.add_header("api_key", API_KEY)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        raise RuntimeError(f"POST {path} failed ({e.code}): {body[:500]}")


def get_wallet_address() -> str:
    addresses = node_get("/wallet/addresses")
    if not addresses:
        raise RuntimeError("wallet has no addresses")
    return addresses[0]


def issue_nft(name: str, description: str) -> dict:
    """Issue one NFT and return tx_id + the newly minted token id."""
    address = get_wallet_address()
    payload = {
        "requests": [
            {
                "amount": 1,
                "name": name,
                "description": description,
                "decimals": 0,
                "address": address,
                "ergValue": NFT_VALUE_NANOERG,
            }
        ],
        "fee": FEE_NANOERG,
    }

    tx_id = node_post("/wallet/transaction/send", payload)
    if not isinstance(tx_id, str):
        raise RuntimeError(f"unexpected response from /wallet/transaction/send: {tx_id}")

    # Wait for the transaction to appear on-chain so we can read its inputs.
    deadline = time.time() + 120
    while time.time() < deadline:
        try:
            tx = node_get(f"/blockchain/transaction/byId/{tx_id}")
            inputs = tx.get("inputs", [])
            if inputs:
                return {"tx_id": tx_id, "nft_id": inputs[0]["boxId"]}
        except urllib.error.HTTPError:
            pass
        time.sleep(5)

    raise RuntimeError(f"transaction {tx_id} did not confirm within 120s")


def main() -> int:
    if not API_KEY:
        print("BASIS_NODE_API_KEY is not set", file=sys.stderr)
        return 1

    print("Minting tracker NFT...")
    tracker = issue_nft("Basis Tracker NFT", "Tracker NFT for Celaut+USE demo")
    print("Minting dave reserve NFT...")
    reserve = issue_nft("Basis Reserve NFT", "Reserve NFT for user_dave in Celaut+USE demo")

    print("\nTracker NFT:")
    print(f"  export TRACKER_NFT_ID={tracker['nft_id']}")
    print(f"  tx_id={tracker['tx_id']}")
    print("Dave reserve NFT:")
    print(f"  export DAVE_RESERVE_NFT_ID={reserve['nft_id']}")
    print(f"  tx_id={reserve['tx_id']}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:
        print(f"[MINT FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
