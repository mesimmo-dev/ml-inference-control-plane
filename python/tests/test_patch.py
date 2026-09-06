from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st

from micp_eval.patch import apply_patches, get_path, set_path, split_path

from fakes import toy_scenario


def test_split_and_set_roundtrip() -> None:
    s = toy_scenario()
    out = set_path(s, "workload.traffic.mean_rps", 42.0)
    assert get_path(out, "workload.traffic.mean_rps") == 42.0
    assert get_path(s, "workload.traffic.mean_rps") == 15.0


def test_id_selector_and_wildcard() -> None:
    s = toy_scenario()
    out = set_path(s, "fleet[id=fast-8b].capacity.degraded_factor", 0.4)
    assert get_path(out, "fleet[id=fast-8b].capacity.degraded_factor") == 0.4
    out2 = set_path(s, "fleet.*.capacity.degraded_factor", 0.0)
    assert out2["fleet"][0]["capacity"]["degraded_factor"] == 0.0


def test_apply_patches_order() -> None:
    s = toy_scenario()
    out = apply_patches(
        s,
        {
            "workload.constraints.latency_slo": 100.0,
            "workload.traffic.mean_rps": 30.0,
        },
    )
    assert get_path(out, "workload.constraints.latency_slo") == 100.0
    assert get_path(out, "workload.traffic.mean_rps") == 30.0


def test_invalid_path() -> None:
    with pytest.raises(ValueError):
        split_path("")
    with pytest.raises(KeyError):
        get_path(toy_scenario(), "nope.here")


@given(st.floats(min_value=0.1, max_value=1e3, allow_nan=False, allow_infinity=False))
def test_set_numeric_is_stable(v: float) -> None:
    s = toy_scenario()
    out = set_path(s, "workload.traffic.mean_rps", v)
    assert get_path(out, "workload.traffic.mean_rps") == v
