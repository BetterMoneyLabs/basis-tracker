#!/usr/bin/env python3
"""
Basis Sovereign Agent Demo — a self-maintaining, self-improving agentic economy.

Five agents participate:

  * sovereign      : the protagonist. Identity = Ergo node wallet key. Starts with
                     a fixed USE seed reserve ("seed capital") and nothing else.
                     It buys compute, executes verifiable bounties, collects and
                     redeems backed income on-chain, upgrades its own capabilities,
                     and survives a price shock — with zero human actions.
  * infra_compute  : Celaut node maintainer selling metered compute tiers.
  * bounty_board   : automated escrow/judge posting hash-verified bounties backed
                     by a fully-collateralized USE reserve (replaces the human
                     judge of demo/agent_teams).
  * skill_vendor   : sells capability packs that unlock higher-value task classes.

Narrative beats:

  1. Stranger gate     — a key with no reserve cannot buy compute (live check).
  2. Metabolism        — rounds 1..3: buy v1 compute, earn basic bounties,
                         settle income on-chain. Acceptance rides the
                         collateralization branch of infra's policy.
  3. Credit emergence  — after three clean rounds infra whitelists the sovereign;
                         later purchases ride the whitelist branch until cumulative
                         debt outgrows it and the collateral branch resumes.
  4. Growth            — round 4: buy the premium-markets skill pack, switch to
                         v2 compute, unlock premium bounties (measurable revenue jump).
  5. Shock             — round 5: compute prices double; premium margin goes
                         negative; the agent downgrades to basic and survives.
  6. Recovery          — round 6: prices normalize; re-upgrade; report shows P&L,
                         collateralization history and finite runway.

Requires a real Ergo node, USE tokens and NFTs — same setup as run 6 of
demo/agent_celaut_use (see specs/celaut_use_demo_run6_report.md).
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
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional

from market import (
    COMPUTE_TIERS,
    SKILL_PACKS,
    TASK_CLASSES,
    execute_task,
    post_task,
    verify_deliverable,
)
from economics import (
    SEED_RESERVE_USE,
    INFRA_WHITELIST_LIMIT,
    RoundRecord,
    SovereignLedger,
    adapt_downgrade,
    choose_work,
    plan_upgrade,
    print_report,
    runway_rounds,
)
from reserve_helper import (
    node_get,
    node_post,
    wait_for_node_confirmation as _wait_for_node_confirmation,
    get_node_wallet_keypair,
)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEMO_DIR = Path(__file__).resolve().parent
DATA_DIR = DEMO_DIR / "data"
PROJECT_ROOT = DEMO_DIR.parent.parent

AGENTS = ["sovereign", "infra_compute", "bounty_board", "skill_vendor"]

# USE has 3 decimals in this deployment.
USE_DECIMALS = 3
USE_UNIT = 10 ** USE_DECIMALS

# Round schedule (1-based). Round SHOCK_ROUND doubles all compute prices.
SHOCK_ROUND = int(os.environ.get("SOVEREIGN_SHOCK_ROUND", "5"))
UPGRADE_ROUND = int(os.environ.get("SOVEREIGN_UPGRADE_ROUND", "4"))
CREDIT_EMERGENCE_ROUND = 3   # infra whitelists the sovereign after this round
TOTAL_ROUNDS = int(os.environ.get("SOVEREIGN_TOTAL_ROUNDS", "6"))

# Tracker server URL (shared by all agents).
SERVER_URL = os.environ.get("BASIS_SERVER_URL", "http://127.0.0.1:3048")

# Path to the basis-mcp binary. Override with BASIS_MCP env var.
BASIS_MCP = os.environ.get("BASIS_MCP", PROJECT_ROOT / "target" / "release" / "basis-mcp")

# Required environment variables (see README).
USE_TOKEN_ID = os.environ.get("USE_TOKEN_ID", "")
TRACKER_NFT_ID = os.environ.get("TRACKER_NFT_ID", "")

# Bounty board has a fixed demo keypair so its escrow reserve can be locked in
# preflight before the server starts. Override to reuse an existing reserve.
BOUNTY_PUBKEY = os.environ.get(
    "BOUNTY_PUBKEY",
    "02ea220b8d7b6b1727b7e555493144f1b7085debdf24e41e547a687425b0d3c802",
)
BOUNTY_SECRET = os.environ.get(
    "BOUNTY_SECRET",
    "2acafd6320fbe5dbc5f1e84e8d075b0d1dadafffab1b9441a3ade1b508f83a64e",
)

# How long to wait for the scanner to see reserves / notes / transactions.
RESERVE_POLL_TIMEOUT = float(os.environ.get("BASIS_RESERVE_POLL_TIMEOUT", "300"))
NOTE_CONFIRM_TIMEOUT = float(os.environ.get("BASIS_NOTE_CONFIRM_TIMEOUT", "900"))
TX_CONFIRM_TIMEOUT = float(os.environ.get("BASIS_TX_CONFIRM_TIMEOUT", "900"))

# A deterministic throwaway pubkey for the stranger-gate probe (never funded).
STRANGER_PUBKEY = "02" + __import__("hashlib").sha256(
    b"sovereign-demo-stranger-probe").hexdigest()


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


def use_units(raw: int) -> str:
    return f"{raw / USE_UNIT:.6f}"


# ---------------------------------------------------------------------------
# MCP stdio client (same protocol as demo/agent_coop and demo/agent_celaut_use)
# ---------------------------------------------------------------------------

class McpClient:
    """Minimal MCP client speaking JSON-RPC over a subprocess' stdio."""

    def __init__(self, home_dir: Path, server_url: str = SERVER_URL):
        self.home_dir = home_dir
        self.server_url = server_url
        self.proc = None
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
                "clientInfo": {"name": "basis-agent-sovereign-demo", "version": "0.1.0"},
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
            "params": {"name": name, "arguments": arguments or {}},
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


@dataclass
class Agent:
    name: str
    role: str
    client: McpClient
    pubkey: str = ""


def reset_agent_home(name: str) -> Path:
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

def infra_policy_initial(sovereign_pubkey: str) -> Dict[str, Any]:
    """
    infra_compute accepts notes only from issuers whose reserve covers >= 100%
    of their outstanding liabilities. No whitelist yet — credit must be earned.
    """
    return {
        "default": "reject",
        "root": "accept",
        "predicates": [
            {
                "type": "all_of",
                "name": "accept",
                "predicates": ["fully_collateralized"],
            },
            {
                "type": "collateralization",
                "name": "fully_collateralized",
                "min_ratio": 1.0,
            },
        ],
    }


def infra_policy_promoted(sovereign_pubkey: str) -> Dict[str, Any]:
    """
    After three clean rounds infra extends pure credit to the sovereign up to
    INFRA_WHITELIST_LIMIT raw USE; everyone else still needs full collateral.
    """
    return {
        "default": "reject",
        "root": "accept",
        "predicates": [
            {
                "type": "any_of",
                "name": "accept",
                "predicates": ["trusted_agent", "fully_collateralized"],
            },
            {
                "type": "whitelist",
                "name": "trusted_agent",
                "holders": [sovereign_pubkey],
                "max_debt": INFRA_WHITELIST_LIMIT,
            },
            {
                "type": "collateralization",
                "name": "fully_collateralized",
                "min_ratio": 1.0,
            },
        ],
    }


def sovereign_policy(bounty_pubkey: str) -> Dict[str, Any]:
    """The sovereign accepts bounty payments from the board (its 'employer')."""
    return {
        "default": "reject",
        "root": "accept",
        "predicates": [
            {
                "type": "all_of",
                "name": "accept",
                "predicates": ["employer"],
            },
            {
                "type": "whitelist",
                "name": "employer",
                "holders": [bounty_pubkey],
                "max_debt": 1500,
            },
        ],
    }


def reject_all_policy() -> Dict[str, Any]:
    return {"default": "reject"}


def set_policy(agent: Agent, policy: Dict[str, Any], label: str) -> None:
    print(f"\n[POLICY] {agent.name} publishes {label}")
    result = agent.client.call_tool("policy_set", {"policy": policy})
    print(f"  saved={result.get('saved', False)}, uploaded={result.get('uploaded', False)}, "
          f"hash={result.get('policy_hash', 'n/a')[:16]}...")


# ---------------------------------------------------------------------------
# Notes, verification, redemption
# ---------------------------------------------------------------------------

def issue_note(payer: Agent, recipient: Agent, amount: int, description: str) -> Dict[str, Any]:
    print(f"\n[NOTE] {payer.name} pays {recipient.name} {use_units(amount)} USE "
          f"(cumulative restatement) for: {description}")
    result = payer.client.call_tool("note_create", {
        "recipient": recipient.pubkey,
        "amount": amount,
    })
    print(f"  issued -> total debt now {use_units(result['amount'])} USE")
    return result


def wait_for_note_confirmed(issuer: str, recipient: str,
                            timeout: float = NOTE_CONFIRM_TIMEOUT) -> Dict[str, Any]:
    """Wait until the off-chain note is committed to a tracker box on-chain."""
    print(f"  [WAIT] Waiting for note {issuer[:16]}... -> {recipient[:16]}... "
          f"to be committed on-chain (timeout {timeout:.0f}s)")
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
                print("  [WAIT] Note confirmed and redeemable")
                return data
        except Exception as exc:
            print(f"  [WAIT] note state not ready yet: {exc}")
        time.sleep(10)
    raise TimeoutError(f"note not confirmed within {timeout:.0f}s")


def redeem_income_mcp(sovereign: Agent, board_pubkey: str, amount: int) -> Dict[str, Any]:
    """Redemption attempt via the sovereign's own MCP (server-side signing)."""
    return sovereign.client.call_tool(
        "note_redeem", {"issuer": board_pubkey, "amount": amount}, timeout=120.0)


def redeem_income_cli(board_pubkey: str, board_secret: str, sovereign_pubkey: str,
                      amount: int) -> str:
    """
    Fallback: run-6-proven local-sign path via basis_cli. The issuer (board)
    signs from a temp config; the recipient (sovereign) key lives in the node
    wallet and is fetched by the CLI itself.
    """
    print("  [REDEEM] falling back to basis_cli local-sign path")
    cli_bin = Path(BASIS_MCP).resolve().parent / "basis_cli"
    if not cli_bin.exists():
        cli_bin = PROJECT_ROOT / "target" / "debug" / "basis_cli"
    if not cli_bin.exists():
        raise RuntimeError(f"basis_cli binary not found (looked near {BASIS_MCP})")

    config_dir = Path(tempfile.mkdtemp(prefix="basis_redeem_board_"))
    config_path = config_dir / "cli.toml"
    config_path.write_text(
        f'server_url = "{SERVER_URL}"\ncurrent_account = "bounty_board"\n\n'
        f'[accounts.bounty_board]\nname = "bounty_board"\n'
        f'pubkey_hex = "{board_pubkey}"\n'
        f'private_key_hex = "{board_secret}"\n'
        f'created_at = {int(time.time())}\n',
        encoding="utf-8",
    )
    try:
        cmd = [
            str(cli_bin),
            "--config", str(config_path),
            "--server-url", SERVER_URL,
            "transaction", "generate-redemption",
            "--issuer-pubkey", board_pubkey,
            "--recipient-pubkey", sovereign_pubkey,
            "--amount", str(amount),
            "--local-sign",
        ]
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
        wait_for_node_confirmation(tx_id)
        print(f"  ✅ On-chain income settlement confirmed ({tx_id[:16]}...)")
        return tx_id
    finally:
        shutil.rmtree(config_dir, ignore_errors=True)


def settle_income(sovereign: Agent, board_pubkey: str, delta: int) -> Optional[str]:
    """Redeem `delta` of outstanding bounty debt; MCP first, CLI fallback."""
    if delta <= 0:
        return None
    print(f"\n[INCOME] sovereign settles {use_units(delta)} USE of earned bounty "
          f"on-chain (backed money -> real USE tokens)")
    try:
        result = redeem_income_mcp(sovereign, board_pubkey, delta)
        tx_id = result.get("tx_id")
        print(f"  redeemed via MCP -> tx_id {tx_id}")
        return tx_id
    except Exception as exc:
        print(f"  [warn] MCP redemption unavailable: {exc}")
    try:
        return redeem_income_cli(board_pubkey, BOUNTY_SECRET, sovereign.pubkey, delta)
    except Exception as exc:
        print(f"  [warn] CLI fallback also failed: {exc}")
        print("         (income stays as redeemable backed credit; run "
              "`basis_cli transaction generate-redemption` later)")
        return None


# ---------------------------------------------------------------------------
# Reports
# ---------------------------------------------------------------------------

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
    for var in ("USE_TOKEN_ID", "TRACKER_NFT_ID"):
        if not os.environ.get(var):
            missing.append(var)
    if missing:
        raise RuntimeError(
            "missing required environment variables: " + ", ".join(missing) +
            " — see demo/agent_sovereign/README.md"
        )
    if len(USE_TOKEN_ID) != 64 or not all(c in "0123456789abcdefABCDEF" for c in USE_TOKEN_ID):
        raise RuntimeError("USE_TOKEN_ID must be a 64-character hex string")


def reserve_status_or_none(agent: Agent) -> Optional[Dict[str, Any]]:
    try:
        return agent.client.call_tool("reserve_status", {"pubkey": agent.pubkey})
    except Exception:
        return None


def run_scenario(auto: bool) -> None:
    print("=" * 78)
    print("Basis Sovereign Agent Demo — metabolism + growth, no human in the loop")
    print("=" * 78)
    print(f"Tracker server: {SERVER_URL}")
    print(f"basis-mcp:      {BASIS_MCP}")

    check_prerequisites()

    # The sovereign's identity IS the Ergo node wallet key: income redeemed
    # on-chain lands in that wallet, and its seed reserve was locked from it.
    wallet_pubkey, wallet_secret, wallet_address = get_node_wallet_keypair()
    print(f"[CONFIG] node wallet address (sovereign): {wallet_address}")
    print(f"[CONFIG] sovereign identity:              {wallet_pubkey[:20]}...")

    # 1. Bootstrap agents.
    agents = {
        "sovereign": bootstrap_agent("sovereign", "self-sovereign worker",
                                     private_key_hex=wallet_secret),
        "infra_compute": bootstrap_agent("infra_compute", "compute node maintainer"),
        "bounty_board": bootstrap_agent("bounty_board", "escrow judge",
                                        private_key_hex=BOUNTY_SECRET),
        "skill_vendor": bootstrap_agent("skill_vendor", "capability vendor"),
    }
    sovereign = agents["sovereign"]
    infra = agents["infra_compute"]
    board = agents["bounty_board"]
    vendor = agents["skill_vendor"]

    try:
        # 2. Publish acceptance policies.
        set_policy(infra, infra_policy_initial(sovereign.pubkey),
                   "collateralization >= 100% only (no trust yet)")
        set_policy(board, reject_all_policy(), "reject-all (only issues bounties)")
        set_policy(vendor, reject_all_policy(), "reject-all (only sells packs)")
        set_policy(sovereign, sovereign_policy(board.pubkey),
                   "whitelist bounty_board (must accept its employer's bounties)")

        # 3. Seed capital check: preflight locked the fixed USE seed reserve.
        status = reserve_status_or_none(sovereign)
        if not status or status.get("collateral", 0) <= 0:
            raise RuntimeError(
                "sovereign seed reserve not found — run.sh preflight should have "
                "locked it before starting the tracker"
            )
        print(f"\n[SEED] sovereign seed capital confirmed on-chain: "
              f"{use_units(status['collateral'])} USE "
              f"(ratio {status.get('collateralization_ratio', 0):.2f})")

        escrow = reserve_status_or_none(board)
        if escrow and escrow.get("collateral", 0) > 0:
            print(f"[SEED] bounty_board escrow confirmed on-chain: "
                  f"{use_units(escrow['collateral'])} USE — bounty notes are backed")

        # 4. Stranger gate: an unfunded key cannot buy compute. Live rejection.
        print("\n" + "=" * 78)
        print("STRANGER GATE — a key without reserve or reputation is rejected")
        print("=" * 78)
        probe_cost = TASK_CLASSES["basic"].compute_units * COMPUTE_TIERS["v1"].unit_price
        check = acceptance_check(STRANGER_PUBKEY, infra.pubkey, probe_cost)
        print(f"  stranger -> infra {use_units(probe_cost)} USE: "
              f"acceptable={check['acceptable']} (reason: {check.get('reason')})")
        if check["acceptable"]:
            print("  [warn] expected rejection — policy did not gate the stranger")

        # 5. Rounds.
        ledger = SovereignLedger(tier="v1", skills=set())
        bounty_earned_cum = 0     # gross bounty debt board -> sovereign
        bounty_settled = 0        # part already redeemed on-chain
        price_multiplier_override: Dict[int, float] = {SHOCK_ROUND: 2.0}

        for rnd in range(1, TOTAL_ROUNDS + 1):
            mult = price_multiplier_override.get(rnd, 1.0)
            print("\n" + "=" * 78)
            tag = " — PRICE SHOCK (x2)" if mult != 1.0 else ""
            print(f"ROUND {rnd}{tag}")
            print("=" * 78)

            # -- growth ------------------------------------------------------
            if rnd == UPGRADE_ROUND:
                want, why = plan_upgrade(ledger, mult)
                if want:
                    pack_name = list(SKILL_PACKS)[0]
                    pack_price = SKILL_PACKS[pack_name].price
                    print(f"\n[GROWTH] buying '{pack_name}' skill pack "
                          f"({use_units(pack_price)} USE): {why}")
                    ledger.other_debt += pack_price
                    issue_note(sovereign, vendor, pack_price,
                               f"skill pack '{pack_name}' (capability upgrade)")
                    ledger.unlock(pack_name)
                    ledger.tier = "v2"
                    print(f"  capability upgraded: tier v2, skills {sorted(ledger.skills)}")

            # -- work selection ----------------------------------------------
            cls, why = choose_work(ledger, mult)
            if cls is None:
                print(f"\n[IDLE] round {rnd}: {why}")
                ledger.record(RoundRecord(round_no=rnd, multiplier=mult,
                                          action="idle", note=why))
                continue

            tier_price = COMPUTE_TIERS[
                "v2" if cls.required_tier == "v2" else "v1"].unit_price
            cost = int(round(tier_price * cls.compute_units * mult))

            # -- acceptance gate (projected) ----------------------------------
            projected = ledger.projected_total_debt(extra_spend_to_infra=cost)
            branch = ledger.acceptance_branch(cost)
            check = acceptance_check(sovereign.pubkey, infra.pubkey, projected)
            print(f"\n[GATE] sovereign -> infra projected debt "
                  f"{use_units(projected)} USE: acceptable={check['acceptable']} "
                  f"via {branch} branch (reason: {check.get('reason')})")
            if not check["acceptable"]:
                alt, why = adapt_downgrade(ledger, cls, mult)
                if alt is None:
                    print(f"  [ADAPT] {why}")
                    ledger.record(RoundRecord(round_no=rnd, multiplier=mult,
                                              action="idle", note=why))
                    continue
                print(f"  [ADAPT] {why}")
                cls = alt
                tier_price = COMPUTE_TIERS[
                    "v2" if cls.required_tier == "v2" else "v1"].unit_price
                cost = int(round(tier_price * cls.compute_units * mult))

            # -- metabolism: rent compute ------------------------------------
            print(f"\n[METABOLISM] renting {cls.compute_units} unit(s) of "
              f"'{('v2' if cls.required_tier == 'v2' else 'v1')}' compute "
              f"for {use_units(cost)} USE")
            ledger.infra_debt_cum += cost
            issue_note(sovereign, infra, ledger.infra_debt_cum,
                       f"compute ({cls.name} task, x{mult:.1f} price)")

            # -- labor: execute + verify --------------------------------------
            task = post_task(cls.name, nonce=f"r{rnd}-{sovereign.pubkey[:8]}")
            deliverable = execute_task(task, ledger.tier)
            ok = verify_deliverable(task, deliverable, ledger.tier)
            print(f"\n[WORK] executing '{cls.name}' task {task.task_id} "
                  f"in {deliverable.execution_time_ms} ms")
            print(f"  deliverable hash: {deliverable.output_hash}")
            if not ok:
                print("  ❌ board verification FAILED — no payout this round")
                ledger.record(RoundRecord(round_no=rnd, multiplier=mult,
                                          action=cls.name, spend_compute=cost,
                                          verified=False,
                                          note="verification failed; no payout"))
                continue
            print("  ✔ board verification passed")

            # -- income: backed bounty note -----------------------------------
            bounty_earned_cum += cls.bounty
            issue_note(board, sovereign, bounty_earned_cum,
                       f"verified '{cls.name}' bounty (escrow-backed)")

            # -- credit emergence ---------------------------------------------
            if rnd == CREDIT_EMERGENCE_ROUND:
                print("\n[CREDIT EMERGENCE] infra whitelists the sovereign after "
                      f"three clean rounds (pure credit up to "
                      f"{use_units(INFRA_WHITELIST_LIMIT)} USE)")
                set_policy(infra, infra_policy_promoted(sovereign.pubkey),
                           f"whitelist(sovereign, {INFRA_WHITELIST_LIMIT}) OR "
                           f"collateralization >= 100%")

            # -- settlement: first income proves the loop ---------------------
            outstanding = bounty_earned_cum - bounty_settled
            if rnd == 1:
                wait_for_note_confirmed(board.pubkey, sovereign.pubkey)
                if settle_income(sovereign, board.pubkey, outstanding):
                    bounty_settled += outstanding

            rec = RoundRecord(round_no=rnd, multiplier=mult, action=cls.name,
                              spend_compute=cost, bounty_earned=cls.bounty,
                              verified=True)
            ledger.record(rec)
            collat = ledger.collateralization()
            collat_str = f"{collat:.2f}" if collat != float("inf") else "inf"
            print(f"\n[STATUS] round {rnd}: net {rec.net:+d}, "
                  f"total debt {ledger.total_debt}/{SEED_RESERVE_USE} seed "
                  f"(c={collat_str}), runway {runway_rounds(ledger, mult)} "
                  f"premium round(s)")

        # 6. Final settlement of remaining earned income.
        outstanding = bounty_earned_cum - bounty_settled
        if outstanding > 0:
            wait_for_note_confirmed(board.pubkey, sovereign.pubkey)
            if settle_income(sovereign, board.pubkey, outstanding):
                bounty_settled += outstanding

        # 7. Reports.
        print_report(ledger)
        print_collateralization_report(agents)

        settled_pct = 100.0 * bounty_settled / bounty_earned_cum if bounty_earned_cum else 100.0
        print(f"\n[SOVEREIGNTY CHECK] earned {use_units(bounty_earned_cum)} USE, "
              f"settled {settled_pct:.0f}% on-chain, spent on metabolism+growth, "
              f"survived shock — zero human actions.")
        print("\nDemo complete.")

    finally:
        print("\n[SHUTDOWN] Stopping agent MCP processes...")
        for agent in agents.values():
            agent.client.stop()
        print("Done.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Basis sovereign agent economy demo")
    parser.add_argument("--auto", action="store_true",
                        help="non-interactive mode (no effect; scenario is fully scripted)")
    args = parser.parse_args()
    try:
        run_scenario(auto=args.auto)
    except Exception as exc:
        print(f"\n[DEMO FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
