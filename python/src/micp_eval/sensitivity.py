"""One-at-a-time sensitivity of the recommended route to constraint moves."""

from __future__ import annotations

from typing import Any

from .artifacts import experiment_id, flatten_plan, utc_now
from .engine import Engine
from .experiments import evaluate_one
from .metadata import environment_info
from .patch import apply_patches, get_path
from .sweeps import SweepAxis, SweepSpec, iter_sweep_points

# Named axes. Values are relative recipes applied to the scenario baseline.
SENSITIVITY_AXES: dict[str, str] = {
    "latency_slo": "workload.constraints.latency_slo",
    "mean_rps": "workload.traffic.mean_rps",
    "cost_ceiling": "workload.constraints.cost_ceiling_per_request",
    "quality_floor": "workload.constraints.quality_floor",
    "context_budget": "workload.retrieval.context_budget",
    "top_k": "workload.retrieval.top_k",
    "degraded_factor": "fleet.*.capacity.degraded_factor",
}


def default_values(axis: str, baseline: float | int) -> list[Any]:
    """Bounded, explicit grid around a baseline. Not an adaptive search."""
    b = float(baseline)
    if axis == "latency_slo":
        return _unique([max(b * x, 1.0) for x in (0.10, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0)])
    if axis == "mean_rps":
        return _unique([max(b * x, 0.5) for x in (0.5, 1.0, 1.5, 2.0, 4.0, 8.0)])
    if axis == "cost_ceiling":
        return _unique([max(b * x, 1e-6) for x in (0.25, 0.5, 0.75, 1.0, 2.0)])
    if axis == "quality_floor":
        return _unique([min(max(x, 0.0), 0.99) for x in (0.5, 0.6, 0.7, 0.8, 0.85, 0.9, 0.95)])
    if axis == "context_budget":
        return _unique([max(int(b * x), 0) for x in (0.0, 0.25, 0.5, 1.0, 2.0)])
    if axis == "top_k":
        base = max(int(b), 0)
        return _unique([max(k, 0) for k in (0, 1, 4, 8, base, max(base, 1) * 2, 32)])
    if axis == "degraded_factor":
        return [0.0, 0.25, 0.4, 0.6, 1.0]
    raise KeyError(axis)


def _unique(xs: list[Any]) -> list[Any]:
    out: list[Any] = []
    for x in xs:
        if x not in out:
            out.append(x)
    return out


def baseline_value(scenario: dict[str, Any], path: str) -> Any:
    if path.endswith("degraded_factor") or path.startswith("fleet.*"):
        fleet = scenario.get("fleet") or []
        if not fleet:
            return 1.0
        return (fleet[0].get("capacity") or {}).get("degraded_factor", 1.0)
    try:
        return get_path(scenario, path)
    except (KeyError, ValueError):
        return None


def run_sensitivity(
    engine: Engine,
    scenario_id: str,
    *,
    axes: list[str] | None = None,
    max_points: int = 64,
) -> dict[str, Any]:
    scenario = engine.get_scenario(scenario_id)
    names = list(axes or SENSITIVITY_AXES.keys())
    blocks: list[dict[str, Any]] = []
    all_rows: list[dict[str, Any]] = []
    for name in names:
        path = SENSITIVITY_AXES[name]
        base_val = baseline_value(scenario, path)
        if base_val is None:
            continue
        values = default_values(name, base_val)
        spec = SweepSpec(
            name=f"sensitivity:{name}",
            scenario_id=scenario_id,
            axes=(SweepAxis(path, tuple(values)),),
            max_points=max_points,
        )
        rows: list[dict[str, Any]] = []
        for i, patch in enumerate(iter_sweep_points(spec)):
            outcome = evaluate_one(engine, apply_patches(scenario, patch))
            value = next(iter(patch.values()))
            row = {
                "axis": name,
                "path": path,
                "index": i,
                "value": value,
                "status": outcome.status,
                **flatten_plan(outcome.plan),
            }
            rows.append(row)
            all_rows.append(row)
        blocks.append(
            {
                "axis": name,
                "path": path,
                "baseline": base_val,
                "values": values,
                "transitions": transitions(rows),
                "infeasibility_boundary": infeasibility_boundary(rows),
                "saturation_threshold": saturation_threshold(rows),
                "points": rows,
            }
        )
    eid = experiment_id("sensitivity", {"scenario_id": scenario_id, "axes": names})
    return {
        "experiment_id": eid,
        "kind": "sensitivity",
        "timestamp": utc_now(),
        "scenario_id": scenario_id,
        "seed": 0,
        "input": {"axes": names},
        "blocks": blocks,
        "points": all_rows,
        "validation": {"passed": True, "failures": []},
        "environment": environment_info(),
        "origin": "modeled",
    }


def transitions(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Recommendation (or feasibility) changes between adjacent axis values."""
    out: list[dict[str, Any]] = []
    prev: dict[str, Any] | None = None
    for row in rows:
        if prev is None:
            prev = row
            continue
        if row.get("recommended_key") != prev.get("recommended_key") or row.get("feasible") != prev.get("feasible"):
            out.append(
                {
                    "from_value": prev.get("value"),
                    "to_value": row.get("value"),
                    "from_key": prev.get("recommended_key"),
                    "to_key": row.get("recommended_key"),
                    "from_feasible": prev.get("feasible"),
                    "to_feasible": row.get("feasible"),
                }
            )
        prev = row
    return out


def infeasibility_boundary(rows: list[dict[str, Any]]) -> dict[str, Any] | None:
    """First point, in axis order, that becomes infeasible."""
    for row in rows:
        if not row.get("feasible"):
            return {"value": row.get("value"), "status": row.get("status")}
    return None


def saturation_threshold(rows: list[dict[str, Any]]) -> dict[str, Any] | None:
    for row in rows:
        if row.get("saturated"):
            return {"value": row.get("value"), "utilization": row.get("utilization")}
    return None
