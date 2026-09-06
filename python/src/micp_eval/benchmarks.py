"""Fleet fixtures and local engineering benchmarks.

Numbers produced here are **local synthetic/engineering timings**, not
production performance claims. They include the environment metadata
needed to interpret them.
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Any

import numpy as np

from .engine import SCENARIO_IDS, Engine
from .metadata import environment_info
from .sweeps import SweepAxis, SweepSpec, iter_sweep_points


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


def _time_call(fn: Any, rounds: int) -> dict[str, float]:
    samples: list[float] = []
    fn()  # warmup
    for _ in range(rounds):
        t0 = time.perf_counter()
        fn()
        samples.append((time.perf_counter() - t0) * 1000.0)
    arr = np.asarray(samples, dtype=float)
    return {
        "rounds": float(rounds),
        "mean_ms": float(arr.mean()),
        "median_ms": float(np.median(arr)),
        "p95_ms": float(np.percentile(arr, 95)),
        "min_ms": float(arr.min()),
        "max_ms": float(arr.max()),
    }


def local_bench(engine: Engine, *, rounds: int = 5) -> dict[str, Any]:
    """Time engine round-trips in this process. Local synthetic only."""
    timings: dict[str, Any] = {}
    cards = engine.list_scenarios()
    timings["list_scenarios"] = _time_call(engine.list_scenarios, rounds)
    ids = [str(c["id"]) for c in cards] or list(SCENARIO_IDS)
    first = engine.get_scenario(ids[0])
    for sid in ids:
        scenario = engine.get_scenario(sid)
        timings[f"evaluate:{sid}"] = _time_call(lambda s=scenario: engine.evaluate(s, allow_infeasible=True), rounds)

    spec = SweepSpec(
        name="bench-rps",
        scenario_id=ids[0],
        axes=(SweepAxis("workload.traffic.mean_rps", (5.0, 15.0, 30.0, 60.0, 120.0)),),
        max_points=8,
    )
    points = iter_sweep_points(spec)

    def _sweep() -> None:
        from .patch import apply_patches

        for patch in points:
            engine.evaluate(apply_patches(first, patch), allow_infeasible=True)

    timings["sweep_mean_rps_5pts"] = _time_call(_sweep, max(1, rounds // 2))
    model_id = None
    fleet = first.get("fleet") or []
    if fleet:
        model_id = fleet[0].get("id")
    timings["simulate_interactive"] = _time_call(
        lambda: engine.simulate(first, model_id=model_id, seed=1, duration_s=1.0),
        max(1, rounds // 2),
    )

    growing: list[dict[str, Any]] = []
    for n in (4, 8, 16, 32, 64):
        t0 = time.perf_counter()
        _python_pareto_cost(n, seed=0)
        growing.append({"n": n, "ms": (time.perf_counter() - t0) * 1000.0})
    timings["python_pareto_growth"] = growing

    return {
        "kind": "bench",
        "label": "local synthetic/engineering timings; not a production performance claim",
        "environment": environment_info(),
        "timings": timings,
    }


def _python_pareto_cost(n: int, seed: int) -> int:
    """Python-only stand-in used to watch front extraction grow with n.

    This does **not** time the Rust optimizer. It exists so a laptop run
    can still plot an O(n²) curve without compiling Criterion.
    """
    rng = np.random.default_rng(seed)
    pts = rng.random((n, 4))
    front = 0
    for i in range(n):
        dominated = False
        for j in range(n):
            if i == j:
                continue
            if (pts[j] <= pts[i] + 1e-12).all() and (pts[j] < pts[i] - 1e-12).any():
                dominated = True
                break
        if not dominated:
            front += 1
    return front
