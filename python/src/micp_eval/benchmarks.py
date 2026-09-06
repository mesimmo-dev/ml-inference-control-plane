"""Fleet generators used as fixtures for routing/allocation sweeps."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class Candidate:
    model_id: str
    latency_p99_ms: float
    quality: float
    cost_per_1k_tokens: float
    capacity_rps: float
    error_rate: float


def generate_fleet(n: int, seed: int = 0) -> list[Candidate]:
    """Draw `n` plausible serving profiles.

    Ranges are chosen to overlap typical interactive SLO budgets so a
    random fleet is neither uniformly feasible nor uniformly rejected.
    """
    if n <= 0:
        raise ValueError("n must be positive")
    rng = np.random.default_rng(seed)
    fleet: list[Candidate] = []
    for i in range(n):
        fleet.append(
            Candidate(
                model_id=f"m{i:02d}",
                latency_p99_ms=float(rng.uniform(40.0, 400.0)),
                quality=float(rng.uniform(0.60, 0.98)),
                cost_per_1k_tokens=float(rng.uniform(0.01, 0.80)),
                capacity_rps=float(rng.uniform(5.0, 150.0)),
                error_rate=float(rng.uniform(0.001, 0.03)),
            )
        )
    return fleet
