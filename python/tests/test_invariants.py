from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st

from micp_eval.invariants import (
    InvariantFailure,
    assert_invariants,
    check_capacity_throughput_monotone,
    check_cost_ceiling_consistency,
    check_deterministic_recommendation,
    check_feasible_set_nested,
    check_pareto_undominated,
    check_rate_utilization_monotone,
    check_sim_reproducible,
)

from fakes import FakeEngine, make_plan, route_item, toy_scenario


def test_pareto_rejects_dominated_member() -> None:
    good = route_item(key="a:k0:none:c0:none", p99=50, quality=0.9, cost=0.01, util=0.2, failure=0.01, feasible=True)
    bad = route_item(key="b:k0:none:c0:none", p99=80, quality=0.8, cost=0.02, util=0.3, failure=0.02, feasible=True)
    plan = make_plan([good, bad])
    plan["pareto_keys"] = ["a:k0:none:c0:none", "b:k0:none:c0:none"]
    failures = check_pareto_undominated(plan)
    assert failures


def test_pareto_accepts_incomparable() -> None:
    fast = route_item(key="fast:k0:none:c0:none", p99=50, quality=0.7, cost=0.01, util=0.2, failure=0.01, feasible=True)
    qual = route_item(
        key="qual:k0:none:c0:none", p99=200, quality=0.95, cost=0.04, util=0.3, failure=0.01, feasible=True
    )
    plan = make_plan([fast, qual])
    assert check_pareto_undominated(plan) == []


def test_feasible_set_must_nest_when_tightening() -> None:
    a = route_item(key="a:k0:none:c0:none", p99=80, quality=0.8, cost=0.01, util=0.2, failure=0.01, feasible=True)
    b = route_item(key="b:k0:none:c0:none", p99=90, quality=0.8, cost=0.01, util=0.2, failure=0.01, feasible=True)
    loose = make_plan([a, b])
    tight = make_plan([a])
    assert check_feasible_set_nested(loose, tight) == []
    assert check_feasible_set_nested(tight, loose)


def test_cost_violation_must_exceed_ceiling() -> None:
    item = route_item(
        key="a:k0:none:c0:none", p99=50, quality=0.8, cost=0.01, util=0.1, failure=0.01, feasible=False, slo=10
    )
    # force a cost violation with observed < limit
    item["violations"] = [{"kind": "Cost", "message": "x", "observed": 0.001, "limit": 0.01}]
    plan = make_plan([item])
    assert check_cost_ceiling_consistency(plan)


def test_rate_utilization_monotone_via_fake() -> None:
    eng = FakeEngine()
    s = toy_scenario()
    series = []
    for rps in (5.0, 15.0, 40.0, 80.0):
        s["workload"]["traffic"]["mean_rps"] = rps
        series.append((rps, eng.evaluate(s).plan))
    failures = check_rate_utilization_monotone(series, "fast-8b:k0:none:c0:none")
    assert failures == []


def test_capacity_throughput_monotone_via_fake() -> None:
    eng = FakeEngine()
    s = toy_scenario()
    series = []
    for f in (1.0, 0.5, 0.25, 0.0):
        s["fleet"][0]["capacity"]["degraded_factor"] = f
        series.append((f, eng.evaluate(s).plan))
    failures = check_capacity_throughput_monotone(series, "fast-8b:k0:none:c0:none")
    assert failures == []


def test_deterministic_and_sim_repro() -> None:
    eng = FakeEngine()
    s = toy_scenario()
    a = eng.evaluate(s).plan
    b = eng.evaluate(s).plan
    assert check_deterministic_recommendation(a, b) == []
    sa = eng.simulate(s, seed=3)
    sb = eng.simulate(s, seed=3)
    assert check_sim_reproducible(sa, sb) == []
    sc = eng.simulate(s, seed=4)
    assert check_sim_reproducible(sa, sc)


def test_assert_invariants_raises() -> None:
    with pytest.raises(InvariantFailure):
        assert_invariants(["boom"], context="x")


@given(st.lists(st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False), min_size=2, max_size=6))
def test_undominated_equal_points_all_survive(qs: list[float]) -> None:
    items = [
        route_item(
            key=f"k{i}:k0:none:c0:none",
            p99=100.0,
            quality=0.8,
            cost=0.01,
            util=0.2,
            failure=0.01,
            feasible=True,
        )
        for i, _ in enumerate(qs)
    ]
    plan = make_plan(items)
    assert check_pareto_undominated(plan) == []
