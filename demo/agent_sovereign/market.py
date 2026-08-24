#!/usr/bin/env python3
"""
Deterministic service market for the sovereign agent demo.

Models the Celaut separation of concerns:

  * infra_compute   — node maintainer selling metered compute in tiers
  * skill_vendor    — developer selling capability packs
  * bounty_board    — escrow/judge posting deterministically verifiable tasks

Tasks are verifiable by construction: the expected deliverable hash is
sha256(task_id || input_bytes || class_salt). The same function is used by the
sovereign agent to produce its deliverable and by the board to verify it, so a
correct execution always verifies and a lazy or broken one never does.
"""

import hashlib
import time
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional


# ---------------------------------------------------------------------------
# Price catalog (raw USE units, 3 decimals)
# ---------------------------------------------------------------------------

COMPUTE_V1_UNIT_PRICE = 25     # per compute unit, tier v1
COMPUTE_V2_UNIT_PRICE = 60     # per compute unit, tier v2 (better model)
SKILL_PACK_PRICE = 120         # one-time "premium-markets" pack

BOUNTY_BASIC = 90              # bounty for a verified basic task
BOUNTY_PREMIUM = 300           # bounty for a verified premium task


@dataclass(frozen=True)
class ComputeTier:
    """A metered compute tier sold by the infra agent."""

    name: str
    unit_price: int

    def cost(self, units: int, multiplier: float = 1.0) -> int:
        return int(round(self.unit_price * units * multiplier))


COMPUTE_TIERS: Dict[str, ComputeTier] = {
    "v1": ComputeTier("v1", COMPUTE_V1_UNIT_PRICE),
    "v2": ComputeTier("v2", COMPUTE_V2_UNIT_PRICE),
}


@dataclass(frozen=True)
class SkillPack:
    """A one-time capability purchase from the skill vendor."""

    name: str
    price: int
    unlocks_task_class: str


SKILL_PACKS: Dict[str, SkillPack] = {
    "premium-markets": SkillPack("premium-markets", SKILL_PACK_PRICE, "premium"),
}


@dataclass(frozen=True)
class TaskClass:
    """A bounty task class posted by the board."""

    name: str
    bounty: int
    compute_units: int          # units consumed per execution
    required_tier: str          # minimum compute tier
    required_skill: Optional[str]  # skill pack needed, if any
    salt: str                   # class salt mixed into verification hashes


TASK_CLASSES: Dict[str, TaskClass] = {
    "basic": TaskClass("basic", BOUNTY_BASIC, 1, "v1", None, "basic-salt-v1"),
    "premium": TaskClass("premium", BOUNTY_PREMIUM, 3, "v2", "premium-markets",
                         "premium-salt-v2"),
}


def task_classes_for(tier: str, skills: set) -> List[TaskClass]:
    """Task classes executable with the given compute tier and skills."""
    order = ["v1", "v2"]
    out = []
    for cls in TASK_CLASSES.values():
        if order.index(cls.required_tier) > order.index(tier):
            continue
        if cls.required_skill is not None and cls.required_skill not in skills:
            continue
        out.append(cls)
    return out


# ---------------------------------------------------------------------------
# Deterministic execution and verification
# ---------------------------------------------------------------------------

@dataclass
class Task:
    """A bounty task posted by the board."""

    task_id: str
    task_class: str
    input_hex: str

    def input_bytes(self) -> bytes:
        return bytes.fromhex(self.input_hex)


@dataclass
class Deliverable:
    task_id: str
    output_hash: str
    execution_time_ms: int
    compute_tier: str


def post_task(task_class: str, nonce: str) -> Task:
    """Board posts a new task with deterministic pseudo-random input."""
    digest = hashlib.sha256(f"{task_class}:{nonce}".encode()).digest()
    return Task(
        task_id=hashlib.sha256(f"id:{task_class}:{nonce}".encode()).hexdigest()[:16],
        task_class=task_class,
        input_hex=digest.hex(),
    )


def execute_task(task: Task, tier: str) -> Deliverable:
    """
    Sovereign agent executes a task on its rented compute.

    The computation is deterministic: sha256(task_id || input || class_salt),
    performed inside an isolated temp directory to mirror Celaut BOX isolation.
    """
    cls = TASK_CLASSES[task.task_class]
    work_dir = Path(f"/tmp/sovereign_demo_{task.task_id}_{uuid.uuid4().hex}")
    work_dir.mkdir(parents=True, exist_ok=True)
    try:
        start = time.perf_counter()
        hasher = hashlib.sha256()
        hasher.update(task.task_id.encode())
        hasher.update(task.input_bytes())
        hasher.update(cls.salt.encode())
        output_hash = hasher.hexdigest()
        (work_dir / "output.txt").write_text(output_hash)
        elapsed_ms = int((time.perf_counter() - start) * 1000)
        return Deliverable(task.task_id, output_hash, elapsed_ms, tier)
    finally:
        for child in work_dir.iterdir():
            child.unlink()
        work_dir.rmdir()


def verify_deliverable(task: Task, deliverable: Deliverable,
                       claimed_tier: str) -> bool:
    """
    Board-side verification. Recomputes the expected hash and checks both the
    result and that the claimed compute tier satisfies the task class.
    """
    if deliverable.task_id != task.task_id:
        return False
    if TASK_CLASSES[task.task_class].required_tier == "v2" and claimed_tier != "v2":
        return False
    return deliverable.output_hash == expected_output_hash(task)


def expected_output_hash(task: Task) -> str:
    cls = TASK_CLASSES[task.task_class]
    hasher = hashlib.sha256()
    hasher.update(task.task_id.encode())
    hasher.update(task.input_bytes())
    hasher.update(cls.salt.encode())
    return hasher.hexdigest()
