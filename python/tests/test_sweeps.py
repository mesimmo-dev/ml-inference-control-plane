from __future__ import annotations

import numpy as np
from hypothesis import given
from hypothesis import strategies as st

from micp_eval.benchmarks import generate_fleet
from micp_eval.sweeps import (
    SweepAxis,
    SweepSpec,
    WeightGrid,
    bound_axes,
    iter_sweep_points,
    iter_weight_grid,
    parse_axis,
    weight_matrix,
)


def test_weight_grid_sums_to_one() -> None:
    rows = list(iter_weight_grid(WeightGrid(step=0.5)))
    assert rows
    for w in rows:
        assert abs(sum(w.values()) - 1.0) < 1e-9
        assert set(w) == {"latency", "quality", "cost", "throughput", "reliability"}


def test_weight_matrix_shape() -> None:
    m = weight_matrix(WeightGrid(step=0.5))
    assert m.ndim == 2
    assert m.shape[1] == 5
    assert np.allclose(m.sum(axis=1), 1.0)


def test_fleet_size_and_ids() -> None:
    fleet = generate_fleet(4, seed=0)
    assert len(fleet) == 4
    assert [c.model_id for c in fleet] == ["m00", "m01", "m02", "m03"]
    assert all(0.0 < c.quality <= 1.0 for c in fleet)


def test_parse_axis() -> None:
    axis = parse_axis("workload.traffic.mean_rps=5,15,30")
    assert axis.path == "workload.traffic.mean_rps"
    assert axis.values == (5, 15, 30)


def test_budget_caps_cartesian() -> None:
    spec = SweepSpec(
        name="grid",
        scenario_id="interactive_assistant",
        axes=(
            SweepAxis("a", tuple(range(10))),
            SweepAxis("b", tuple(range(10))),
        ),
        max_points=12,
    )
    points = iter_sweep_points(spec)
    assert len(points) <= 12
    assert points[0]["a"] == 0
    bounded = bound_axes(spec.axes, 12)
    assert all(len(a.values) >= 2 for a in bounded)


def test_one_dimensional_preserves_values() -> None:
    spec = SweepSpec(
        name="1d",
        scenario_id="x",
        axes=(SweepAxis("workload.traffic.mean_rps", (5.0, 10.0, 20.0)),),
        max_points=64,
    )
    pts = iter_sweep_points(spec)
    assert [p["workload.traffic.mean_rps"] for p in pts] == [5.0, 10.0, 20.0]


@given(st.integers(min_value=1, max_value=20), st.integers(min_value=2, max_value=8))
def test_budget_never_exceeded(budget: int, nvals: int) -> None:
    spec = SweepSpec(
        name="p",
        scenario_id="x",
        axes=(SweepAxis("a", tuple(range(nvals))), SweepAxis("b", tuple(range(nvals)))),
        max_points=budget,
    )
    pts = iter_sweep_points(spec)
    assert 1 <= len(pts) <= budget
