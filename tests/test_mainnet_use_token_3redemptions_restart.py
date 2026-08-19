#!/usr/bin/env python3
"""
Integration test: token-backed (USE) reserve with 3 × 0.1 USD redemptions and a
tracker server restart after the first redemption.

The reserve is backed by a test stablecoin token configured as the tracker's
reserve_token_id.  This test uses the same local Ergo node
(http://127.0.0.1:9053) and Basis tracker server (http://127.0.0.1:3048) as the
ERG-backed test, but all note/redemption amounts are raw token units.

Steps:
  1. Export or generate the issuer keypair.
  2. Derive the recipient public key from the wallet address.
  3. Issue a fresh reserve NFT from a plain-ERG wallet box.
  4. Create a token-backed reserve via the tracker payload + wallet payment/send.
  5. Create a note for the full redeemable token amount.
  6. Redeem 0.1 USD three times with `basis_cli transaction generate-redemption --local-sign`.
  7. Restart the tracker server after the first redemption.
  8. Verify the note is fully redeemed after the third redemption.

Required environment:
  RESERVE_TOKEN_ID      - Hex token ID of the reserve-backed stablecoin.

Optional environment:
  NODE_URL              - Ergo node URL (default http://127.0.0.1:9053)
  API_KEY               - Ergo node api_key (default hello)
  TRACKER_URL           - Basis tracker server URL (default http://127.0.0.1:3048)
  WALLET_ADDRESS        - Wallet address used for issuer/recipient/fees/NFT
  RECIPIENT_PUBKEY      - Recipient public key (hex); derived from wallet address if omitted
  ISSUER_PRIVATE_KEY    - Issuer DLOG secret (hex); generated if omitted
  RESERVE_NFT_ID        - Existing reserve NFT to reuse; if omitted, a fresh NFT is issued
  RESERVE_TOKEN_AMOUNT  - Reserve collateral in raw token units (default 500000 = 0.5 USD with 6 decimals)
  RESERVE_ERG_AMOUNT    - ERG value of the reserve box in nanoERG (default 3000000)
  NOTE_AMOUNT           - Note total debt in raw token units (default 300000 = 0.3 USD)
  REDEEM_AMOUNT         - Each redemption amount in raw token units (default 100000 = 0.1 USD)
  NUM_REDEMPTIONS       - Number of consecutive redemptions (default 3)
  FEE_BOX_AMOUNT        - Size of token-free fee box in nanoERG (default 50000000)
  CLI_BIN               - Path to basis_cli binary (default ./target/debug/basis_cli)
  WAIT_TIMEOUT          - Seconds to wait for confirmations (default 300)
  SERVER_START_SCRIPT   - Script to start tracker server (default ./scripts/run_server.sh)
  SERVER_STOP_SCRIPT    - Script to stop tracker server (default ./scripts/stop_server.sh)
  TX_FEE_NANOERG        - Transaction fee in nanoERG (default 1000000)
  POLL_INTERVAL_SECONDS - Seconds between confirmation polls (default 3)
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
WALLET_ADDRESS = os.environ.get("WALLET_ADDRESS")
RECIPIENT_PUBKEY = os.environ.get("RECIPIENT_PUBKEY")
ISSUER_PRIVATE_KEY = os.environ.get("ISSUER_PRIVATE_KEY")
RESERVE_NFT_ID = os.environ.get("RESERVE_NFT_ID")
RESERVE_TOKEN_ID = os.environ.get("RESERVE_TOKEN_ID")
RESERVE_TOKEN_AMOUNT = int(os.environ.get("RESERVE_TOKEN_AMOUNT", "500000"))
RESERVE_ERG_AMOUNT = int(os.environ.get("RESERVE_ERG_AMOUNT", "3000000"))
NOTE_AMOUNT = int(os.environ.get("NOTE_AMOUNT", "300000"))
REDEEM_AMOUNT = int(os.environ.get("REDEEM_AMOUNT", "100000"))
NUM_REDEMPTIONS = int(os.environ.get("NUM_REDEMPTIONS", "3"))
FEE_BOX_AMOUNT = int(os.environ.get("FEE_BOX_AMOUNT", "50000000"))
CLI_BIN = os.environ.get("CLI_BIN", "./target/debug/basis_cli")
WAIT_TIMEOUT = int(os.environ.get("WAIT_TIMEOUT", "300"))
SERVER_START_SCRIPT = os.environ.get("SERVER_START_SCRIPT", "./scripts/run_server.sh")
SERVER_STOP_SCRIPT = os.environ.get("SERVER_STOP_SCRIPT", "./scripts/stop_server.sh")
TX_FEE_NANOERG = int(os.environ.get("TX_FEE_NANOERG", "1000000"))
POLL_INTERVAL_SECONDS = int(os.environ.get("POLL_INTERVAL_SECONDS", "3"))


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
    """Wait for a tx to leave the mempool and be known to the wallet/node."""
    print(f"⏳ Waiting for {label} {txid} to confirm...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        in_pool_status, _ = http_request(
            "HEAD",
            f"{NODE_URL}/transactions/unconfirmed/{txid}",
        )
        if in_pool_status == 200:
            time.sleep(POLL_INTERVAL_SECONDS)
            continue

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

        time.sleep(POLL_INTERVAL_SECONDS)
    fail(f"Timed out waiting for {label} {txid}")


def wait_for_reserve_tracked(issuer: str, token_amount: int):
    print("⏳ Waiting for reserve to be tracked by the server...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        try:
            data = tracker_api("GET", f"/reserves/issuer/{issuer}")
            for r in data or []:
                if r.get("collateral_amount") == token_amount:
                    print("✅ Reserve tracked")
                    return r
            print("   reserve not yet tracked, retrying...")
        except SystemExit:
            raise
        except Exception as e:
            print(f"   reserve lookup error: {e}")
        time.sleep(POLL_INTERVAL_SECONDS)
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
        time.sleep(POLL_INTERVAL_SECONDS)
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


def generate_fresh_issuer() -> tuple[str, str]:
    """Generate a fresh secp256k1 keypair for the reserve owner/issuer."""
    print("🔑 Generating fresh issuer keypair...")
    result = subprocess.run(
        [CLI_BIN, "generate-keypair"], capture_output=True, text=True, check=True
    )
    out = result.stdout + result.stderr
    pub_m = re.search(r"Public Key \(hex\):\s*([0-9a-f]{66})", out)
    priv_m = re.search(r"Private Key \(hex\):\s*([0-9a-f]{64})", out)
    if not pub_m or not priv_m:
        fail(f"Could not parse generated keypair:\n{out}")
    return priv_m.group(1), pub_m.group(1)


def derive_recipient_pubkey(wallet_address: str) -> str:
    """Derive the recipient public key from the wallet address via node API."""
    raw_resp = node_api("GET", f"/utils/addressToRaw/{wallet_address}")
    if isinstance(raw_resp, dict):
        recipient_pubkey = (
            raw_resp.get("raw")
            or raw_resp.get("pubkey")
            or raw_resp.get("publicKey")
            or raw_resp.get("value")
        )
    else:
        recipient_pubkey = raw_resp
    if not isinstance(recipient_pubkey, str) or len(recipient_pubkey) != 66:
        fail(f"Unexpected raw pubkey format from node: {recipient_pubkey!r}")
    return recipient_pubkey


def find_plain_erg_box(min_value: int) -> tuple[dict, str]:
    """Return a wallet box with no tokens and at least min_value nanoERG."""
    wallet_boxes = node_api("GET", "/wallet/boxes/unspent")
    for entry in wallet_boxes:
        box = entry.get("box", entry)
        assets = box.get("assets", [])
        if not assets and box["value"] >= min_value:
            binary = node_api("GET", f"/utxo/byIdBinary/{box['boxId']}")
            if isinstance(binary, dict):
                binary = binary.get("bytes") or binary
            return box, binary
    fail(
        f"Could not find a plain ERG box with at least {min_value} nanoERG "
        f"for reserve NFT issuance"
    )


def issue_reserve_nft(wallet_address: str) -> str:
    print("🎫 Issuing a fresh reserve NFT from a plain ERG box...")
    required_input = RESERVE_ERG_AMOUNT + FEE_BOX_AMOUNT + TX_FEE_NANOERG
    plain_box, plain_binary = find_plain_erg_box(required_input)
    body = {
        "fee": TX_FEE_NANOERG,
        "requests": [
            {
                "address": wallet_address,
                "ergValue": required_input,
                "amount": 1,
                "name": "BasisReserve",
                "description": "Basis reserve NFT",
                "decimals": 0,
            }
        ],
        "inputsRaw": [plain_binary],
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


def find_reserve_nft_box(reserve_nft_id: str) -> tuple[dict, str]:
    """Return the unspent wallet box holding the reserve NFT and its serialized bytes."""
    wallet_boxes = node_api("GET", "/wallet/boxes/unspent")
    candidates = []
    for entry in wallet_boxes:
        box = entry.get("box", entry)
        assets = box.get("assets", [])
        if any(
            a.get("tokenId") == reserve_nft_id and a.get("amount", 0) >= 1
            for a in assets
        ):
            candidates.append(box)
    if not candidates:
        fail(f"Could not find a wallet box containing reserve NFT {reserve_nft_id}")
    candidates.sort(key=lambda b: b["value"], reverse=True)
    box = candidates[0]
    binary = node_api("GET", f"/utxo/byIdBinary/{box['boxId']}")
    if isinstance(binary, dict):
        binary = binary.get("bytes") or binary
    return box, binary


def find_reserve_token_box(token_id: str, min_amount: int) -> tuple[dict, str]:
    """Return the unspent wallet box holding at least min_amount of the reserve token."""
    wallet_boxes = node_api("GET", "/wallet/boxes/unspent")
    candidates = []
    for entry in wallet_boxes:
        box = entry.get("box", entry)
        assets = box.get("assets", [])
        token_amount = sum(
            a.get("amount", 0)
            for a in assets
            if a.get("tokenId") == token_id
        )
        if token_amount >= min_amount:
            candidates.append((box, token_amount))
    if not candidates:
        fail(
            f"Could not find a wallet box containing at least {min_amount} units of "
            f"token {token_id}"
        )
    # Prefer the box with the smallest sufficient token amount to avoid fragmenting
    # a large treasury box.
    candidates.sort(key=lambda x: x[1])
    box = candidates[0][0]
    binary = node_api("GET", f"/utxo/byIdBinary/{box['boxId']}")
    if isinstance(binary, dict):
        binary = binary.get("bytes") or binary
    return box, binary


def create_reserve(
    nft_id: str, issuer_pubkey: str, erg_amount: int, token_amount: int, token_id: str
) -> str:
    print("🏦 Creating token reserve payload...")
    payload = tracker_api(
        "POST",
        "/reserves/create",
        {
            "nft_id": nft_id,
            "owner_pubkey": issuer_pubkey,
            "erg_amount": erg_amount,
            "token_amount": token_amount,
            "token_id": token_id,
        },
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

    reserve_nft_box, reserve_nft_binary = find_reserve_nft_box(nft_id)
    reserve_token_box, reserve_token_binary = find_reserve_token_box(
        token_id, token_amount
    )
    required_input = req["value"] + TX_FEE_NANOERG
    if reserve_nft_box["value"] < required_input:
        fail(
            f"Reserve NFT box value {reserve_nft_box['value']} is less than required "
            f"{required_input} nanoERG (reserve {req['value']} + fee {TX_FEE_NANOERG})"
        )

    body = {
        "fee": TX_FEE_NANOERG,
        "requests": payment,
        # Provide both the reserve NFT box and the token box as explicit inputs;
        # otherwise the Ergo node will not automatically add a token-bearing input
        # when inputsRaw is supplied.
        "inputsRaw": [reserve_nft_binary, reserve_token_binary],
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
    body = {
        "fee": TX_FEE_NANOERG,
        "requests": [{"address": wallet_address, "value": FEE_BOX_AMOUNT}],
    }
    txid = node_api("POST", "/wallet/transaction/send", body)
    print(f"   Fee box tx: {txid}")
    return wait_for_tx(txid, "fee box")


def run_local_sign_redemption(
    config_path: str, issuer: str, recipient: str, amount: int
) -> str:
    print(f"🔄 Running local-sign redemption for {amount} token units...")
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


def wait_for_reserve_updated(issuer: str, expected_collateral: int):
    print(
        f"⏳ Waiting for reserve scanner to update to collateral={expected_collateral}..."
    )
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        try:
            reserves = tracker_api("GET", f"/reserves/issuer/{issuer}") or []
            for r in reserves:
                if r.get("collateral_amount") == expected_collateral:
                    print(f"✅ Reserve updated (collateral={expected_collateral})")
                    return r
        except SystemExit:
            raise
        except Exception as e:
            print(f"   reserve lookup error: {e}")
        time.sleep(POLL_INTERVAL_SECONDS)
    fail(f"Timed out waiting for reserve collateral to update to {expected_collateral}")


def wait_for_tracker_health():
    """Wait until the tracker server responds on its root endpoint."""
    print("⏳ Waiting for tracker server to become reachable...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        status, text = http_request("GET", f"{TRACKER_URL}/")
        if status == 200:
            print("✅ Tracker server is reachable")
            return
        print(f"   tracker health status={status}, retrying...")
        time.sleep(2)
    fail("Timed out waiting for tracker server to become reachable after restart")


def restart_tracker():
    """Stop and restart the tracker server using the project helper scripts."""
    print("\n🔄 Restarting tracker server...")
    stop_result = subprocess.run([SERVER_STOP_SCRIPT], capture_output=True, text=True)
    print(stop_result.stdout, end="")
    if stop_result.stderr:
        print(stop_result.stderr, end="", file=sys.stderr)

    # Give the OS a moment to release the listening port.
    time.sleep(3)

    start_result = subprocess.run([SERVER_START_SCRIPT], capture_output=True, text=True)
    print(start_result.stdout, end="")
    if start_result.stderr:
        print(start_result.stderr, end="", file=sys.stderr)
    if start_result.returncode != 0:
        fail(f"Failed to start tracker server with {SERVER_START_SCRIPT}")

    wait_for_tracker_health()
    print("✅ Tracker server restarted\n")


def verify_state_after_restart(issuer: str, recipient: str):
    """Confirm the tracker recovered its persisted reserve and note state."""
    expected_collateral = RESERVE_TOKEN_AMOUNT - REDEEM_AMOUNT
    expected_redeemable = NOTE_AMOUNT - REDEEM_AMOUNT

    print("🔍 Verifying reserve state after restart...")
    reserve = wait_for_reserve_updated(issuer, expected_collateral)
    print(f"   Reserve collateral: {reserve.get('collateral_amount')}")

    print("🔍 Verifying note state after restart...")
    start = time.time()
    while time.time() - start < WAIT_TIMEOUT:
        data = tracker_api(
            "POST",
            "/notes/state",
            {"issuer_pubkey": issuer, "recipient_pubkey": recipient},
        )
        status = data.get("status")
        redeemable_amount = data.get("redeemable_amount", 0)
        print(f"   note state: status={status}, redeemable_amount={redeemable_amount}")
        if status == "confirmed" and redeemable_amount == expected_redeemable:
            print("✅ Note state recovered after restart")
            return data
        time.sleep(POLL_INTERVAL_SECONDS)
    fail("Timed out waiting for note state to recover after restart")


def main():
    if not RESERVE_TOKEN_ID:
        fail("RESERVE_TOKEN_ID must be set to the configured reserve token ID")

    print("🔌 Checking node and tracker connectivity...")
    info = node_api("GET", "/info")
    print(f"   Node height: {info.get('fullHeight')}")
    status, text = http_request("GET", f"{TRACKER_URL}/")
    if status != 200:
        fail(f"Tracker server not reachable ({status}): {text}")
    print("   Tracker server is reachable")

    balances = node_api("GET", "/wallet/balances")
    balance = int(balances.get("balance", 0))
    token_balance = int(balances.get("assets", {}).get(RESERVE_TOKEN_ID, 0))
    print(f"   Wallet balance: {balance} nanoERG")
    print(f"   Wallet {RESERVE_TOKEN_ID} balance: {token_balance} raw units")
    min_required_erg = (
        RESERVE_ERG_AMOUNT + FEE_BOX_AMOUNT * 2 + TX_FEE_NANOERG * 4 + 5_000_000
    )
    if balance < min_required_erg:
        fail(
            f"Wallet ERG balance too low: {balance} nanoERG; need at least {min_required_erg} "
            f"for reserve + fee boxes + NFT issuance + transaction fees"
        )
    if token_balance < RESERVE_TOKEN_AMOUNT:
        fail(
            f"Wallet token balance too low: {token_balance} raw units; need at least "
            f"{RESERVE_TOKEN_AMOUNT} raw units of {RESERVE_TOKEN_ID}"
        )

    wallet_address = WALLET_ADDRESS
    if not wallet_address:
        status = node_api("GET", "/wallet/status")
        wallet_address = status.get("changeAddress")
        if not wallet_address:
            addrs = node_api("GET", "/wallet/addresses")
            if not addrs:
                fail("No wallet addresses returned by node")
            wallet_address = addrs[0]
    print(f"   Using wallet address: {wallet_address}")

    global ISSUER_PRIVATE_KEY, RECIPIENT_PUBKEY

    if not Path(CLI_BIN).exists():
        print("🔨 Building basis_cli...")
        subprocess.run(["cargo", "build", "-p", "basis_cli"], check=True)

    if not RECIPIENT_PUBKEY:
        RECIPIENT_PUBKEY = derive_recipient_pubkey(wallet_address)

    if not ISSUER_PRIVATE_KEY:
        ISSUER_PRIVATE_KEY, issuer_pubkey = generate_fresh_issuer()
    else:
        config_dir_tmp = tempfile.mkdtemp(prefix="basis_cli_pubkey_")
        config_path_tmp = os.path.join(config_dir_tmp, "cli.toml")
        with open(config_path_tmp, "w") as f:
            f.write(f'server_url = "{TRACKER_URL}"\ncurrent_account = "issuer"\n\n')
            f.write("[accounts.issuer]\n")
            f.write('name = "issuer"\n')
            f.write('pubkey_hex = ""\n')
            f.write(f'private_key_hex = "{ISSUER_PRIVATE_KEY}"\n')
            f.write(f"created_at = {int(time.time())}\n")
        try:
            out = run_cli(config_path_tmp, ["account", "info"])
            m = re.search(r"Public Key:\s*([0-9a-f]{66})", out)
            if not m:
                fail(f"Could not parse issuer public key from account info:\n{out}")
            issuer_pubkey = m.group(1)
        finally:
            shutil.rmtree(config_dir_tmp, ignore_errors=True)

    print(f"✅ Issuer public key: {issuer_pubkey}")

    config_dir = tempfile.mkdtemp(prefix="basis_cli_")
    config_path = os.path.join(config_dir, "cli.toml")
    with open(config_path, "w") as f:
        f.write(f'server_url = "{TRACKER_URL}"\ncurrent_account = "issuer"\n\n')
        f.write("[accounts.issuer]\n")
        f.write('name = "issuer"\n')
        f.write(f'pubkey_hex = "{issuer_pubkey}"\n')
        f.write(f'private_key_hex = "{ISSUER_PRIVATE_KEY}"\n')
        f.write(f"created_at = {int(time.time())}\n")

    try:
        reserve_nft_id = RESERVE_NFT_ID
        if not reserve_nft_id:
            reserve_nft_id = issue_reserve_nft(wallet_address)
        print(f"✅ Reserve NFT ID: {reserve_nft_id}")

        reserve_box_id = create_reserve(
            reserve_nft_id,
            issuer_pubkey,
            RESERVE_ERG_AMOUNT,
            RESERVE_TOKEN_AMOUNT,
            RESERVE_TOKEN_ID,
        )
        print(f"✅ Reserve box ID: {reserve_box_id}")

        create_fee_box(wallet_address)

        create_note(config_path, RECIPIENT_PUBKEY, NOTE_AMOUNT)
        wait_for_note_confirmed(issuer_pubkey, RECIPIENT_PUBKEY)

        tx_ids = []
        cumulative = 0
        for i in range(1, NUM_REDEMPTIONS + 1):
            print(f"\n=== Redemption {i} ===")
            txid = run_local_sign_redemption(
                config_path, issuer_pubkey, RECIPIENT_PUBKEY, REDEEM_AMOUNT
            )
            tx_ids.append(txid)
            cumulative += REDEEM_AMOUNT
            remaining_collateral = RESERVE_TOKEN_AMOUNT - cumulative
            if remaining_collateral > 0 and i < NUM_REDEMPTIONS:
                wait_for_reserve_updated(issuer_pubkey, remaining_collateral)

            if i == 1:
                restart_tracker()
                verify_state_after_restart(issuer_pubkey, RECIPIENT_PUBKEY)

        print("\n🔍 Final note state:")
        final = tracker_api(
            "POST",
            "/notes/state",
            {"issuer_pubkey": issuer_pubkey, "recipient_pubkey": RECIPIENT_PUBKEY},
        )
        print(json.dumps(final, indent=2))
        if final.get("redeemable") or final.get("redeemable_amount", 0) > 0:
            fail(f"Note still redeemable after all redemptions: {final}")

        print("\n✅ Integration test passed.")
        print("\nTransactions:")
        print(f"  Reserve NFT:      {reserve_nft_id}")
        print(f"  Reserve box:      {reserve_box_id}")
        for i, txid in enumerate(tx_ids, start=1):
            print(f"  Redemption {i} tx:  {txid}")

    finally:
        try:
            shutil.rmtree(config_dir)
            print(f"\n🧹 Cleaned up temporary CLI config: {config_dir}")
        except OSError:
            pass


if __name__ == "__main__":
    main()
