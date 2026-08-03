#!/usr/bin/env python3
"""
Integration test: multiple local-sign redemptions against one Basis reserve.

Runs end-to-end against a real Ergo node (default http://127.0.0.1:9053) and a
Basis tracker server (default http://127.0.0.1:3048):

  1. Build basis_cli
  2. Import the issuer private key into a throw-away CLI config
  3. Issue a fresh reserve NFT (or reuse RESERVE_NFT_ID) to the wallet
  4. Create a 0.3 ERG reserve via the tracker payload + wallet /payment/send
  5. Create a 0.2 ERG IOU note and submit it to the tracker
  6. Wait for the tracker box to commit the note on-chain
  7. Create a token-free fee box in the wallet
  8. Redeem 0.1 ERG twice with `basis_cli transaction generate-redemption --local-sign`
  9. After each redemption, advance the tracker state via POST /redeem/complete
  10. Verify the note is fully redeemed

Required environment:
  ISSUER_PRIVATE_KEY  - 32-byte hex issuer secret (the CLI account that signs redemptions)

Optional environment:
  NODE_URL            - Ergo node URL (default http://127.0.0.1:9053)
  API_KEY             - Ergo node api_key (default hello)
  TRACKER_URL         - Basis tracker server URL (default http://127.0.0.1:3048)
  WALLET_ADDRESS      - Wallet address used for fees/NFT; if omitted, first /wallet/addresses is used
  RECIPIENT_PUBKEY    - Recipient public key; if omitted, derived from WALLET_ADDRESS via /utils/addressToRaw
  RESERVE_NFT_ID      - Existing reserve NFT to reuse; if omitted, a fresh NFT is issued
  RESERVE_AMOUNT      - Reserve collateral in nanoERG (default 300000000 = 0.3 ERG)
  NOTE_AMOUNT         - Note total debt in nanoERG (default 200000000 = 0.2 ERG)
  REDEEM_AMOUNT       - Each redemption amount in nanoERG (default 100000000 = 0.1 ERG)
  FEE_BOX_AMOUNT      - Size of token-free fee box in nanoERG (default 50000000 = 0.05 ERG)
  CLI_BIN             - Path to basis_cli binary (default ./target/debug/basis_cli)
  WAIT_TIMEOUT        - Seconds to wait for confirmations (default 300)

NOTE: The current CLI local-sign path derives P2PK addresses with mainnet prefix and
fetches the recipient/fee-payer secrets from the node wallet. Run this test against a
mainnet (or equivalent) node wallet that contains the recipient address.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

NODE_URL = os.environ.get("NODE_URL", "http://127.0.0.1:9053").rstrip("/")
API_KEY = os.environ.get("API_KEY", "hello")
TRACKER_URL = os.environ.get("TRACKER_URL", "http://127.0.0.1:3048").rstrip("/")
ISSUER_PRIVATE_KEY = os.environ.get("ISSUER_PRIVATE_KEY")
WALLET_ADDRESS = os.environ.get("WALLET_ADDRESS")
RECIPIENT_PUBKEY = os.environ.get("RECIPIENT_PUBKEY")
RESERVE_NFT_ID = os.environ.get("RESERVE_NFT_ID")
RESERVE_AMOUNT = int(os.environ.get("RESERVE_AMOUNT", "300000000"))
NOTE_AMOUNT = int(os.environ.get("NOTE_AMOUNT", "200000000"))
REDEEM_AMOUNT = int(os.environ.get("REDEEM_AMOUNT", "100000000"))
FEE_BOX_AMOUNT = int(os.environ.get("FEE_BOX_AMOUNT", "50000000"))
CLI_BIN = os.environ.get("CLI_BIN", "./target/debug/basis_cli")
WAIT_TIMEOUT = int(os.environ.get("WAIT_TIMEOUT", "300"))


def fail(msg: str) -> None:
    print(f"\n❌ {msg}", file=sys.stderr)
    sys.exit(1)


def http_request(method: str, url: str, body=None, headers=None):
    h = dict(headers or {})
    if body is not None and "Content-Type" not in h:
        h["Content-Type"] = "application/json"
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, method=method, data=data, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.status, resp.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def node_api(method: str, path: str, body=None):
    status, text = http_request(
        method, f"{NODE_URL}{path}", body, headers={"api_key": API_KEY}
    )
    if status >= 400:
        fail(f"Node API {method} {path} failed ({status}): {text}")
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text


def tracker_api(method: str, path: str, body=None):
    status, text = http_request(method, f"{TRACKER_URL}{path}", body)
    if status >= 400:
        fail(f"Tracker API {method} {path} failed ({status}): {text}")
    resp = json.loads(text)
    if not resp.get("success"):
        fail(f"Tracker API {method} {path} error: {resp.get('error')}")
    return resp.get("data")


def wait_for_tx(txid: str, label: str = "transaction"):
    """Wait for a tx to leave the mempool and be known to the wallet/node.

    The local node used for this test does not have extra blockchain indexing
    enabled, so /blockchain/transaction/byId/{txid} returns HTTP 500.  We
    instead poll the unconfirmed pool and then the wallet's own transaction
    index.
    """
    print(f"⏳ Waiting for {label} {txid} to confirm...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        # HEAD /transactions/unconfirmed/{txId} returns 200 while the tx is in mempool
        in_pool_status, _ = http_request(
            "HEAD",
            f"{NODE_URL}/transactions/unconfirmed/{txid}",
        )
        if in_pool_status == 200:
            time.sleep(5)
            continue

        # Once it leaves the mempool, ask the wallet transaction index
        tx_status, tx_text = http_request(
            "GET",
            f"{NODE_URL}/wallet/transactionById?id={txid}",
            headers={"api_key": API_KEY},
        )
        if tx_status == 200:
            parsed = json.loads(tx_text)
            tx = parsed[0] if isinstance(parsed, list) else parsed
            print(f"✅ {label} confirmed: {txid}")
            return tx

        time.sleep(5)
    fail(f"Timed out waiting for {label} {txid}")


def wait_for_reserve_tracked(issuer: str, amount: int):
    print("⏳ Waiting for reserve to be tracked by the server...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        try:
            data = tracker_api("GET", f"/reserves/issuer/{issuer}")
            for r in data or []:
                if r.get("collateral_amount") == amount:
                    print("✅ Reserve tracked")
                    return r
            print("   reserve not yet tracked, retrying...")
        except SystemExit:
            raise
        except Exception as e:
            print(f"   reserve lookup error: {e}")
        time.sleep(5)
    fail("Timed out waiting for reserve to be tracked")


def wait_for_note_confirmed(issuer: str, recipient: str):
    print("⏳ Waiting for tracker to commit the note on-chain...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        data = tracker_api(
            "POST",
            "/notes/state",
            {"issuer_pubkey": issuer, "recipient_pubkey": recipient},
        )
        status = data.get("status")
        redeemable = data.get("redeemable")
        print(f"   note state: status={status}, redeemable={redeemable}")
        if status == "confirmed" and redeemable:
            print("✅ Note confirmed and redeemable")
            return data
        time.sleep(5)
    fail("Timed out waiting for note confirmation")


def run_cli(config_path: str, args: list[str]) -> str:
    cmd = [CLI_BIN, "--config", config_path, "--server-url", TRACKER_URL] + args
    print(f"\n$ {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True)
    print(result.stdout, end="")
    if result.stderr:
        print(result.stderr, end="", file=sys.stderr)
    if result.returncode != 0:
        fail(f"basis_cli command failed: {' '.join(args)}")
    return result.stdout + result.stderr


def get_issuer_pubkey(config_path: str) -> str:
    run_cli(config_path, ["account", "import", "issuer", ISSUER_PRIVATE_KEY])
    run_cli(config_path, ["account", "switch", "issuer"])
    out = run_cli(config_path, ["account", "info"])
    m = re.search(r"Public Key:\s*([0-9a-f]{66})", out)
    if not m:
        fail(f"Could not parse issuer public key from account info:\n{out}")
    return m.group(1)


def get_tracker_nft_id() -> str:
    """Return the token id of the first asset in the current on-chain tracker box."""
    tracker_info = tracker_api("GET", "/tracker/latest-box-id")
    if not tracker_info or not tracker_info.get("tracker_box_id"):
        fail("Could not get latest tracker box id from tracker server")
    tracker_box_id = tracker_info["tracker_box_id"]
    box = node_api("GET", f"/utxo/byId/{tracker_box_id}")
    assets = box.get("assets", [])
    if not assets:
        fail(f"Tracker box {tracker_box_id} has no assets")
    return assets[0]["tokenId"]


def select_spendable_inputs(required: int, tracker_nft_id: str) -> tuple[list[str], int]:
    """
    Select wallet boxes covering at least `required` nanoERG, excluding any
    box that holds the tracker NFT. Returns (inputsRaw list, total value).
    """
    wallet_boxes = node_api("GET", "/wallet/boxes/unspent")
    candidates = []
    for entry in wallet_boxes:
        box = entry.get("box", entry)
        box_id = box["boxId"]
        assets = box.get("assets", [])
        # Skip the tracker box itself and any box holding the tracker NFT.
        if any(a.get("tokenId") == tracker_nft_id and a.get("amount", 0) >= 1 for a in assets):
            print(f"   Excluding box {box_id[:16]}... (contains tracker NFT)")
            continue
        candidates.append(box)

    # Prefer token-free boxes.
    token_free = [b for b in candidates if not b.get("assets")]
    token_free.sort(key=lambda b: b["value"])
    selected = []
    total = 0
    for box in token_free:
        selected.append(box)
        total += box["value"]
        if total >= required:
            return _fetch_inputs_raw(selected), total

    # Fall back to token-bearing boxes.
    token_boxes = [b for b in candidates if b.get("assets")]
    token_boxes.sort(key=lambda b: b["value"])
    for box in token_boxes:
        selected.append(box)
        total += box["value"]
        if total >= required:
            return _fetch_inputs_raw(selected), total

    fail(
        f"Could not select wallet inputs covering {required} nanoERG without spending "
        f"the tracker NFT box. Available non-tracker value: {total} nanoERG"
    )


def _fetch_inputs_raw(boxes: list[dict]) -> list[str]:
    raw = []
    for box in boxes:
        box_id = box["boxId"]
        binary = node_api("GET", f"/utxo/byIdBinary/{box_id}")
        if isinstance(binary, dict):
            binary = binary.get("bytes") or binary
        raw.append(binary)
    return raw


def issue_reserve_nft(wallet_address: str) -> str:
    print("🎫 Issuing a fresh reserve NFT...")
    tracker_nft_id = get_tracker_nft_id()
    required = RESERVE_AMOUNT + FEE_BOX_AMOUNT + 1_000_000 + 1_000_000  # output + fee + margin
    inputs_raw, _ = select_spendable_inputs(required, tracker_nft_id)
    body = {
        "fee": 1_000_000,
        "requests": [
            {
                "address": wallet_address,
                "ergValue": RESERVE_AMOUNT + FEE_BOX_AMOUNT + 1_000_000,
                "amount": 1,
                "name": "BasisReserve",
                "description": "Basis reserve NFT",
                "decimals": 0,
            }
        ],
        "inputsRaw": inputs_raw,
    }
    txid = node_api("POST", "/wallet/transaction/send", body)
    if isinstance(txid, dict):
        txid = txid.get("id") or txid
    print(f"   NFT issuance tx: {txid}")
    tx = wait_for_tx(txid, "NFT issuance")
    for out in tx.get("outputs", []):
        if out.get("address") == wallet_address and out.get("assets"):
            return out["assets"][0]["tokenId"]
    fail("Could not find issued NFT token ID in transaction outputs")


def create_reserve(nft_id: str, issuer_pubkey: str, amount: int) -> str:
    print("🏦 Creating reserve payload...")
    payload = tracker_api(
        "POST",
        "/reserves/create",
        {"nft_id": nft_id, "owner_pubkey": issuer_pubkey, "erg_amount": amount},
    )
    req = payload["requests"][0]
    payment = [
        {
            "address": req["address"],
            "value": req["value"],
            "assets": [
                {"tokenId": a["token_id"], "amount": a["amount"]} for a in req["assets"]
            ],
            "registers": req["registers"],
        }
    ]
    tracker_nft_id = get_tracker_nft_id()
    required = req["value"] + 1_000_000 + 1_000_000  # reserve value + fee + margin
    inputs_raw, _ = select_spendable_inputs(required, tracker_nft_id)
    body = {
        "fee": 1_000_000,
        "requests": payment,
        "inputsRaw": inputs_raw,
    }
    txid = node_api("POST", "/wallet/transaction/send", body)
    print(f"   Reserve creation tx: {txid}")
    tx = wait_for_tx(txid, "reserve creation")
    reserve_addr = req["address"]
    for out in tx.get("outputs", []):
        if out.get("address") == reserve_addr:
            return out["boxId"]
    fail("Could not find reserve box in transaction outputs")


def create_note(config_path: str, recipient_pubkey: str, amount: int):
    print("📝 Creating note on tracker...")
    run_cli(
        config_path,
        ["note", "create", "--recipient", recipient_pubkey, "--amount", str(amount)],
    )


def create_fee_box(wallet_address: str):
    print("💰 Creating token-free fee box...")
    tracker_nft_id = get_tracker_nft_id()
    required = FEE_BOX_AMOUNT + 1_000_000 + 1_000_000  # output + fee + margin
    inputs_raw, _ = select_spendable_inputs(required, tracker_nft_id)
    body = {
        "fee": 1_000_000,
        "requests": [{"address": wallet_address, "value": FEE_BOX_AMOUNT}],
        "inputsRaw": inputs_raw,
    }
    txid = node_api("POST", "/wallet/transaction/send", body)
    print(f"   Fee box tx: {txid}")
    return wait_for_tx(txid, "fee box")


def run_local_sign_redemption(config_path: str, issuer: str, recipient: str, amount: int) -> str:
    print(f"🔄 Running local-sign redemption for {amount} nanoERG...")
    out = run_cli(
        config_path,
        [
            "transaction",
            "generate-redemption",
            "--issuer-pubkey",
            issuer,
            "--recipient-pubkey",
            recipient,
            "--amount",
            str(amount),
            "--local-sign",
        ],
    )
    m = re.search(r"Transaction ID:\s*([0-9a-f]{64})", out)
    if not m:
        fail(f"Could not parse redemption tx id from CLI output:\n{out}")
    txid = m.group(1)
    print(f"   Redemption tx: {txid}")
    wait_for_tx(txid, "redemption")
    return txid


def complete_redemption(issuer: str, recipient: str, redeemed: int, already: int):
    print(f"☑️  Advancing tracker state: redeemed={redeemed}, cumulative={already}")
    tracker_api(
        "POST",
        "/redeem/complete",
        {
            "redemption_id": str(int(time.time() * 1000)),
            "issuer_pubkey": issuer,
            "recipient_pubkey": recipient,
            "redeemed_amount": redeemed,
            "new_already_redeemed": already,
        },
    )


def main():
    if not ISSUER_PRIVATE_KEY:
        fail("ISSUER_PRIVATE_KEY must be set")

    print("🔌 Checking node and tracker connectivity...")
    info = node_api("GET", "/info")
    print(f"   Node height: {info.get('fullHeight')}")
    status, text = http_request("GET", f"{TRACKER_URL}/")
    if status != 200:
        fail(f"Tracker server not reachable ({status}): {text}")
    print("   Tracker server is reachable")

    balances = node_api("GET", "/wallet/balances")
    balance = int(balances.get("balance", 0))
    print(f"   Wallet balance: {balance} nanoERG")
    min_required = RESERVE_AMOUNT + FEE_BOX_AMOUNT * 2 + 5_000_000
    if balance < min_required:
        fail(
            f"Wallet balance too low: {balance} nanoERG; need at least {min_required} nanoERG "
            f"for reserve + fee boxes + issuance reserve"
        )

    wallet_address = WALLET_ADDRESS
    if not wallet_address:
        addrs = node_api("GET", "/wallet/addresses")
        if not addrs:
            fail("No wallet addresses returned by node")
        wallet_address = addrs[0]
        print(f"   Using wallet address: {wallet_address}")

    recipient_pubkey = RECIPIENT_PUBKEY
    if not recipient_pubkey:
        resp = node_api("GET", f"/utils/addressToRaw/{wallet_address}")
        if isinstance(resp, dict):
            recipient_pubkey = (
                resp.get("raw")
                or resp.get("pubkey")
                or resp.get("publicKey")
                or resp.get("value")
            )
        else:
            recipient_pubkey = resp
        print(f"   Derived recipient pubkey: {recipient_pubkey}")

    if not Path(CLI_BIN).exists():
        print("🔨 Building basis_cli...")
        subprocess.run(["cargo", "build", "-p", "basis_cli"], check=True)

    config_dir = tempfile.mkdtemp(prefix="basis_cli_")
    config_path = os.path.join(config_dir, "cli.toml")
    with open(config_path, "w") as f:
        f.write(f'server_url = "{TRACKER_URL}"\naccounts = {{}}\n')

    try:
        issuer_pubkey = get_issuer_pubkey(config_path)
        print(f"✅ Issuer public key: {issuer_pubkey}")

        reserve_nft_id = RESERVE_NFT_ID
        if not reserve_nft_id:
            reserve_nft_id = issue_reserve_nft(wallet_address)
        print(f"✅ Reserve NFT ID: {reserve_nft_id}")

        reserve_box_id = create_reserve(reserve_nft_id, issuer_pubkey, RESERVE_AMOUNT)
        print(f"✅ Reserve box ID: {reserve_box_id}")

        # Create a token-free fee box before creating the note so the tracker can
        # pay for the on-chain commitment transaction.
        create_fee_box(wallet_address)

        create_note(config_path, recipient_pubkey, NOTE_AMOUNT)
        wait_for_note_confirmed(issuer_pubkey, recipient_pubkey)

        tx_ids = []
        cumulative = 0
        for i in range(1, 3):
            print(f"\n=== Redemption {i} ===")
            txid = run_local_sign_redemption(
                config_path, issuer_pubkey, recipient_pubkey, REDEEM_AMOUNT
            )
            tx_ids.append(txid)
            cumulative += REDEEM_AMOUNT
            complete_redemption(
                issuer_pubkey, recipient_pubkey, REDEEM_AMOUNT, cumulative
            )

        print("\n🔍 Final note state:")
        final = tracker_api(
            "POST",
            "/notes/state",
            {"issuer_pubkey": issuer_pubkey, "recipient_pubkey": recipient_pubkey},
        )
        print(json.dumps(final, indent=2))
        if final.get("redeemable") or final.get("redeemable_amount", 0) > 0:
            fail(f"Note still redeemable after both redemptions: {final}")

        print("\n✅ Integration test passed.")
        print("\nTransactions:")
        print(f"  Reserve NFT:      {reserve_nft_id}")
        print(f"  Reserve box:      {reserve_box_id}")
        print(f"  Redemption 1 tx:  {tx_ids[0]}")
        print(f"  Redemption 2 tx:  {tx_ids[1]}")

    finally:
        import shutil
        try:
            shutil.rmtree(config_dir)
        except OSError:
            pass


if __name__ == "__main__":
    main()
