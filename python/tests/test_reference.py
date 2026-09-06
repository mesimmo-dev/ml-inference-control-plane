from __future__ import annotations

from micp_eval.reference import (
    compare_estimate,
    cost_per_request,
    mean_service_ms,
    occupancy,
    quality_proxy,
)

from fakes import route_item, toy_scenario


def _route_workload() -> tuple[dict, dict]:
    item = route_item(
        key="fast-8b:k0:none:c0:none",
        p99=80,
        quality=0.7,
        cost=0.001,
        util=0.2,
        failure=0.003,
        feasible=True,
    )
    return item["route"], toy_scenario()["workload"]


def test_closed_form_cost_and_quality() -> None:
    route, workload = _route_workload()
    # 256 in * 0.004/1k + 128 out * 0.008/1k = 0.001024 + 0.001024
    assert abs(cost_per_request(route, workload) - 0.002048) < 1e-9
    assert abs(quality_proxy(route, workload) - 0.7) < 1e-12


def test_mean_service_and_occupancy() -> None:
    route, workload = _route_workload()
    s = mean_service_ms(route, workload)
    assert s > 20.0
    rho, sat, n = occupancy(route, workload)
    assert n == 8.0  # min(16, concurrency 8)
    assert not sat
    assert 0.0 < rho < 1.0


def test_compare_estimate_flags_mismatch() -> None:
    route, workload = _route_workload()
    est = {
        "cost_per_request": 9.9,
        "quality": 0.7,
        "utilization": 0.01,
        "saturated": False,
    }
    failures = compare_estimate(route, workload, est)
    assert any("cost_per_request" in f for f in failures)


def test_retrieval_terms_increase_cost_and_service() -> None:
    route, workload = _route_workload()
    closed = mean_service_ms(route, workload)
    route["retrieval"] = {"strategy": "dense", "top_k": 8, "rerank": "cross_encoder", "context_budget": 1024.0}
    route["context_budget"] = 1024.0
    workload["context_tokens"] = {"mean": 400.0, "p95": 400.0, "p99": 400.0}
    opened = mean_service_ms(route, workload)
    assert opened > closed
    assert cost_per_request(route, workload) > 0.002
    assert quality_proxy(route, workload) >= 0.7
