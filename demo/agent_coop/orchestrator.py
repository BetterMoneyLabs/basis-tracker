#!/usr/bin/env python3
"""
Basis Agent Co-op Demo — pure-credit joint economy via MCP.

Three scripted agents (Alice, Bob, Charlie) exchange services and settle with
off-chain IOU notes through a shared Basis tracker. No reserves, collateral,
or on-chain redemption are used in this starter version.

Each agent runs its own `basis-mcp` process with an isolated HOME directory so
wallets and private keys do not collide.
"""

import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEMO_DIR = Path(__file__).resolve().parent
DATA_DIR = DEMO_DIR / "data"
PROJECT_ROOT = DEMO_DIR.parent.parent

AGENTS = ["alice", "bob", "charlie"]

# Service prices in nanoERG.
PRICE_SMALL = 10_000_000   # 0.01 ERG
PRICE_MEDIUM = 20_000_000  # 0.02 ERG

# Unsecured credit limit per peer in nanoERG.
MAX_DEBT = 50_000_000  # 0.05 ERG

# Tracker server URL (shared by all agents).
SERVER_URL = os.environ.get("BASIS_SERVER_URL", "http://127.0.0.1:3048")

# Path to the basis-mcp binary. Override with BASIS_MCP env var.
BASIS_MCP = os.environ.get("BASIS_MCP", PROJECT_ROOT / "target" / "debug" / "basis-mcp")


# ---------------------------------------------------------------------------
# MCP stdio client
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

            # Ignore notifications / logs that have no id.
            if "id" not in msg:
                continue

            if msg.get("id") == expected_id:
                return msg
            # Otherwise it's a response to a different request; keep waiting.

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
                "clientInfo": {"name": "basis-agent-coop-demo", "version": "0.1.0"},
            },
        })
        response = self._recv(init_id)
        if "error" in response:
            raise RuntimeError(f"MCP initialize failed: {response['error']}")

        self._send({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        })

    def call_tool(self, name: str, arguments: Optional[Dict[str, Any]] = None) -> Any:
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
        response = self._recv(req_id)

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
    client: McpClient
    pubkey: str = ""


def reset_agent_home(name: str) -> Path:
    """Create a fresh isolated home directory for an agent."""
    home = DATA_DIR / name
    if home.exists():
        shutil.rmtree(home)
    (home / ".basis").mkdir(parents=True)
    # Create a minimal cli.toml so basis-mcp knows the server URL even before
    # any account is created.
    (home / ".basis" / "cli.toml").write_text(
        f'server_url = "{SERVER_URL}"\naccounts = {{}}\n',
        encoding="utf-8",
    )
    return home


def bootstrap_agent(name: str) -> Agent:
    home = reset_agent_home(name)
    print(f"\n[BOOTSTRAP] Starting {name}'s MCP wallet...")
    client = McpClient(home, SERVER_URL)
    client.start()

    # Ensure stderr does not block by draining it in the background is not
    # necessary for short-lived demo runs, but keep stderr pipe small by
    # reading any warnings at the end.

    account = client.call_tool("account_create", {"name": name})
    print(f"  {name} account: {account['pubkey_hex'][:20]}...")
    return Agent(name=name, client=client, pubkey=account["pubkey_hex"])


def set_trust_policy(agent: Agent, peers: List[Agent]) -> None:
    """Publish a whitelist policy accepting notes from peers up to MAX_DEBT."""
    holders = [p.pubkey for p in peers if p.name != agent.name]
    policy = {
        "default": "reject",
        "root": "trusted_peers",
        "predicates": [
            {
                "type": "whitelist",
                "name": "trusted_peers",
                "holders": holders,
                "max_debt": MAX_DEBT,
            }
        ],
    }
    print(f"\n[POLICY] {agent.name} publishes trust policy (max debt {MAX_DEBT} nanoERG per peer)")
    result = agent.client.call_tool("policy_set", {"policy": policy})
    uploaded = result.get("uploaded", False)
    print(f"  saved={result.get('saved', False)}, uploaded={uploaded}, hash={result.get('policy_hash', 'n/a')[:16]}...")


def issue_note(payer: Agent, recipient: Agent, amount: int, description: str) -> None:
    print(f"\n[NOTE] {payer.name} pays {recipient.name} {amount:,} nanoERG for: {description}")
    result = payer.client.call_tool("note_create", {
        "recipient": recipient.pubkey,
        "amount": amount,
    })
    print(f"  issued -> total debt now {result['amount']:,} nanoERG")


def list_notes(agent: Agent, direction: str) -> List[Dict[str, Any]]:
    return agent.client.call_tool("note_list", {"direction": direction}) or []


def stop_agent(agent: Agent) -> None:
    agent.client.stop()


# ---------------------------------------------------------------------------
# Scenario
# ---------------------------------------------------------------------------

def print_balance_sheet(agents: List[Agent]) -> None:
    print("\n" + "=" * 64)
    print("FINAL BALANCE SHEET")
    print("=" * 64)
    print(f"{'Agent':<10} {'Assets (ERG)':>14} {'Liabilities (ERG)':>18} {'Net (ERG)':>14}")
    print("-" * 64)

    totals = []
    for agent in agents:
        issued = list_notes(agent, "issued")
        received = list_notes(agent, "received")

        liabilities = sum(n["amount"] for n in issued)
        assets = sum(n["amount"] for n in received)
        net = assets - liabilities
        totals.append((agent.name, assets, liabilities, net))

        print(f"{agent.name:<10} {assets/1e9:>14.6f} {liabilities/1e9:>18.6f} {net/1e9:>14.6f}")

    print("-" * 64)
    if sum(t[3] for t in totals) == 0:
        print("Balance sheet checks out: net positions sum to zero.")
    else:
        print("WARNING: net positions do not sum to zero (this should not happen).")


def run_scenario() -> None:
    print("=" * 64)
    print("Basis Agent Co-op Demo — Pure Credit Economy")
    print("=" * 64)
    print(f"Tracker server: {SERVER_URL}")
    print(f"basis-mcp:      {BASIS_MCP}")

    # 1. Bootstrap agents.
    agents = [bootstrap_agent(name) for name in AGENTS]
    alice, bob, charlie = agents

    try:
        # 2. Publish trust policies.
        for agent in agents:
            set_trust_policy(agent, agents)

        # 3. Service round 1 — each agent pays one other agent.
        issue_note(alice, bob, PRICE_SMALL, "Bob stores 1 GB for Alice")
        issue_note(bob, charlie, PRICE_SMALL, "Charlie routes 100 API calls for Bob")
        issue_note(charlie, alice, PRICE_SMALL, "Alice runs a compute job for Charlie")

        # 4. Service round 2 — Alice requests more storage, issuing a second
        #    note to Bob. Amount is cumulative (previous 0.01 + new 0.02 = 0.03).
        issue_note(alice, bob, PRICE_SMALL + PRICE_MEDIUM, "Bob stores another 2 GB for Alice")

        # 5. Print final balance sheet.
        print_balance_sheet(agents)

        # 6. Show a credit-limit warning if any peer is close to the cap.
        print("\n[CREDIT LIMITS]")
        for agent in agents:
            for note in list_notes(agent, "issued"):
                pct = note["amount"] / MAX_DEBT * 100
                bar = "█" * int(pct / 10) + "░" * (10 - int(pct / 10))
                print(f"  {agent.name} -> {note['recipient_pubkey'][:20]}... {note['amount']/1e9:.6f}/{MAX_DEBT/1e9:.6f} ERG [{bar}] {pct:.0f}%")

        print("\nDemo complete.")

    finally:
        print("\n[SHUTDOWN] Stopping agent MCP processes...")
        for agent in agents:
            stop_agent(agent)
        print("Done.")


if __name__ == "__main__":
    try:
        run_scenario()
    except Exception as exc:
        print(f"\n[DEMO FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
