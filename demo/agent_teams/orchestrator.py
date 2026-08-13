#!/usr/bin/env python3
"""
Basis Agent Teams Demo — three-tier money: pure credit, partially
collateralized credit, and fully backed money.

Two agent teams (each led by a managing agent that decomposes the judge's task
into subtasks and hires role agents) collaborate economically through a shared
Basis tracker:

  1. Intra-team payments are PURE CREDIT (trust only).
  2. Cross-team payments are CREDIT COLLATERALIZED AT >= 50% — the recipient's
     acceptance policy requires the issuer's reserve to cover at least half of
     its liabilities.
  3. The human judge's prize is FULLY BACKED by her on-chain reserve and is
     redeemed on-chain by the winning manager.

Unlike demo/agent_coop, this demo requires a real Ergo node (see README.md).

Each agent runs its own `basis-mcp` process with an isolated HOME directory so
wallets and private keys do not collide.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request
import urllib.error
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEMO_DIR = Path(__file__).resolve().parent
DATA_DIR = DEMO_DIR / "data"
PROJECT_ROOT = DEMO_DIR.parent.parent

TEAMS = {
    "alpha": {"manager": "adam", "workers": {"compute": "ava", "storage": "alex"}},
    "beta": {"manager": "bella", "workers": {"compute": "bryn", "storage": "ben"}},
}
MANAGERS = [t["manager"] for t in TEAMS.values()]
WORKERS = [w for t in TEAMS.values() for w in t["workers"].values()]
JUDGE = "judy"
AGENTS = MANAGERS + WORKERS + [JUDGE]

# Amounts in nanoERG.
COMPUTE_PRICE = 20_000_000      # 0.02 ERG — intra-team compute subtask
STORAGE_PRICE = 10_000_000      # 0.01 ERG — intra-team storage subtask
CROSS_COMPUTE_PRICE = 15_000_000  # 0.015 ERG — cross-team compute purchase
CROSS_STORAGE_PRICE = 5_000_000   # 0.005 ERG — cross-team storage purchase
PRIZE = 100_000_000             # 0.1 ERG — judge's backed prize
REDEEM_AMOUNT = 40_000_000      # 0.04 ERG — on-chain redemption of the prize
BONUS_COMPUTE_TOTAL = 25_000_000    # cumulative restatement after bonus
BONUS_STORAGE_TOTAL = 12_500_000    # cumulative restatement after bonus

# Reserves (on-chain collateral).
JUDGE_RESERVE = 200_000_000     # 0.2 ERG — backs the prize fully
MANAGER_RESERVE = 50_000_000    # 0.05 ERG — backs cross-team credit >= 50%

# Acceptance-policy credit limits.
INTRA_TEAM_LIMIT = 50_000_000   # 0.05 ERG
CROSS_TEAM_LIMIT = 20_000_000   # 0.02 ERG
JUDGE_LIMIT = 150_000_000       # 0.15 ERG
MIN_CROSS_RATIO = 0.5           # cross-team notes must be >= 50% collateralized
MIN_JUDGE_RATIO = 1.0           # judge's money must be fully backed

# Tracker server URL (shared by all agents).
SERVER_URL = os.environ.get("BASIS_SERVER_URL", "http://127.0.0.1:3048")

# Path to the basis-mcp binary. Override with BASIS_MCP env var.
BASIS_MCP = os.environ.get("BASIS_MCP", PROJECT_ROOT / "target" / "debug" / "basis-mcp")

# Reserve NFT ids (supplied by run.sh from the environment).
RESERVE_NFTS = {
    JUDGE: os.environ.get("JUDGE_RESERVE_NFT_ID", ""),
    "adam": os.environ.get("ADAM_RESERVE_NFT_ID", ""),
    "bella": os.environ.get("BELLA_RESERVE_NFT_ID", ""),
}

# How long to wait for the scanner to detect a reserve (seconds).
RESERVE_POLL_TIMEOUT = float(os.environ.get("BASIS_RESERVE_POLL_TIMEOUT", "300"))


# ---------------------------------------------------------------------------
# HTTP helper (tracker REST endpoints not exposed via MCP)
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


# ---------------------------------------------------------------------------
# MCP stdio client (same protocol as demo/agent_coop)
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
                "clientInfo": {"name": "basis-agent-teams-demo", "version": "0.1.0"},
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
    client: McpClient
    pubkey: str = ""


def erg(nano: int) -> str:
    return f"{nano / 1e9:.6f}"


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
    account = client.call_tool("account_create", {"name": name})
    print(f"  {name} account: {account['pubkey_hex'][:20]}...")
    return Agent(name=name, client=client, pubkey=account["pubkey_hex"])


# ---------------------------------------------------------------------------
# Acceptance policies
# ---------------------------------------------------------------------------

def worker_policy(own_manager: str, rival_manager: str) -> Dict[str, Any]:
    """Workers: pure credit from their own manager; cross-team credit only if
    the issuer's reserve covers >= 50% of its liabilities."""
    return {
        "default": "reject",
        "root": "accept",
        "predicates": [
            {"type": "any_of", "name": "accept", "predicates": ["own_team", "cross_team"]},
            {"type": "whitelist", "name": "own_team", "holders": [own_manager],
             "max_debt": INTRA_TEAM_LIMIT},
            {"type": "all_of", "name": "cross_team",
             "predicates": ["rival_wl", "half_backed"]},
            {"type": "whitelist", "name": "rival_wl", "holders": [rival_manager],
             "max_debt": CROSS_TEAM_LIMIT},
            {"type": "collateralization", "name": "half_backed",
             "min_ratio": MIN_CROSS_RATIO},
        ],
    }


def manager_policy(judge_pubkey: str) -> Dict[str, Any]:
    """Managers: accept the judge's money only if it is fully backed."""
    return {
        "default": "reject",
        "root": "judge_money",
        "predicates": [
            {"type": "all_of", "name": "judge_money",
             "predicates": ["judge_wl", "fully_backed"]},
            {"type": "whitelist", "name": "judge_wl", "holders": [judge_pubkey],
             "max_debt": JUDGE_LIMIT},
            {"type": "collateralization", "name": "fully_backed",
             "min_ratio": MIN_JUDGE_RATIO},
        ],
    }


def set_policy(agent: Agent, policy: Dict[str, Any], label: str) -> None:
    print(f"\n[POLICY] {agent.name} publishes {label}")
    result = agent.client.call_tool("policy_set", {"policy": policy})
    print(f"  saved={result.get('saved', False)}, uploaded={result.get('uploaded', False)}, "
          f"hash={result.get('policy_hash', 'n/a')[:16]}...")


def publish_policies(agents: Dict[str, Agent]) -> None:
    judy = agents[JUDGE]
    for team_name, team in TEAMS.items():
        rival = TEAMS["beta" if team_name == "alpha" else "alpha"]
        own_manager = agents[team["manager"]].pubkey
        rival_manager = agents[rival["manager"]].pubkey
        for role, worker_name in team["workers"].items():
            set_policy(
                agents[worker_name],
                worker_policy(own_manager, rival_manager),
                f"pure-credit intra-team + >=50%-backed cross-team policy ({role} worker)",
            )
    for manager_name in MANAGERS:
        set_policy(agents[manager_name], manager_policy(judy.pubkey),
                   "fully-backed judge-money policy")
    set_policy(judy, {"default": "reject"}, "reject-all policy (judge only issues money)")


# ---------------------------------------------------------------------------
# Reserves (backed money)
# ---------------------------------------------------------------------------

def create_reserve(agent: Agent, nft_id: str, amount: int) -> None:
    print(f"\n[RESERVE] {agent.name} creates an on-chain reserve of {erg(amount)} ERG "
          f"(NFT {nft_id[:12]}...)")
    result = agent.client.call_tool("reserve_create", {"nft_id": nft_id, "amount": amount})

    submission = http_post("/reserves/submit", result["payload"])
    if not submission.get("success"):
        raise RuntimeError(f"reserve submission failed: {submission.get('error')}")
    print(f"  submitted -> tx_id {submission['data']['tx_id']}")

    # Wait for the scanner to report the collateral.
    deadline = time.time() + RESERVE_POLL_TIMEOUT
    while time.time() < deadline:
        status = agent.client.call_tool("reserve_status", {"pubkey": agent.pubkey})
        if status and status.get("collateral", 0) >= amount:
            print(f"  confirmed on-chain: collateral {erg(status['collateral'])} ERG")
            return
        time.sleep(5)
    raise TimeoutError(
        f"reserve for {agent.name} not detected within {RESERVE_POLL_TIMEOUT:.0f}s "
        "(scanner still catching up?)"
    )


# ---------------------------------------------------------------------------
# Notes
# ---------------------------------------------------------------------------

def issue_note(payer: Agent, recipient: Agent, amount: int, description: str) -> Dict[str, Any]:
    print(f"\n[NOTE] {payer.name} pays {recipient.name} {amount:,} nanoERG for: {description}")
    result = payer.client.call_tool("note_create", {
        "recipient": recipient.pubkey,
        "amount": amount,
    })
    print(f"  issued -> total debt now {result['amount']:,} nanoERG")
    return result


def list_notes(agent: Agent, direction: str) -> List[Dict[str, Any]]:
    return agent.client.call_tool("note_list", {"direction": direction}) or []


# ---------------------------------------------------------------------------
# Scenario steps
# ---------------------------------------------------------------------------

def show_acceptance_gate(agents: Dict[str, Agent]) -> None:
    """Show the >=50% collateralization gate before any manager reserve exists."""
    adam, bryn = agents["adam"], agents["bryn"]
    print("\n[GATE] Cross-team acceptance check BEFORE adam's reserve exists:")
    check = acceptance_check(adam.pubkey, bryn.pubkey, CROSS_COMPUTE_PRICE)
    print(f"  adam -> bryn {erg(CROSS_COMPUTE_PRICE)} ERG: "
          f"acceptable={check['acceptable']} (reason: {check.get('reason')})")
    if check["acceptable"]:
        print("  [warn] expected rejection here — policy fails closed without a reserve")


def show_acceptance_gate_after(agents: Dict[str, Agent]) -> None:
    adam, bryn = agents["adam"], agents["bryn"]
    print("\n[GATE] Cross-team acceptance check AFTER manager reserves are on-chain:")
    check = acceptance_check(adam.pubkey, bryn.pubkey, CROSS_COMPUTE_PRICE)
    print(f"  adam -> bryn {erg(CROSS_COMPUTE_PRICE)} ERG: "
          f"acceptable={check['acceptable']} (reason: {check.get('reason')})")
    if not check["acceptable"]:
        raise RuntimeError(
            "cross-team payment still rejected after reserves were created — "
            "the >=50% collateralization requirement is not met"
        )


def work_round(agents: Dict[str, Agent]) -> None:
    print("\n" + "=" * 64)
    print("WORK ROUND — managers delegate subtasks, teams trade services")
    print("=" * 64)
    print("\n[DELEGATION] judy's task: 'analyze the dataset and publish the result'")
    for team_name, team in TEAMS.items():
        manager = team["manager"]
        print(f"  team {team_name}: {manager} splits the task into:")
        print(f"    - compute subtask -> {team['workers']['compute']}")
        print(f"    - storage subtask -> {team['workers']['storage']}")

    # Intra-team pure credit.
    for team_name, team in TEAMS.items():
        manager = agents[team["manager"]]
        compute = agents[team["workers"]["compute"]]
        storage = agents[team["workers"]["storage"]]
        issue_note(manager, compute, COMPUTE_PRICE,
                   f"[pure credit] {compute.name} runs the compute subtask for team {team_name}")
        issue_note(manager, storage, STORAGE_PRICE,
                   f"[pure credit] {storage.name} stores the dataset for team {team_name}")

    # Cross-team >=50%-collateralized credit.
    adam, bella = agents["adam"], agents["bella"]
    bryn, alex = agents["bryn"], agents["alex"]
    issue_note(adam, bryn, CROSS_COMPUTE_PRICE,
               "[>=50% backed] Alpha buys extra compute capacity from Beta")
    issue_note(bella, alex, CROSS_STORAGE_PRICE,
               "[>=50% backed] Beta buys backup storage from Alpha")


def judge(agents: Dict[str, Agent], auto: bool) -> str:
    """Human judge evaluates both deliverables; returns the winning team name."""
    print("\n" + "=" * 64)
    print("JUDGING — human evaluation")
    print("=" * 64)
    print("\nDeliverable metrics (simulated):")
    print(f"  {'team':<8} {'completeness':>14} {'latency (s)':>12} {'storage proof':>14}")
    print(f"  {'alpha':<8} {'96%':>14} {'4.2':>12} {'ok':>14}")
    print(f"  {'beta':<8} {'89%':>14} {'6.8':>12} {'ok':>14}")

    if auto:
        print("\n[JUDGE] --auto mode: judy picks team alpha")
        return "alpha"

    while True:
        choice = input("\n[JUDGE] Which team wins? [alpha/beta]: ").strip().lower()
        if choice in TEAMS:
            return choice
        print("  please enter 'alpha' or 'beta'")


def pay_prize(agents: Dict[str, Agent], winner: str) -> None:
    judy = agents[JUDGE]
    manager = agents[TEAMS[winner]["manager"]]
    print(f"\n[PRIZE] judy rewards team {winner}: backed money for {manager.name}")
    result = issue_note(judy, manager, PRIZE,
                        f"[fully backed] prize for winning team {winner}")
    before = result.get("reserve_status_before", {})
    after = result.get("reserve_status_after", {})
    print("  backing proof (judy's reserve):")
    print(f"    before: collateral {erg(before.get('collateral', 0))} ERG, "
          f"total debt {erg(before.get('total_debt', 0))} ERG")
    print(f"    after:  collateral {erg(after.get('collateral', 0))} ERG, "
          f"total debt {erg(after.get('total_debt', 0))} ERG")


def redeem_prize(agents: Dict[str, Agent], winner: str) -> None:
    judy = agents[JUDGE]
    manager = agents[TEAMS[winner]["manager"]]
    print(f"\n[REDEEM] {manager.name} redeems {erg(REDEEM_AMOUNT)} ERG of the prize "
          f"on-chain from judy's reserve")
    try:
        result = manager.client.call_tool(
            "note_redeem", {"issuer": judy.pubkey, "amount": REDEEM_AMOUNT},
            timeout=120.0,
        )
        print(f"  redeemed -> tx_id {result.get('tx_id')}")
        print("  backed credit converted to real ERG.")
    except Exception as exc:
        print(f"  [warn] redemption not completed: {exc}")
        print("  (the tracker box update may not have enough confirmations yet — "
              "try `basis-cli note redeem` later)")


def pay_bonuses(agents: Dict[str, Agent], winner: str) -> None:
    team = TEAMS[winner]
    manager = agents[team["manager"]]
    compute = agents[team["workers"]["compute"]]
    storage = agents[team["workers"]["storage"]]
    print(f"\n[BONUS] {manager.name}, now holding backed money, pays completion bonuses")
    issue_note(manager, compute, BONUS_COMPUTE_TOTAL,
               "compute bonus (cumulative: subtask + bonus)")
    issue_note(manager, storage, BONUS_STORAGE_TOTAL,
               "storage bonus (cumulative: subtask + bonus)")


# ---------------------------------------------------------------------------
# Reports
# ---------------------------------------------------------------------------

def print_balance_sheet(agents: Dict[str, Agent]) -> None:
    print("\n" + "=" * 64)
    print("FINAL BALANCE SHEET")
    print("=" * 64)
    print(f"{'Agent':<10} {'Assets (ERG)':>14} {'Liabilities (ERG)':>18} {'Net (ERG)':>14}")
    print("-" * 64)

    totals: Dict[str, List[int]] = {}
    nets = []
    for name in AGENTS:
        agent = agents[name]
        issued = list_notes(agent, "issued")
        received = list_notes(agent, "received")

        liabilities = sum(n["amount"] - n.get("redeemed", 0) for n in issued)
        assets = sum(n["amount"] - n.get("redeemed", 0) for n in received)
        net = assets - liabilities
        nets.append(net)
        totals[name] = [assets, liabilities, net]
        print(f"{name:<10} {erg(assets):>14} {erg(liabilities):>18} {erg(net):>14}")

    print("-" * 64)
    for team_name, team in TEAMS.items():
        members = [team["manager"], *team["workers"].values()]
        t_assets = sum(totals[m][0] for m in members)
        t_liab = sum(totals[m][1] for m in members)
        t_net = sum(totals[m][2] for m in members)
        print(f"team {team_name:<5} {erg(t_assets):>14} {erg(t_liab):>18} {erg(t_net):>14}")
    print("-" * 64)
    if sum(nets) == 0:
        print("Balance sheet checks out: net positions sum to zero.")
    else:
        print("WARNING: net positions do not sum to zero (this should not happen).")


def print_collateralization_report(agents: Dict[str, Agent]) -> None:
    print("\n[COLLATERALIZATION]")
    tiers = {JUDGE: "fully backed (judge)", "adam": ">=50% backed (cross-team)",
             "bella": ">=50% backed (cross-team)"}
    for name, tier in tiers.items():
        agent = agents[name]
        status = agent.client.call_tool("reserve_status", {"pubkey": agent.pubkey})
        if not status:
            print(f"  {name:<8} {tier:<28} no reserve found")
            continue
        ratio = status.get("collateralization_ratio", 0.0)
        print(f"  {name:<8} {tier:<28} collateral {erg(status.get('collateral', 0))} ERG, "
              f"total debt {erg(status.get('total_debt', 0))} ERG, ratio {ratio:.2f}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def run_scenario(auto: bool) -> None:
    print("=" * 64)
    print("Basis Agent Teams Demo — Pure Credit, 50%-Backed Credit, Backed Money")
    print("=" * 64)
    print(f"Tracker server: {SERVER_URL}")
    print(f"basis-mcp:      {BASIS_MCP}")

    missing = [name for name, nft in RESERVE_NFTS.items() if not nft]
    if missing:
        raise RuntimeError(
            "missing reserve NFT ids for: " + ", ".join(missing) +
            " — set JUDGE_RESERVE_NFT_ID / ADAM_RESERVE_NFT_ID / BELLA_RESERVE_NFT_ID "
            "(see demo/agent_teams/README.md)"
        )

    # 1. Bootstrap agents.
    agents = {name: bootstrap_agent(name) for name in AGENTS}

    try:
        # 2. Publish acceptance policies (needed for the gate demonstration).
        publish_policies(agents)

        # 3. Show the >=50% gate failing closed before reserves exist.
        show_acceptance_gate(agents)

        # 4. Create on-chain reserves (judge + both managers).
        for name, amount in [(JUDGE, JUDGE_RESERVE),
                             ("adam", MANAGER_RESERVE),
                             ("bella", MANAGER_RESERVE)]:
            create_reserve(agents[name], RESERVE_NFTS[name], amount)

        # 5. Show the gate now accepting the cross-team payment.
        show_acceptance_gate_after(agents)

        # 6. Work round: intra-team pure credit + cross-team backed credit.
        work_round(agents)

        # 7. Human judge evaluates; 8. backed prize; 9. redemption; 10. bonuses.
        winner = judge(agents, auto)
        pay_prize(agents, winner)
        redeem_prize(agents, winner)
        pay_bonuses(agents, winner)

        # 11. Reports.
        print_balance_sheet(agents)
        print_collateralization_report(agents)

        print("\nDemo complete.")

    finally:
        print("\n[SHUTDOWN] Stopping agent MCP processes...")
        for agent in agents.values():
            agent.client.stop()
        print("Done.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Basis Agent Teams demo orchestrator")
    parser.add_argument("--auto", action="store_true",
                        help="non-interactive mode: scripted judge decision (team alpha)")
    args = parser.parse_args()
    try:
        run_scenario(auto=args.auto or not sys.stdin.isatty())
    except Exception as exc:
        print(f"\n[DEMO FAILED] {exc}", file=sys.stderr)
        sys.exit(1)
