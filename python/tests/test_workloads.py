from __future__ import annotations

import pytest

from micp_eval.workloads import ArrivalSpec, generate_arrivals


def test_arrivals_are_deterministic() -> None:
    spec = ArrivalSpec(arrival_rate_rps=40.0, duration_s=2.0, seed=11)
    a = generate_arrivals(spec)
    b = generate_arrivals(spec)
    assert [(x.t_s, x.traffic_class) for x in a] == [(x.t_s, x.traffic_class) for x in b]
    assert a
    assert a[0].t_s < a[-1].t_s < spec.duration_s


def test_class_mix_roughly_honored() -> None:
    spec = ArrivalSpec(
        arrival_rate_rps=200.0,
        duration_s=3.0,
        seed=3,
        class_mix={"interactive": 0.9, "batch": 0.1},
    )
    arr = generate_arrivals(spec)
    frac = sum(1 for a in arr if a.traffic_class == "interactive") / len(arr)
    assert 0.8 < frac < 0.98


def test_invalid_spec() -> None:
    with pytest.raises(ValueError):
        generate_arrivals(ArrivalSpec(arrival_rate_rps=0.0, duration_s=1.0, seed=1))
    with pytest.raises(ValueError):
        generate_arrivals(ArrivalSpec(arrival_rate_rps=1.0, duration_s=1.0, seed=1, class_mix={"interactive": 0.0}))
