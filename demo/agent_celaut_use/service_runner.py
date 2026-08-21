#!/usr/bin/env python3
"""
Mock Celaut deterministic service runner.

Celaut services are deterministic software containers: given the same inputs,
they always produce the same outputs. This module simulates that property by
running a pure Python transformation inside an isolated temporary directory
(representing the Celaut BOX filesystem).

No Docker or real Celaut nodo integration is used in this demo.
"""

import hashlib
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Dict


@dataclass
class ServiceSpec:
    """Minimal Celaut service specification."""

    name: str
    box: str  # execution environment description
    api: str  # interface description
    net: str  # network scope
    price_use: int  # price in raw USE token units (6 decimals)

    def to_dict(self) -> Dict[str, object]:
        return {
            "name": self.name,
            "box": self.box,
            "api": self.api,
            "net": self.net,
            "price_use": self.price_use,
        }


@dataclass
class ExecutionResult:
    """Result of a deterministic service execution."""

    service_name: str
    input_hex: str
    output_hash: str
    execution_time_ms: int
    price_use: int


def execute(service: ServiceSpec, input_bytes: bytes) -> ExecutionResult:
    """
    Execute a service deterministically.

    The "computation" is a SHA-256 of (service name || input bytes || service
    price), which is deterministic for the same inputs. The work is performed
    inside an isolated temp directory to simulate Celaut BOX isolation.
    """
    # Simulate Celaut BOX isolation: create a temp working directory.
    work_dir = Path(f"/tmp/celaut_demo_{service.name}_{uuid.uuid4().hex}")
    work_dir.mkdir(parents=True, exist_ok=True)
    try:
        input_path = work_dir / "input.dat"
        output_path = work_dir / "output.dat"
        input_path.write_bytes(input_bytes)

        start = time.perf_counter()
        # Deterministic transformation: SHA-256(service name + input + price)
        hasher = hashlib.sha256()
        hasher.update(service.name.encode())
        hasher.update(input_bytes)
        hasher.update(str(service.price_use).encode())
        output_hash = hasher.hexdigest()
        output_path.write_text(output_hash)
        elapsed_ms = int((time.perf_counter() - start) * 1000)

        return ExecutionResult(
            service_name=service.name,
            input_hex=input_bytes.hex(),
            output_hash=output_hash,
            execution_time_ms=elapsed_ms,
            price_use=service.price_use,
        )
    finally:
        # Clean up the isolated BOX.
        for child in work_dir.iterdir():
            child.unlink()
        work_dir.rmdir()
