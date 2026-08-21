#!/usr/bin/env python3
"""
Create the initial on-chain tracker box for a fresh Basis tracker instance.

The tracker box holds the tracker NFT and commits the (initially empty) note
AVL tree in R5.  It is created before the tracker server starts so the
server's updater can find an existing box to update.

Required environment:
  BASIS_NODE_API_KEY  - node API key
  TRACKER_NFT_ID      - hex token id of the tracker NFT
  TRACKER_PUBKEY      - 33-byte compressed tracker public key (hex)
  TRACKER_SECRET      - 32-byte tracker secret key (hex), passed to the node signer
"""

import json
import os
import sys
import time
import urllib.request
import urllib.error
from typing import Any, Dict, List

NODE_URL = os.environ.get("BASIS_NODE_URL", "http://127.0.0.1:9053").rstrip("/")
API_KEY = os.environ.get("BASIS_NODE_API_KEY", "")

FEE_ERGO_TREE = (
    "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
    "ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108"
    "cdeeac93b1a57304"
)


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
        with urllib.request.urlopen(req, timeout=120) as resp:
            return resp.status, json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode(errors="replace"))


def wait_for_confirmation(tx_id: str, timeout: float = 300.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            node_get(f"/blockchain/transaction/byId/{tx_id}")
            return
        except urllib.error.HTTPError:
            pass
        time.sleep(5)
    raise TimeoutError(f"transaction {tx_id} not confirmed within {timeout:.0f}s")


def address_to_ergo_tree(address: str) -> str:
    return str(node_get(f"/script/addressToTree/{address}")["tree"])


def get_wallet_address() -> str:
    addresses = node_get("/wallet/addresses")
    if not addresses:
        raise RuntimeError("wallet has no addresses")
    return str(addresses[0])


def vlq_encode(value: int) -> bytes:
    if value == 0:
        return b"\x00"
    out = bytearray()
    while value > 0:
        byte = value & 0x7F
        value >>= 7
        if value != 0:
            byte |= 0x80
        out.append(byte)
    return bytes(out)


def build_initial_r5(digest_hex: str) -> str:
    """Build an R5 AVLTree register value matching tracker_box_updater.rs."""
    digest = bytes.fromhex(digest_hex)
    if len(digest) != 33:
        raise ValueError("digest must be 33 bytes (66 hex chars)")
    r5 = bytearray()
    r5.append(0x64)  # AVLTree constant type prefix
    r5.extend(digest)
    r5.append(0x03)  # insert + update allowed
    r5.extend(vlq_encode(32))
    r5.extend(vlq_encode(0))
    return r5.hex()


def main() -> int:
    tracker_nft_id = os.environ.get("TRACKER_NFT_ID", "").strip()
    tracker_pubkey = os.environ.get("TRACKER_PUBKEY", "").strip()
    tracker_secret = os.environ.get("TRACKER_SECRET", "").strip()

    if not tracker_nft_id or not tracker_pubkey or not tracker_secret:
        print("TRACKER_NFT_ID, TRACKER_PUBKEY and TRACKER_SECRET are required", file=sys.stderr)
        return 1
    if len(tracker_pubkey) != 66:
        print(f"TRACKER_PUBKEY must be 66 hex chars, got {len(tracker_pubkey)}", file=sys.stderr)
        return 1

    # Find the box holding the tracker NFT (may be a wallet box or an existing
    # tracker box).  Then collect additional wallet boxes for ERG if needed.
    nft_boxes = node_get(f"/blockchain/box/unspent/byTokenId/{tracker_nft_id}?limit=20")
    tracker_fund_box = None
    for box in nft_boxes:
        if any(
            asset.get("tokenId", "").lower() == tracker_nft_id.lower()
            and asset.get("amount", 0) >= 1
            for asset in box.get("assets", [])
        ):
            tracker_fund_box = box
            break

    if tracker_fund_box is None:
        print(f"tracker NFT {tracker_nft_id[:16]}... not found on-chain", file=sys.stderr)
        return 1

    wallet_boxes = node_get("/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1")
    other_inputs: List[Dict[str, Any]] = []
    for entry in wallet_boxes:
        box = entry.get("box", entry)
        # Skip the tracker fund box itself if it happens to be returned by wallet.
        if box["boxId"] == tracker_fund_box["boxId"]:
            continue
        other_inputs.append(box)

    print(f"Found tracker NFT box: {tracker_fund_box['boxId'][:16]}... value={tracker_fund_box['value']}")

    # Ensure we have enough ERG by adding additional inputs if needed.
    fee = 1_000_000
    tracker_box_value = 10_000_000
    required = tracker_box_value + fee + 1_000_000  # + min change
    total_available = tracker_fund_box["value"] + sum(b["value"] for b in other_inputs)
    selected_other: List[Dict[str, Any]] = []
    if tracker_fund_box["value"] < required:
        running = tracker_fund_box["value"]
        for box in other_inputs:
            selected_other.append(box)
            running += box["value"]
            if running >= required:
                break
        if running < required:
            print(f"insufficient ERG for tracker box: have {total_available}, need {required}", file=sys.stderr)
            return 1

    # Derive tracker P2PK address and ergoTree from the public key.
    tracker_address = node_get(f"/utils/rawToAddress/{tracker_pubkey}")
    if isinstance(tracker_address, dict):
        tracker_address = tracker_address.get("address") or tracker_address.get("value") or ""
    tracker_tree = address_to_ergo_tree(tracker_address)

    wallet_address = get_wallet_address()
    wallet_tree = address_to_ergo_tree(wallet_address)

    current_height = node_get("/info")["fullHeight"]
    fee = 1_000_000
    tracker_box_value = 100_000_000  # 0.1 ERG, enough for storage rent and fee-box splitting
    total_input_value = tracker_fund_box["value"] + sum(b["value"] for b in selected_other)
    change_value = total_input_value - tracker_box_value - fee
    if change_value < 1_000_000:
        raise RuntimeError(f"total input value too small: {total_input_value}")

    # Preserve non-tracker tokens from all inputs in the change output.
    change_assets = []
    seen_tokens = set()
    for box in [tracker_fund_box] + selected_other:
        for a in box.get("assets", []):
            tid = a.get("tokenId", "").lower()
            if tid == tracker_nft_id.lower():
                continue
            if tid in seen_tokens:
                # Merge multiple inputs of the same token.
                for ca in change_assets:
                    if ca["tokenId"].lower() == tid:
                        ca["amount"] += a.get("amount", 0)
                        break
            else:
                seen_tokens.add(tid)
                change_assets.append({"tokenId": a["tokenId"], "amount": a.get("amount", 0)})

    inputs = [{"boxId": tracker_fund_box["boxId"], "extension": {}}]
    inputs_raw = [node_get(f"/utxo/byIdBinary/{tracker_fund_box['boxId']}")["bytes"]]
    for box in selected_other:
        inputs.append({"boxId": box["boxId"], "extension": {}})
        inputs_raw.append(node_get(f"/utxo/byIdBinary/{box['boxId']}")["bytes"])

    outputs: List[Dict[str, Any]] = [
        {
            "value": tracker_box_value,
            "ergoTree": tracker_tree,
            "creationHeight": current_height,
            "assets": [{"tokenId": tracker_nft_id, "amount": 1}],
            "additionalRegisters": {
                "R4": f"07{tracker_pubkey}",
                "R5": build_initial_r5("0" * 66),  # 33-byte zero digest
                "R6": f"0e20{tracker_nft_id}",
            },
        },
        {
            "value": fee,
            "ergoTree": FEE_ERGO_TREE,
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {},
        },
        {
            "value": change_value,
            "ergoTree": wallet_tree,
            "creationHeight": current_height,
            "assets": change_assets,
            "additionalRegisters": {},
        },
    ]

    unsigned_tx = {
        "tx": {"inputs": inputs, "dataInputs": [], "outputs": outputs},
        "inputsRaw": inputs_raw,
        "dataInputsRaw": [],
        "secrets": {"dlog": [tracker_secret]},
    }

    status, body = node_post("/wallet/transaction/sign", unsigned_tx)
    if status != 200:
        print(f"/wallet/transaction/sign failed ({status}): {body}", file=sys.stderr)
        return 1

    signed_tx = body
    status2, broadcast_body = node_post("/transactions", signed_tx)
    if status2 != 200:
        print(f"/transactions broadcast failed ({status2}): {broadcast_body}", file=sys.stderr)
        return 1

    tx_id = broadcast_body if isinstance(broadcast_body, str) else broadcast_body.get("id", "unknown")
    print(f"Tracker box creation tx: {tx_id}")
    wait_for_confirmation(tx_id)
    print("Tracker box confirmed.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:
        print(f"[CREATE TRACKER BOX FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
