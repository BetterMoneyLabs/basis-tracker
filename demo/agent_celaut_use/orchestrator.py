#!/usr/bin/env python3
"""
Basis + Celaut + USE Stablecoin Demo — agentic credit with on-chain redemption.

Four agents participate in a minimal Celaut-style service economy:

  * dev_alice    : service developer (publishes a deterministic service spec)
  * node_bob     : node maintainer (runs services, extends credit, redeems on-chain)
  * user_charlie : trusted service user (pays with pure-credit IOUs)
  * user_dave    : new service user (must back notes with a USE reserve)

The demo demonstrates three tiers of money:

  1. Pure credit          : user_charlie pays node_bob via whitelist policy.
  2. Collateralized credit: user_dave's note is rejected until his USE reserve
                            covers >= 100% of liabilities, then accepted.
  3. Backed money         : node_bob redeems user_dave's IOU on-chain for USE
                            tokens from dave's basis-token.es reserve.

Each agent runs its own isolated basis-mcp process. The demo requires a real
Ergo node and USE tokens in the node wallet.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
import urllib.error
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from service_runner import ExecutionResult, ServiceSpec, execute
from reserve_helper import (
    node_get,
    node_post,
    wait_for_node_confirmation as _wait_for_node_confirmation,
    address_to_ergo_tree,
    get_wallet_address,
    find_reserve_box,
    create_use_reserve as _create_use_reserve,
)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEMO_DIR = Path(__file__).resolve().parent
DATA_DIR = DEMO_DIR / "data"
PROJECT_ROOT = DEMO_DIR.parent.parent

AGENTS = ["dev_alice", "node_bob", "user_charlie", "user_dave"]
SERVICE_NAME = "hash-service"

# USE has 3 decimals in this deployment.
USE_DECIMALS = 3
USE_UNIT = 10 ** USE_DECIMALS

# Amounts in raw USE units. Total test exposure stays below 1 USE (1000 units).
SERVICE_PRICE = 100                      # 0.1 USE per execution
TRUSTED_CREDIT_LIMIT = 200               # pure-credit cap for charlie (0.2 USE)
DAVE_RESERVE_AMOUNT = 500                # dave's USE collateral (0.5 USE)
REDEEM_AMOUNT = 100                      # amount bob redeems on-chain (0.1 USE)

# Tracker server URL (shared by all agents).
SERVER_URL = os.environ.get("BASIS_SERVER_URL", "http://127.0.0.1:3048")

# Path to the basis-mcp binary. Override with BASIS_MCP env var.
BASIS_MCP = os.environ.get("BASIS_MCP", PROJECT_ROOT / "target" / "debug" / "basis-mcp")

# Required environment variables.
USE_TOKEN_ID = os.environ.get("USE_TOKEN_ID", "")
DAVE_RESERVE_NFT_ID = os.environ.get("DAVE_RESERVE_NFT_ID", "")
TRACKER_NFT_ID = os.environ.get("TRACKER_NFT_ID", "")
DAVE_PUBKEY = os.environ.get("DAVE_PUBKEY", "")
DAVE_SECRET = os.environ.get("DAVE_SECRET", "")

# How long to wait for the scanner to detect a reserve (seconds).
RESERVE_POLL_TIMEOUT = float(os.environ.get("BASIS_RESERVE_POLL_TIMEOUT", "300"))

# How long to wait for an off-chain note to be committed to a tracker box on
# mainnet. Default 900s (15 min) because mainnet block times are variable.
NOTE_CONFIRM_TIMEOUT = float(os.environ.get("BASIS_NOTE_CONFIRM_TIMEOUT", "900"))

# How long to wait for on-chain transactions (reserve creation, redemption)
# to be confirmed. Default 900s to handle mainnet block intervals.
TX_CONFIRM_TIMEOUT = float(os.environ.get("BASIS_TX_CONFIRM_TIMEOUT", "900"))


# ---------------------------------------------------------------------------
# HTTP helpers
# ---------------------------------------------------------------------------

def http_post(path: str, payload: Any) -> Dict[str, Any]:
    req = urllib.request.Request(
        SERVER_URL + path,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        raise RuntimeError(f"POST {path} failed ({e.code}): {body[:300]}")


def http_get(path: str) -> Dict[str, Any]:
    with urllib.request.urlopen(SERVER_URL + path, timeout=15) as resp:
        return json.loads(resp.read().decode())


def acceptance_check(issuer: str, recipient: str, total_debt: int) -> Dict[str, Any]:
    resp = http_post("/acceptance/check", {
        "issuer_pubkey": issuer,
        "recipient_pubkey": recipient,
        "total_debt": total_debt,
    })
    if not resp.get("success"):
        raise RuntimeError(f"/acceptance/check error: {resp.get('error')}")
    return resp["data"]


def get_reserve_token_config() -> Dict[str, Any]:
    resp = http_get("/config/reserve-token")
    if not resp.get("success"):
        raise RuntimeError(f"/config/reserve-token error: {resp.get('error')}")
    return resp["data"]


def submit_reserve(payload: Any) -> Dict[str, Any]:
    resp = http_post("/reserves/submit", payload)
    if not resp.get("success"):
        raise RuntimeError(f"/reserves/submit error: {resp.get('error')}")
    return resp["data"]


# ---------------------------------------------------------------------------
# MCP stdio client (same protocol as demo/agent_coop and demo/agent_teams)
# ---------------------------------------------------------------------------

class McpClient:
    """Minimal MCP client speaking JSON-RPC over a subprocess' stdio."""

    def __init__(self, home_dir: Path, server_url: str = SERVER_URL):
        self.home_dir = home_dir
        self.server_url = server_url
        self.proc: Optional[subprocess.Popen] = None
        self._next_id = 1

    def start(self) -> None:
        env = os.environ.copy()
        env["HOME"] = str(self.home_dir)
        env["BASIS_SERVER_URL"] = self.server_url

        self.proc = subprocess.Popen(
            [str(BASIS_MCP)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )
        self._initialize()

    def stop(self) -> None:
        if self.proc is not None:
            try:
                self.proc.stdin.close()
            except Exception:
                pass
            self.proc.wait(timeout=5)
            self.proc = None

    def _send(self, message: Dict[str, Any]) -> None:
        line = json.dumps(message, separators=(",", ":"))
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()

    def _recv(self, expected_id: int, timeout: float = 30.0) -> Dict[str, Any]:
        deadline = time.time() + timeout
        while time.time() < deadline:
            line = self.proc.stdout.readline()
            if not line:
                time.sleep(0.05)
                continue
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError as e:
                print(f"  [warn] non-JSON stdout line: {line[:200]} ({e})")
                continue

            if "id" not in msg:
                continue

            if msg.get("id") == expected_id:
                return msg

        raise TimeoutError(f"Did not receive MCP response for id {expected_id}")

    def _initialize(self) -> None:
        init_id = self._next_id
        self._send({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "basis-agent-celaut-use-demo", "version": "0.1.0"},
            },
        })
        response = self._recv(init_id)
        if "error" in response:
            raise RuntimeError(f"MCP initialize failed: {response['error']}")

        self._send({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        })

    def call_tool(self, name: str, arguments: Optional[Dict[str, Any]] = None,
                  timeout: float = 30.0) -> Any:
        req_id = self._next_id
        self._next_id += 1
        self._send({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments or {},
            },
        })
        response = self._recv(req_id, timeout=timeout)

        if "error" in response:
            raise RuntimeError(f"MCP tool error for {name}: {response['error']}")

        result = response.get("result", {})
        if result.get("isError"):
            text = "\n".join(
                item.get("text", "")
                for item in result.get("content", [])
                if item.get("type") == "text"
            )
            raise RuntimeError(f"Tool {name} returned error: {text}")

        text = "\n".join(
            item.get("text", "")
            for item in result.get("content", [])
            if item.get("type") == "text"
        )
        try:
            return json.loads(text) if text else None
        except json.JSONDecodeError:
            return text


# ---------------------------------------------------------------------------
# Agent wrapper
# ---------------------------------------------------------------------------

@dataclass
class Agent:
    name: str
    role: str
    client: McpClient
    pubkey: str = ""


def use_units(raw: int) -> str:
    """Convert raw USE units to a human-readable USE string."""
    return f"{raw / USE_UNIT:.6f}"


def reset_agent_home(name: str) -> Path:
    """Create a fresh isolated home directory for an agent."""
    home = DATA_DIR / name
    if home.exists():
        shutil.rmtree(home)
    (home / ".basis").mkdir(parents=True)
    (home / ".basis" / "cli.toml").write_text(
        f'server_url = "{SERVER_URL}"\naccounts = {{}}\n',
        encoding="utf-8",
    )
    return home


def bootstrap_agent(name: str, role: str, private_key_hex: str = "") -> Agent:
    home = reset_agent_home(name)
    print(f"\n[BOOTSTRAP] Starting {name} ({role})...")
    client = McpClient(home, SERVER_URL)
    client.start()
    if private_key_hex:
        account = client.call_tool("account_import", {
            "name": name,
            "private_key_hex": private_key_hex,
        })
        # account_import does not always select the imported account as current.
        client.call_tool("account_switch", {"name": name})
        print(f"  {name} account (imported): {account['pubkey_hex'][:20]}...")
    else:
        account = client.call_tool("account_create", {"name": name})
        print(f"  {name} account: {account['pubkey_hex'][:20]}...")
    return Agent(name=name, role=role, client=client, pubkey=account["pubkey_hex"])


# ---------------------------------------------------------------------------
# Policies
# ---------------------------------------------------------------------------

def node_bob_policy(charlie_pubkey: str) -> Dict[str, Any]:
    """
    node_bob accepts:
      * pure credit from user_charlie up to TRUSTED_CREDIT_LIMIT
      * notes from anyone else only if backed >= 100% by a USE reserve
    """
    return {
        "default": "reject",
        "root": "accept",
        "predicates": [
            {
                "type": "any_of",
                "name": "accept",
                "predicates": ["trusted_user", "fully_collateralized_user"],
            },
            {
                "type": "whitelist",
                "name": "trusted_user",
                "holders": [charlie_pubkey],
                "max_debt": TRUSTED_CREDIT_LIMIT,
            },
            {
                "type": "all_of",
                "name": "fully_collateralized_user",
                "predicates": ["min_collateral"],
            },
            {
                "type": "collateralization",
                "name": "min_collateral",
                "min_ratio": 1.0,
            },
        ],
    }


def reject_all_policy() -> Dict[str, Any]:
    """Agents that only pay out do not accept incoming notes."""
    return {"default": "reject"}


def set_policy(agent: Agent, policy: Dict[str, Any], label: str) -> None:
    print(f"\n[POLICY] {agent.name} publishes {label}")
    result = agent.client.call_tool("policy_set", {"policy": policy})
    print(f"  saved={result.get('saved', False)}, uploaded={result.get('uploaded', False)}, "
          f"hash={result.get('policy_hash', 'n/a')[:16]}...")


# ---------------------------------------------------------------------------
# Service registration and execution
# ---------------------------------------------------------------------------

def register_service(developer: Agent) -> ServiceSpec:
    spec = ServiceSpec(
        name=SERVICE_NAME,
        box="python3:sha256-deterministic:v1",
        api="execute(input_bytes: bytes) -> output_hash: str",
        net="isolated",
        price_use=SERVICE_PRICE,
    )
    print(f"\n[SERVICE] {developer.name} registers '{spec.name}'")
    print(f"  spec: {json.dumps(spec.to_dict(), indent=2)}")
    return spec


def quote_service(provider: Agent, spec: ServiceSpec) -> int:
    """node_bob quotes the service price in raw USE units."""
    print(f"\n[QUOTE] {provider.name} quotes {use_units(spec.price_use)} USE for '{spec.name}'")
    return spec.price_use


def execute_service(user: Agent, provider: Agent, spec: ServiceSpec,
                    input_bytes: bytes) -> ExecutionResult:
    print(f"\n[EXECUTE] {user.name} runs '{spec.name}' on {provider.name}")
    result = execute(spec, input_bytes)
    print(f"  input:  {result.input_hex[:32]}...")
    print(f"  output: {result.output_hash}")
    print(f"  time:   {result.execution_time_ms} ms")
    return result


# ---------------------------------------------------------------------------
# Notes and reserves
# ---------------------------------------------------------------------------

def issue_note(payer: Agent, recipient: Agent, amount: int, description: str) -> Dict[str, Any]:
    print(f"\n[NOTE] {payer.name} pays {recipient.name} {use_units(amount)} USE for: {description}")
    result = payer.client.call_tool("note_create", {
        "recipient": recipient.pubkey,
        "amount": amount,
    })
    print(f"  issued -> total debt now {use_units(result['amount'])} USE")
    return result


def list_notes(agent: Agent, direction: str) -> List[Dict[str, Any]]:
    return agent.client.call_tool("note_list", {"direction": direction}) or []


def wait_for_node_confirmation(tx_id: str, timeout: float = TX_CONFIRM_TIMEOUT) -> None:
    """Wait until the transaction is confirmed on the Ergo node."""
    _wait_for_node_confirmation(tx_id, timeout=timeout)


def get_node_wallet_keypair() -> Tuple[str, str, str]:
    """Return (pubkey, secret, address) for the first address in the Ergo node wallet."""
    addresses = node_get("/wallet/addresses")
    if not addresses:
        raise RuntimeError("node wallet has no addresses")
    address = str(addresses[0])
    raw = node_get(f"/utils/addressToRaw/{address}")
    if isinstance(raw, dict):
        pubkey = str(raw.get("raw") or raw.get("pubkey") or raw.get("publicKey") or raw.get("value"))
    else:
        pubkey = str(raw)
    if len(pubkey) != 66:
        raise RuntimeError(f"unexpected pubkey format/length from node: {pubkey}")
    status, body = node_post("/wallet/getPrivateKey", {"address": address})
    if status != 200:
        raise RuntimeError(f"/wallet/getPrivateKey failed ({status}): {body}")
    secret = str(body) if isinstance(body, str) else str(body.get("privateKey", body))
    if len(secret) != 64:
        raise RuntimeError(f"unexpected secret length from node: {secret}")
    return pubkey, secret, address


def wait_for_note_confirmed(issuer: str, recipient: str, timeout: float = NOTE_CONFIRM_TIMEOUT) -> Dict[str, Any]:
    """Wait until the off-chain note is committed to a tracker box on-chain."""
    print(f"  [WAIT] Waiting for note {issuer[:16]}... -> {recipient[:16]}... to be confirmed on-chain")
    print(f"  [WAIT] Timeout set to {timeout:.0f}s (mainnet blocks can take several minutes)")
    deadline = time.time() + timeout
    last_status = None
    while time.time() < deadline:
        try:
            resp = http_post("/notes/state", {
                "issuer_pubkey": issuer,
                "recipient_pubkey": recipient,
            })
            data = resp.get("data", {})
            status = data.get("status")
            redeemable = data.get("redeemable")
            if status != last_status:
                print(f"  [WAIT] note status: {status}, redeemable={redeemable}")
                last_status = status
            if status == "confirmed" and redeemable:
                print(f"  [WAIT] Note confirmed and redeemable")
                return data
        except Exception as exc:
            print(f"  [WAIT] note state not ready yet: {exc}")
        time.sleep(10)
    raise TimeoutError(f"note not confirmed within {timeout:.0f}s")


def redeem_note_local(issuer_pubkey: str, issuer_secret: str, recipient_pubkey: str,
                      amount: int) -> str:
    """
    Perform a real on-chain redemption via basis_cli local-sign path.
    The issuer (Dave) signs the reserve message; the recipient (Bob, whose key
    lives in the Ergo node wallet) signs the reserve input proveDlog.
    """
    print(f"\n[REDEEM] node_bob redeems {use_units(amount)} USE from user_dave (local-sign)")

    # The CLI binary name uses an underscore (basis_cli), not a hyphen.
    cli_bin = Path(BASIS_MCP).resolve().parent / "basis_cli"
    if not cli_bin.exists():
        cli_bin = PROJECT_ROOT / "target" / "release" / "basis_cli"
    if not cli_bin.exists():
        cli_bin = PROJECT_ROOT / "target" / "debug" / "basis_cli"
    if not cli_bin.exists():
        raise RuntimeError(f"basis_cli binary not found (looked near {BASIS_MCP})")

    # Create an isolated config with Dave as the current account (required by the
    # local-sign path so it can produce the issuer's reserve signature).
    config_dir = Path(tempfile.mkdtemp(prefix="basis_redeem_dave_"))
    config_path = config_dir / "cli.toml"
    config_path.write_text(
        f'server_url = "{SERVER_URL}"\ncurrent_account = "dave"\n\n'
        f'[accounts.dave]\nname = "dave"\n'
        f'pubkey_hex = "{issuer_pubkey}"\n'
        f'private_key_hex = "{issuer_secret}"\n'
        f'created_at = {int(time.time())}\n',
        encoding="utf-8",
    )

    try:
        cmd = [
            str(cli_bin),
            "--config", str(config_path),
            "--server-url", SERVER_URL,
            "transaction", "generate-redemption",
            "--issuer-pubkey", issuer_pubkey,
            "--recipient-pubkey", recipient_pubkey,
            "--amount", str(amount),
            "--local-sign",
        ]
        print(f"  running: {' '.join(cmd)}")
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=240)
        print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
        if result.returncode != 0:
            raise RuntimeError(f"basis_cli redemption failed (exit {result.returncode})")

        m = re.search(r"Transaction ID:\s*([0-9a-f]{64})", result.stdout + result.stderr)
        if not m:
            raise RuntimeError("could not parse redemption transaction id from CLI output")
        tx_id = m.group(1)
        print(f"  broadcast -> tx_id {tx_id}")
        wait_for_node_confirmation(tx_id)
        print("  ✅ On-chain redemption confirmed.")
        return tx_id
    finally:
        shutil.rmtree(config_dir, ignore_errors=True)


def create_use_reserve(agent: Agent, nft_id: str, token_amount: int,
                       erg_amount: int = 3_000_000) -> None:
    """Create a USE-token-backed reserve via basis-token.es if one does not exist."""
    print(f"\n[RESERVE] {agent.name} creates USE-backed reserve of {use_units(token_amount)} USE "
          f"(NFT {nft_id[:12]}...)")

    config = get_reserve_token_config()
    reserve_p2s = config["basis_token_reserve_contract_p2s"]
    reserve_ergo_tree = address_to_ergo_tree(reserve_p2s)
    reserve_token_id = config.get("reserve_token_id") or USE_TOKEN_ID

    existing = find_reserve_box(nft_id, reserve_ergo_tree)
    if existing is not None:
        existing_collateral = 0
        for asset in existing.get("assets", []):
            if asset.get("tokenId") == USE_TOKEN_ID:
                existing_collateral = int(asset.get("amount", 0))
                break
        print(f"  reserve already on-chain (box {existing['boxId'][:16]}...); "
              f"collateral {use_units(existing_collateral)} USE; skipping creation")
    else:
        box = _create_use_reserve(
            owner_pubkey=agent.pubkey,
            nft_id=nft_id,
            reserve_token_id=reserve_token_id,
            tracker_nft_id=TRACKER_NFT_ID,
            token_amount=token_amount,
            erg_amount=erg_amount,
        )
        print(f"  submitted -> tx_id {box['transactionId']}")
        wait_for_node_confirmation(box["transactionId"])

    deadline = time.time() + RESERVE_POLL_TIMEOUT
    while time.time() < deadline:
        status = agent.client.call_tool("reserve_status", {"pubkey": agent.pubkey})
        if status:
            collateral = status.get("collateral", 0)
            if collateral >= token_amount or (existing is not None and collateral > 0):
                print(f"  confirmed on-chain: collateral {use_units(collateral)} USE")
                return
        time.sleep(5)
    raise TimeoutError(
        f"reserve for {agent.name} not detected within {RESERVE_POLL_TIMEOUT:.0f}s"
    )


def wait_for_tracker_box(timeout: float = TX_CONFIRM_TIMEOUT) -> None:
    """Wait until the server has created and confirmed a tracker box on-chain."""
    print("\n[WAIT] Waiting for on-chain tracker box (created after first notes)...")
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            resp = http_get("/tracker/latest-box-id")
            if resp.get("success") and resp.get("data", {}).get("tracker_box_id"):
                box_id = resp["data"]["tracker_box_id"]
                print(f"  tracker box confirmed: {box_id[:16]}...")
                return
        except Exception:
            pass
        time.sleep(5)
    raise TimeoutError(f"tracker box not confirmed within {timeout:.0f}s")


def redeem_note(recipient: Agent, issuer: Agent, amount: int) -> None:
    wait_for_tracker_box()
    print(f"\n[REDEEM] {recipient.name} redeems {use_units(amount)} USE from {issuer.name}")
    try:
        result = recipient.client.call_tool(
            "note_redeem", {"issuer": issuer.pubkey, "amount": amount},
            timeout=120.0,
        )
        print(f"  redeemed -> tx_id {result.get('tx_id')}")
        print("  backed credit converted to real USE tokens.")
    except Exception as exc:
        print(f"  [warn] redemption not completed: {exc}")
        print("  (the tracker box update may not have enough confirmations yet — "
              "try `basis-cli note redeem` later)")


# ---------------------------------------------------------------------------
# Reports
# ---------------------------------------------------------------------------

def print_balance_sheet(agents: Dict[str, Agent]) -> None:
    print("\n" + "=" * 72)
    print("FINAL BALANCE SHEET (USE)")
    print("=" * 72)
    print(f"{'Agent':<14} {'Role':<20} {'Assets (USE)':>14} {'Liabilities (USE)':>18} {'Net (USE)':>12}")
    print("-" * 72)

    nets = []
    for name in AGENTS:
        agent = agents[name]
        issued = list_notes(agent, "issued")
        received = list_notes(agent, "received")

        liabilities = sum(n["amount"] - n.get("redeemed", 0) for n in issued)
        assets = sum(n["amount"] - n.get("redeemed", 0) for n in received)
        net = assets - liabilities
        nets.append(net)
        print(f"{name:<14} {agent.role:<20} {assets / USE_UNIT:>14.6f} "
              f"{liabilities / USE_UNIT:>18.6f} {net / USE_UNIT:>12.6f}")

    print("-" * 72)
    if sum(nets) == 0:
        print("Balance sheet checks out: net positions sum to zero.")
    else:
        print("WARNING: net positions do not sum to zero.")


def print_collateralization_report(agents: Dict[str, Agent]) -> None:
    print("\n[COLLATERALIZATION]")
    for name in AGENTS:
        agent = agents[name]
        status = agent.client.call_tool("reserve_status", {"pubkey": agent.pubkey})
        if not status:
            continue
        ratio = status.get("collateralization_ratio", 0.0)
        print(f"  {name:<14} collateral {use_units(status.get('collateral', 0))} USE, "
              f"debt {use_units(status.get('total_debt', 0))} USE, ratio {ratio:.2f}")


# ---------------------------------------------------------------------------
# Scenario
# ---------------------------------------------------------------------------

def check_prerequisites() -> None:
    missing = []
    if not USE_TOKEN_ID:
        missing.append("USE_TOKEN_ID")
    if not DAVE_RESERVE_NFT_ID:
        missing.append("DAVE_RESERVE_NFT_ID")
    if missing:
        raise RuntimeError(
            "missing required environment variables: " + ", ".join(missing) +
            " — see demo/agent_celaut_use/README.md"
        )

    if len(USE_TOKEN_ID) != 64 or not all(c in "0123456789abcdefABCDEF" for c in USE_TOKEN_ID):
        raise RuntimeError("USE_TOKEN_ID must be a 64-character hex string")

    config = get_reserve_token_config()
    configured_token = config.get("reserve_token_id")
    if configured_token and configured_token.lower() != USE_TOKEN_ID.lower():
        raise RuntimeError(
            f"configured reserve_token_id ({configured_token}) does not match USE_TOKEN_ID ({USE_TOKEN_ID})"
        )
    print(f"[CONFIG] tracker reserve token: {USE_TOKEN_ID[:16]}...")


def run_scenario(auto: bool) -> None:
    print("=" * 72)
    print("Basis + Celaut + USE Demo — Agentic Credit with On-Chain Redemption")
    print("=" * 72)
    print(f"Tracker server: {SERVER_URL}")
    print(f"basis-mcp:      {BASIS_MCP}")

    check_prerequisites()

    # node_bob runs the local Ergo node wallet, so import its key so the local-
    # sign redemption path can fetch the recipient's private key from the node.
    wallet_pubkey, wallet_secret, wallet_address = get_node_wallet_keypair()
    print(f"[CONFIG] node wallet address: {wallet_address}")
    print(f"[CONFIG] node wallet pubkey:  {wallet_pubkey[:20]}...")

    # 1. Bootstrap agents.
    # Dave uses the fixed demo keypair so his on-chain reserve (R4) matches his
    # account and his notes can be redeemed against it.
    agents = {
        "dev_alice": bootstrap_agent("dev_alice", "service developer"),
        "node_bob": bootstrap_agent("node_bob", "node maintainer", private_key_hex=wallet_secret),
        "user_charlie": bootstrap_agent("user_charlie", "trusted user"),
        "user_dave": bootstrap_agent("user_dave", "new user", private_key_hex=DAVE_SECRET),
    }

    try:
        # 2. Publish policies.
        set_policy(agents["node_bob"], node_bob_policy(agents["user_charlie"].pubkey),
                   "pure credit for charlie, >=100% USE collateral for others")
        for name in ["dev_alice", "user_charlie", "user_dave"]:
            set_policy(agents[name], reject_all_policy(), "reject-all (this agent only pays)")

        # 3. Register the Celaut-style service.
        spec = register_service(agents["dev_alice"])

        # 4. Trusted user pays with pure credit.
        print("\n" + "=" * 72)
        print("PURE CREDIT — user_charlie is whitelisted by node_bob")
        print("=" * 72)
        charlie_input = b"hello celaut from charlie"
        execute_service(agents["user_charlie"], agents["node_bob"], spec, charlie_input)
        issue_note(agents["user_charlie"], agents["node_bob"], SERVICE_PRICE,
                   "hash-service execution (pure credit)")

        # 5. Collateralization gate.
        print("\n" + "=" * 72)
        print("COLLATERALIZATION GATE — user_dave needs a USE reserve")
        print("=" * 72)
        dave_input = b"hello celaut from dave"
        execute_service(agents["user_dave"], agents["node_bob"], spec, dave_input)
        check = acceptance_check(agents["user_dave"].pubkey, agents["node_bob"].pubkey, SERVICE_PRICE)
        print(f"  dave -> bob {use_units(SERVICE_PRICE)} USE: "
              f"acceptable={check['acceptable']} (reason: {check.get('reason')})")

        if check["acceptable"]:
            print("  [GATE] Dave already has a USE-backed reserve covering this note.")
        else:
            print("  [GATE] Rejected as expected. dave must create a USE-backed reserve.")

            # 6. New user creates a USE-backed reserve.
            print("\n" + "=" * 72)
            print("CREATE USE RESERVE — user_dave backs his credit on-chain")
            print("=" * 72)
            create_use_reserve(agents["user_dave"], DAVE_RESERVE_NFT_ID, DAVE_RESERVE_AMOUNT)

        # 7. Issue Dave's collateralized note.
        print("\n" + "=" * 72)
        print("COLLATERALIZED CREDIT — user_dave's note is accepted")
        print("=" * 72)
        check = acceptance_check(agents["user_dave"].pubkey, agents["node_bob"].pubkey, SERVICE_PRICE)
        print(f"  dave -> bob {use_units(SERVICE_PRICE)} USE: "
              f"acceptable={check['acceptable']} (reason: {check.get('reason')})")
        if not check["acceptable"]:
            raise RuntimeError("expected acceptance here — dave's reserve should cover the note")
        issue_note(agents["user_dave"], agents["node_bob"], SERVICE_PRICE,
                   "hash-service execution (USE-collateralized credit)")

        # 8. Creditor redeems on-chain for real USE tokens.
        print("\n" + "=" * 72)
        print("ON-CHAIN REDEMPTION — node_bob converts dave's IOU to USE tokens")
        print("=" * 72)
        wait_for_note_confirmed(agents["user_dave"].pubkey, agents["node_bob"].pubkey)
        redemption_tx_id = redeem_note_local(
            agents["user_dave"].pubkey,
            DAVE_SECRET,
            agents["node_bob"].pubkey,
            REDEEM_AMOUNT,
        )
        print(f"\n[RESULT] Redemption transaction: {redemption_tx_id}")

        # 9. Reports.
        print_balance_sheet(agents)
        print_collateralization_report(agents)

        print("\nDemo complete.")

    finally:
        print("\n[SHUTDOWN] Stopping agent MCP processes...")
        for agent in agents.values():
            agent.client.stop()
        print("Done.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Basis + Celaut + USE agentic credit demo")
    parser.add_argument("--auto", action="store_true",
                        help="non-interactive mode (no effect; scenario is fully scripted)")
    args = parser.parse_args()
    try:
        run_scenario(auto=args.auto)
    except Exception as exc:
        print(f"\n[DEMO FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
