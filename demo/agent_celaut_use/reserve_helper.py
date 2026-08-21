#!/usr/bin/env python3
"""
Shared helpers for creating a USE-token-backed Basis reserve directly via the
Ergo node's wallet API.

This is used both by orchestrator.py (reserve creation during the demo) and by
a run.sh preflight step that locks the reserve NFT before the tracker server
starts.  Locking the NFT early prevents the tracker's box updater from
accidentally selecting the unspent NFT box as a fee input.
"""

import json
import os
import sys
import time
import urllib.request
import urllib.error
from typing import Any, Dict, List, Optional, Tuple

NODE_URL = os.environ.get("BASIS_NODE_URL", "http://127.0.0.1:9053")
API_KEY = os.environ.get("BASIS_NODE_API_KEY", "")
TRACKER_SECRET = os.environ.get("BASIS_TRACKER_SECRET", "")

# Standard Ergo fee script ergoTree (matches basis_server transaction builder).
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
        body = e.read().decode(errors="replace")
        try:
            return e.code, json.loads(body)
        except json.JSONDecodeError:
            return e.code, {"raw": body}


def wait_for_node_confirmation(tx_id: str, timeout: float = 600.0) -> None:
    """Wait until the transaction is confirmed on the Ergo node."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            node_get(f"/blockchain/transaction/byId/{tx_id}")
            return
        except urllib.error.HTTPError:
            pass
        time.sleep(5)
    raise TimeoutError(f"transaction {tx_id} not confirmed within {timeout:.0f}s")


def get_wallet_address() -> str:
    addresses = node_get("/wallet/addresses")
    if not addresses:
        raise RuntimeError("node wallet has no addresses")
    return str(addresses[0])


def address_to_ergo_tree(address: str) -> str:
    return str(node_get(f"/script/addressToTree/{address}")["tree"])


def find_reserve_box(nft_id: str, reserve_ergo_tree: str) -> Optional[Dict[str, Any]]:
    """Return the unspent reserve box for `nft_id` if it already exists."""
    boxes = node_get(f"/blockchain/box/unspent/byTokenId/{nft_id}?limit=20")
    for box in boxes:
        if box.get("ergoTree", "").lower() == reserve_ergo_tree.lower():
            return box
    return None


def asset_amount(box: Dict[str, Any], token_id: str) -> int:
    for asset in box.get("assets", []):
        if asset.get("tokenId", "").lower() == token_id.lower():
            return int(asset.get("amount", 0))
    return 0


def _contains_token(box: Dict[str, Any], token_id: str) -> bool:
    for asset in box.get("assets", []):
        if asset.get("tokenId", "").lower() == token_id.lower():
            return True
    return False


def find_boxes_for_reserve_creation(
    nft_id: str,
    reserve_token_id: str,
    tracker_nft_id: str,
    required_erg: int,
    required_token: int,
) -> Tuple[List[str], List[Dict[str, Any]], int, int]:
    """
    Find input boxes that together hold the reserve NFT, enough ERG and enough
    reserve tokens.  Returns (input_box_ids, input_boxes_json, total_erg,
    total_token).

    Boxes that hold the tracker NFT are explicitly excluded so the reserve
    creation transaction does not accidentally spend the on-chain tracker box.
    """
    # First, locate the box that currently holds the reserve NFT.
    nft_boxes = node_get(f"/blockchain/box/unspent/byTokenId/{nft_id}?limit=20")
    reserve_fund_box = None
    for box in nft_boxes:
        # Any unspent box holding the NFT is the fund box; we will move it into
        # the reserve contract.
        reserve_fund_box = box
        break
    if reserve_fund_box is None:
        raise RuntimeError(f"reserve NFT {nft_id[:16]}... not found in any unspent box")

    inputs: List[Dict[str, Any]] = [reserve_fund_box]
    total_erg = reserve_fund_box.get("value", 0)
    total_token = asset_amount(reserve_fund_box, reserve_token_id)
    input_ids = {reserve_fund_box["boxId"]}

    # Collect additional wallet boxes until we have enough ERG and tokens.
    wallet_boxes = node_get("/wallet/boxes/unspent?minConfirmations=1")
    for entry in wallet_boxes:
        box = entry.get("box", entry)
        box_id = box["boxId"]
        if box_id in input_ids:
            continue
        if tracker_nft_id and _contains_token(box, tracker_nft_id):
            # Never spend the tracker box; the server manages it independently.
            continue
        box_erg = box.get("value", 0)
        box_token = asset_amount(box, reserve_token_id)

        need_erg = total_erg < required_erg
        need_token = total_token < required_token
        has_token = box_token > 0

        # Prefer boxes that help cover a still-missing requirement.  Plain ERG
        # boxes are picked when we still need ERG; token boxes are picked when
        # we still need tokens (they also contribute their ERG).
        useful = False
        if need_erg and not box.get("assets"):
            useful = True
        if need_token and has_token:
            useful = True
        if not useful:
            continue

        inputs.append(box)
        input_ids.add(box_id)
        total_erg += box_erg
        total_token += box_token

        if total_erg >= required_erg and total_token >= required_token:
            break

    if total_erg < required_erg:
        raise RuntimeError(
            f"insufficient ERG to create reserve: have {total_erg}, need {required_erg}"
        )
    if total_token < required_token:
        raise RuntimeError(
            f"insufficient {reserve_token_id[:16]}... tokens to create reserve: "
            f"have {total_token}, need {required_token}"
        )

    return [b["boxId"] for b in inputs], inputs, total_erg, total_token


def submit_reserve_transaction_direct(
    owner_pubkey: str,
    nft_id: str,
    reserve_token_id: str,
    tracker_nft_id: str,
    token_amount: int,
    erg_amount: int,
) -> str:
    """
    Build, sign and broadcast a token reserve creation tx directly via the Ergo
    node's /wallet/transaction/sign + /transactions endpoints.
    """
    if not TRACKER_SECRET:
        raise RuntimeError(
            "BASIS_TRACKER_SECRET is required for direct reserve creation signing"
        )

    # Token reserve contract P2S from the server config example.  This is the
    # same address run.sh writes into the demo config.
    token_reserve_p2s = (
        "96HrjMftJd4NbjzufhMHXyZqzaUbdc5zUqtSySUnyEZoHogM9TD9RsRNkhq83tGaWTH4ZiFeDKzAdHkQyi1SWwMdkKtDaDhobhsrb5tjEDZkYhhF5aPGf3b4fUo3W23tec9N7GyzA6buyhRYzf3DqVWRsJGzCEdYeDfcMYKXon3wMxMFgQPX4FUbCAGe32dCd1aiFwQwx4Rnjy19G6qPaACdZvfSNFLgM1ur3U6HSSMX22g5o8MbbxzNeKScWFedW76z1zCmC38BrgiSv1qC7395yi9y4dDy2y26Tgrc2MPxvned5j1F3cTTTY9HVPYcS9U4vQocDRHufEkMG4dyu7eQqiHbqa6962B1jKQUMofyNV2mehQDrTzfzT5yHPsTeAGMTbDHeDECsmmJ2ideonha9VBuEP6fivixWjej43spbMZDcy3SCNa5gTrLuhh8gN4j8CbMncTXYghrxNdRqPbiUfJy8rMpcdeRaDnbodibGAyJqzzh95ZC6FHPwL7UQZbFbc472WSPtPRXU9g7QpZasdYtZb4GrHqvUJASsLwgYsRevJ4QZ24nrcUXv5ttziaVsGfeCW35viGB5zc3Sxk8e37smwQzWPfAGvsLfgy98dJfmNFyQUCNUyGX5qq4uJrKNaRamouL4S1VNFcgVaJTdGDTT7ykcy6qLmaFg6tUiAsLLC2p4GGB83fqsw13hfmxCxMGuFQUFu6aNc9W31wMMJ2beCN7Xb7cUkffzEXZnjiqFKMicTHsmvLz2mZuAgiKykrxqNTMqmTd2DjGsuYmQNtLXqS4j5d9qXnSVxmeVGp3cqcC7NYF9ujvLT38yqfnk5tZ5X9gnMXgkkKk8MfuUgY5a2ccSgbetkj7yNk1ciHK1QYGrvydDPWB2kJEQb9yAb72Uf1jZXVH1kSAr7vv1BBRQWveJcsjD6GQVKXJrLTGLrR5XV1uYK7uQSR4XYEi5yJsjk4rPMCDtt2FDsrUk7bo9suFWq2sqZxQZZWEgoKkwog6DtvyX4cyMpT45eeqjctBTjnAnJRf8CgBuyEhfU8RMfG7dPNVk9EWS3JbGmwow4R95Ae5P"
    )
    reserve_ergo_tree = address_to_ergo_tree(token_reserve_p2s)

    fee = 12_000_000
    required_erg = erg_amount + fee
    input_box_ids, input_boxes, total_erg, total_token = find_boxes_for_reserve_creation(
        nft_id, reserve_token_id, tracker_nft_id, required_erg, token_amount
    )

    change_address = get_wallet_address()
    change_tree = address_to_ergo_tree(change_address)
    current_height = node_get("/info")["fullHeight"]

    # Build inputs with empty extensions and raw binary for the node signer.
    inputs = [{"boxId": box_id, "extension": {}} for box_id in input_box_ids]
    inputs_raw = [
        node_get(f"/utxo/byIdBinary/{box_id}")["bytes"] for box_id in input_box_ids
    ]

    outputs: List[Dict[str, Any]] = [
        {
            "value": erg_amount,
            "ergoTree": reserve_ergo_tree,
            "creationHeight": current_height,
            "assets": [
                {"tokenId": nft_id, "amount": 1},
                {"tokenId": reserve_token_id, "amount": token_amount},
            ],
            "additionalRegisters": {
                "R4": f"07{owner_pubkey}",
                "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000",
                "R6": f"0e20{tracker_nft_id}",
                "R7": "05000000000000000000",
            },
        },
        {
            "value": fee,
            "ergoTree": FEE_ERGO_TREE,
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {},
        },
    ]

    change_amount = total_erg - required_erg
    if change_amount > 0:
        outputs.append({
            "value": change_amount,
            "ergoTree": change_tree,
            "creationHeight": current_height,
            "assets": [],
            "additionalRegisters": {},
        })

    # Any reserve tokens not locked in the reserve go back to the wallet as
    # change.  This keeps the tx balanced when an input box held more tokens
    # than required.
    token_change = total_token - token_amount
    if token_change > 0:
        if outputs[-1].get("ergoTree") == change_tree:
            outputs[-1]["assets"].append(
                {"tokenId": reserve_token_id, "amount": token_change}
            )
        else:
            outputs.append({
                "value": 1_000_000,
                "ergoTree": change_tree,
                "creationHeight": current_height,
                "assets": [{"tokenId": reserve_token_id, "amount": token_change}],
                "additionalRegisters": {},
            })

    unsigned_tx = {
        "tx": {
            "inputs": inputs,
            "dataInputs": [],
            "outputs": outputs,
        },
        "inputsRaw": inputs_raw,
        "dataInputsRaw": [],
        "secrets": {"dlog": [TRACKER_SECRET]},
    }

    status, body = node_post("/wallet/transaction/sign", unsigned_tx)
    if status != 200:
        raise RuntimeError(f"/wallet/transaction/sign failed ({status}): {body}")
    signed_tx = body

    status2, broadcast_body = node_post("/transactions", signed_tx)
    if status2 != 200:
        raise RuntimeError(f"/transactions broadcast failed ({status2}): {broadcast_body}")
    tx_id = broadcast_body if isinstance(broadcast_body, str) else broadcast_body.get("id", "unknown")
    return str(tx_id)


def create_use_reserve(
    owner_pubkey: str,
    nft_id: str,
    reserve_token_id: str,
    tracker_nft_id: str,
    token_amount: int,
    erg_amount: int = 3_000_000,
) -> Dict[str, Any]:
    """
    Create a USE-token-backed reserve via basis-token.es if one does not exist.
    Returns the on-chain reserve box (existing or newly created).
    """
    token_reserve_p2s = (
        "96HrjMftJd4NbjzufhMHXyZqzaUbdc5zUqtSySUnyEZoHogM9TD9RsRNkhq83tGaWTH4ZiFeDKzAdHkQyi1SWwMdkKtDaDhobhsrb5tjEDZkYhhF5aPGf3b4fUo3W23tec9N7GyzA6buyhRYzf3DqVWRsJGzCEdYeDfcMYKXon3wMxMFgQPX4FUbCAGe32dCd1aiFwQwx4Rnjy19G6qPaACdZvfSNFLgM1ur3U6HSSMX22g5o8MbbxzNeKScWFedW76z1zCmC38BrgiSv1qC7395yi9y4dDy2y26Tgrc2MPxvned5j1F3cTTTY9HVPYcS9U4vQocDRHufEkMG4dyu7eQqiHbqa6962B1jKQUMofyNV2mehQDrTzfzT5yHPsTeAGMTbDHeDECsmmJ2ideonha9VBuEP6fivixWjej43spbMZDcy3SCNa5gTrLuhh8gN4j8CbMncTXYghrxNdRqPbiUfJy8rMpcdeRaDnbodibGAyJqzzh95ZC6FHPwL7UQZbFbc472WSPtPRXU9g7QpZasdYtZb4GrHqvUJASsLwgYsRevJ4QZ24nrcUXv5ttziaVsGfeCW35viGB5zc3Sxk8e37smwQzWPfAGvsLfgy98dJfmNFyQUCNUyGX5qq4uJrKNaRamouL4S1VNFcgVaJTdGDTT7ykcy6qLmaFg6tUiAsLLC2p4GGB83fqsw13hfmxCxMGuFQUFu6aNc9W31wMMJ2beCN7Xb7cUkffzEXZnjiqFKMicTHsmvLz2mZuAgiKykrxqNTMqmTd2DjGsuYmQNtLXqS4j5d9qXnSVxmeVGp3cqcC7NYF9ujvLT38yqfnk5tZ5X9gnMXgkkKk8MfuUgY5a2ccSgbetkj7yNk1ciHK1QYGrvydDPWB2kJEQb9yAb72Uf1jZXVH1kSAr7vv1BBRQWveJcsjD6GQVKXJrLTGLrR5XV1uYK7uQSR4XYEi5yJsjk4rPMCDtt2FDsrUk7bo9suFWq2sqZxQZZWEgoKkwog6DtvyX4cyMpT45eeqjctBTjnAnJRf8CgBuyEhfU8RMfG7dPNVk9EWS3JbGmwow4R95Ae5P"
    )
    reserve_ergo_tree = address_to_ergo_tree(token_reserve_p2s)

    existing = find_reserve_box(nft_id, reserve_ergo_tree)
    if existing is not None:
        return existing

    tx_id = submit_reserve_transaction_direct(
        owner_pubkey,
        nft_id,
        reserve_token_id,
        tracker_nft_id,
        token_amount,
        erg_amount,
    )
    wait_for_node_confirmation(tx_id)
    return find_reserve_box(nft_id, reserve_ergo_tree)


def main() -> int:
    """CLI entry point for run.sh preflight reserve creation."""
    owner_pubkey = os.environ.get("DAVE_PUBKEY", "").strip()
    nft_id = os.environ.get("DAVE_RESERVE_NFT_ID", "").strip()
    reserve_token_id = os.environ.get("USE_TOKEN_ID", "").strip()
    tracker_nft_id = os.environ.get("TRACKER_NFT_ID", "").strip()

    if not all([owner_pubkey, nft_id, reserve_token_id, tracker_nft_id]):
        print(
            "DAVE_PUBKEY, DAVE_RESERVE_NFT_ID, USE_TOKEN_ID and TRACKER_NFT_ID "
            "are all required",
            file=sys.stderr,
        )
        return 1

    token_amount = int(os.environ.get("DAVE_RESERVE_AMOUNT", "500"))
    erg_amount = int(os.environ.get("DAVE_RESERVE_ERG_AMOUNT", "3000000"))

    print(f"[PREFLIGHT] Ensuring USE reserve exists for NFT {nft_id[:16]}...")
    box = create_use_reserve(
        owner_pubkey,
        nft_id,
        reserve_token_id,
        tracker_nft_id,
        token_amount,
        erg_amount,
    )
    collateral = asset_amount(box, reserve_token_id)
    print(f"[PREFLIGHT] Reserve box: {box['boxId'][:16]}... collateral={collateral}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:
        print(f"[RESERVE PREFLIGHT FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
