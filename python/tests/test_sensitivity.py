from __future__ import annotations

from micp_eval.sensitivity import (
    default_values,
    infeasibility_boundary,
    run_sensitivity,
    transitions,
)

from fakes import FakeEngine


def test_default_values_keep_budget() -> None:
    vs = default_values("mean_rps", 15.0)
    assert 15.0 in vs
    assert vs[0] < vs[-1]
    q = default_values("quality_floor", 0.65)
    assert all(0.0 <= x <= 0.99 for x in q)


def test_transitions_detect_key_change() -> None:
    rows = [
        {"value": 1, "recommended_key": "a", "feasible": True},
        {"value": 2, "recommended_key": "a", "feasible": True},
        {"value": 3, "recommended_key": "b", "feasible": True},
        {"value": 4, "recommended_key": None, "feasible": False},
    ]
    t = transitions(rows)
    assert len(t) == 2
    assert t[0]["from_key"] == "a" and t[0]["to_key"] == "b"
    bound = infeasibility_boundary(rows)
    assert bound is not None
    assert bound["value"] == 4


def test_sensitivity_mean_rps_via_fake() -> None:
    rec = run_sensitivity(FakeEngine(), "interactive_assistant", axes=["mean_rps", "latency_slo"])
    assert rec["kind"] == "sensitivity"
    names = [b["axis"] for b in rec["blocks"]]
    assert names == ["mean_rps", "latency_slo"]
    slo = next(b for b in rec["blocks"] if b["axis"] == "latency_slo")
    assert slo["infeasibility_boundary"] is not None
    assert rec["points"]
