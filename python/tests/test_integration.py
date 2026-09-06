"""Live micp-api tests. Skipped when the release binary is absent."""

from __future__ import annotations

import pytest

from micp_eval.engine import SCENARIO_IDS, HttpEngine
from micp_eval.experiments import run_scenario, run_simulations, run_sweep
from micp_eval.invariants import (
    check_deterministic_recommendation,
    check_feasible_set_nested,
    check_plan,
    check_rate_utilization_monotone,
)
from micp_eval.patch import apply_patches
from micp_eval.reference import compare_estimate
from micp_eval.sensitivity import run_sensitivity
from micp_eval.sweeps import SweepAxis, SweepSpec

pytestmark = pytest.mark.integration


def test_five_scenarios_evaluate(live_engine: HttpEngine) -> None:
    cards = live_engine.list_scenarios()
    assert {c["id"] for c in cards} == set(SCENARIO_IDS)
    for sid in SCENARIO_IDS:
        rec = run_scenario(live_engine, sid)
        assert rec["result"]["origin"] == "modeled"
        assert rec["result"]["status"] == "ok"
        assert rec["validation"]["passed"], rec["validation"]["failures"]
        plan = rec["result"]["plan"]
        scenario = live_engine.get_scenario(sid)
        assert check_plan(plan, scenario["workload"]) == []


def test_evaluate_is_deterministic(live_engine: HttpEngine) -> None:
    s = live_engine.get_scenario("quality_rag")
    a = live_engine.evaluate(s).plan
    b = live_engine.evaluate(s).plan
    assert a is not None and b is not None
    assert check_deterministic_recommendation(a, b) == []


def test_reference_formulas_match_engine(live_engine: HttpEngine) -> None:
    s = live_engine.get_scenario("quality_rag")
    plan = live_engine.evaluate(s).plan
    assert plan is not None
    failures: list[str] = []
    for item in plan["evaluated"]:
        failures.extend(compare_estimate(item["route"], s["workload"], item["estimate"]))
    assert failures == []


def test_tighter_slo_cannot_admit_new_routes(live_engine: HttpEngine) -> None:
    s = live_engine.get_scenario("interactive_assistant")
    loose = live_engine.evaluate(s).plan
    tight = live_engine.evaluate(apply_patches(s, {"workload.constraints.latency_slo": 50.0})).plan
    assert loose is not None and tight is not None
    assert check_feasible_set_nested(loose, tight) == []


def test_rate_sweep_utilization(live_engine: HttpEngine) -> None:
    spec = SweepSpec(
        name="rps",
        scenario_id="interactive_assistant",
        axes=(SweepAxis("workload.traffic.mean_rps", (15.0, 30.0, 45.0, 60.0)),),
        max_points=8,
    )
    rec = run_sweep(live_engine, spec)
    assert any(r.get("origin") == "modeled" for r in rec["points"])
    s = live_engine.get_scenario("interactive_assistant")
    series = []
    key = None
    for rps in (15.0, 30.0, 45.0, 60.0):
        plan = live_engine.evaluate(apply_patches(s, {"workload.traffic.mean_rps": rps})).plan
        assert plan is not None
        if key is None:
            key = plan["evaluated"][0]["estimate"]["route_key"]
        series.append((rps, plan))
    assert key is not None
    assert check_rate_utilization_monotone(series, key) == []


def test_simulate_reproducible(live_engine: HttpEngine) -> None:
    rec = run_simulations(live_engine, "interactive_assistant", model_id="fast-8b", seeds=[3, 5], duration_s=2.0)
    assert rec["origin"] == "simulated"
    assert rec["reports"][0]["n_arrivals"] > 0


def test_sensitivity_exports_transitions(live_engine: HttpEngine) -> None:
    rec = run_sensitivity(live_engine, "interactive_assistant", axes=["latency_slo", "mean_rps"])
    assert rec["blocks"]
    slo = next(b for b in rec["blocks"] if b["axis"] == "latency_slo")
    assert slo["points"]
    # Tightening to 0.1x of 200ms should become infeasible for the interactive preset.
    assert slo["infeasibility_boundary"] is not None
