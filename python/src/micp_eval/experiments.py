"""Experiment orchestration: evaluate, sweep, simulate, record artifacts."""

from __future__ import annotations

from typing import Any

import numpy as np

from . import artifacts, invariants
from .artifacts import flatten_plan
from .engine import SCENARIO_IDS, Engine, PlanOutcome
from .metadata import environment_info, git_sha
from .patch import apply_patches
from .stats import summarize
from .sweeps import SweepSpec, iter_sweep_points


def evaluate_one(engine: Engine, scenario: dict[str, Any]) -> PlanOutcome:
    return engine.evaluate(scenario, allow_infeasible=True)


def run_scenario(engine: Engine, scenario_id: str, *, require_invariants: bool = False) -> dict[str, Any]:
    scenario = engine.get_scenario(scenario_id)
    outcome = evaluate_one(engine, scenario)
    plan = outcome.plan
    failures = invariants.check_plan(plan, scenario.get("workload")) if plan else []
    record = {
        "experiment_id": artifacts.experiment_id("evaluate", {"scenario_id": scenario_id, "git": git_sha()}),
        "kind": "evaluate",
        "timestamp": artifacts.utc_now(),
        "scenario_id": scenario_id,
        "seed": None,
        "input": {
            "scenario_id": scenario_id,
            "assumptions": scenario.get("assumptions", []),
            "weights": scenario.get("weights"),
        },
        "inventory": [{"id": c.get("id")} for c in scenario.get("fleet") or []],
        "result": {
            "status": outcome.status,
            "error": outcome.error,
            "origin": "modeled",
            **flatten_plan(plan),
            "plan": plan,
        },
        "validation": {"failures": failures, "passed": not failures},
        "environment": environment_info(),
    }
    if require_invariants:
        invariants.assert_invariants(failures, context=scenario_id)
    return record


def run_all_scenarios(engine: Engine, *, require_invariants: bool = False) -> list[dict[str, Any]]:
    ids = [c["id"] for c in engine.list_scenarios()] or list(SCENARIO_IDS)
    return [run_scenario(engine, sid, require_invariants=require_invariants) for sid in ids]


def run_sweep(engine: Engine, spec: SweepSpec) -> dict[str, Any]:
    base = engine.get_scenario(spec.scenario_id)
    points = iter_sweep_points(spec)
    rows: list[dict[str, Any]] = []
    for i, patch in enumerate(points):
        scenario = apply_patches(base, patch)
        outcome = evaluate_one(engine, scenario)
        plan = outcome.plan
        row = {
            "index": i,
            "patches": patch,
            "status": outcome.status,
            **{f"p_{k.replace('.', '_')}": v for k, v in patch.items()},
            **flatten_plan(plan),
        }
        rows.append(row)
    eid = artifacts.experiment_id(
        "sweep",
        {
            "name": spec.name,
            "scenario_id": spec.scenario_id,
            "axes": [{"path": a.path, "values": list(a.values)} for a in spec.axes],
            "max_points": spec.max_points,
            "seed": spec.seed,
            "n": len(points),
        },
    )
    return {
        "experiment_id": eid,
        "kind": "sweep",
        "timestamp": artifacts.utc_now(),
        "scenario_id": spec.scenario_id,
        "seed": spec.seed,
        "input": {
            "name": spec.name,
            "axes": [{"path": a.path, "values": list(a.values)} for a in spec.axes],
            "max_points": spec.max_points,
            "n_points": len(points),
        },
        "points": rows,
        "validation": {"passed": True, "failures": []},
        "environment": environment_info(),
        "origin": "modeled",
    }


def run_simulations(
    engine: Engine,
    scenario_id: str,
    *,
    model_id: str | None = None,
    seeds: list[int] | None = None,
    duration_s: float = 2.0,
    long_tail: bool = False,
) -> dict[str, Any]:
    """Seeded Monte Carlo over the Rust G/G/n simulator.

    Summaries are simulation-derived, not empirical production intervals.
    """
    scenario = engine.get_scenario(scenario_id)
    seeds = list(seeds or [1, 2, 3, 5, 8])
    reports = [
        engine.simulate(scenario, model_id=model_id, seed=s, duration_s=duration_s, long_tail=long_tail) for s in seeds
    ]
    p99 = [float(r["p99_ms"]) for r in reports]
    served = [float(r["n_served"]) for r in reports]
    a = engine.simulate(scenario, model_id=model_id, seed=seeds[0], duration_s=duration_s, long_tail=long_tail)
    b = engine.simulate(scenario, model_id=model_id, seed=seeds[0], duration_s=duration_s, long_tail=long_tail)
    repro = invariants.check_sim_reproducible(a, b)
    invariants.assert_invariants(repro, context="sim-repro")
    eid = artifacts.experiment_id(
        "simulate",
        {"scenario_id": scenario_id, "model_id": model_id, "seeds": seeds, "duration_s": duration_s},
    )
    return {
        "experiment_id": eid,
        "kind": "simulate",
        "timestamp": artifacts.utc_now(),
        "scenario_id": scenario_id,
        "seed": seeds[0],
        "input": {
            "model_id": model_id,
            "seeds": seeds,
            "duration_s": duration_s,
            "long_tail": long_tail,
        },
        "origin": "simulated",
        "reports": reports,
        "summary": {
            "p99_ms": summarize(np.asarray(p99, dtype=float), seed=seeds[0]),
            "n_served": summarize(np.asarray(served, dtype=float), seed=seeds[0]),
            "label": "simulation-derived uncertainty across seeds; not production telemetry",
        },
        "validation": {"passed": True, "failures": []},
        "environment": environment_info(),
    }


def recommended_key(record: dict[str, Any]) -> str | None:
    result = record.get("result") or {}
    return result.get("recommended_key")
