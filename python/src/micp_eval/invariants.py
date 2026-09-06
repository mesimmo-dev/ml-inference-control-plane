"""Analytical invariants. Failures are test failures, not warnings."""

from __future__ import annotations

from typing import Any

from .reference import compare_estimate

EPS = 1e-9


class InvariantFailure(AssertionError):
    pass


def _est(plan: dict[str, Any], route_key: str) -> dict[str, Any] | None:
    for item in plan.get("evaluated") or []:
        estimate = item.get("estimate") or {}
        if estimate.get("route_key") == route_key:
            return estimate
    return None


def _evaluated(plan: dict[str, Any]) -> list[dict[str, Any]]:
    return list(plan.get("evaluated") or [])


def check_pareto_undominated(plan: dict[str, Any]) -> list[str]:
    """No Pareto member is dominated by another retained feasible candidate."""
    keys = list(plan.get("pareto_keys") or [])
    feasible = {e["estimate"]["route_key"]: e["estimate"] for e in _evaluated(plan) if not e.get("violations")}
    failures: list[str] = []
    for k in keys:
        a = feasible.get(k)
        if a is None:
            failures.append(f"pareto key {k} is not in the feasible set")
            continue
        for other_key, b in feasible.items():
            if other_key == k:
                continue
            if _dominates(b, a):
                failures.append(f"{other_key} dominates pareto member {k}")
    return failures


def _dominates(b: dict[str, Any], a: dict[str, Any]) -> bool:
    lat_b, lat_a = float(b["p99_ms"]), float(a["p99_ms"])
    cost_b, cost_a = float(b["cost_per_request"]), float(a["cost_per_request"])
    q_b, q_a = float(b["quality"]), float(a["quality"])
    r_b = 1.0 - float(b["failure_prob"])
    r_a = 1.0 - float(a["failure_prob"])
    le = lat_b <= lat_a + EPS and cost_b <= cost_a + EPS and q_b + EPS >= q_a and r_b + EPS >= r_a
    lt = lat_b < lat_a - EPS or cost_b < cost_a - EPS or q_b > q_a + EPS or r_b > r_a + EPS
    return bool(le and lt)


def check_stricter_slo_cannot_heal(plan: dict[str, Any]) -> list[str]:
    """A route whose p99 already exceeds the SLO must carry a latency violation.

    Tightening the SLO cannot, by itself, make that route feasible.
    """
    failures: list[str] = []
    for item in _evaluated(plan):
        est = item.get("estimate") or {}
        p99 = float(est.get("p99_ms") or 0.0)
        violations = item.get("violations") or []
        lat_v = next((v for v in violations if v.get("kind") == "Latency"), None)
        if lat_v is not None:
            limit = float(lat_v.get("limit") or 0.0)
            if p99 + EPS < limit:
                failures.append(f"{est.get('route_key')}: latency violation but p99 {p99} < SLO {limit}")
    return failures


def check_feasible_set_nested(loose: dict[str, Any], tight: dict[str, Any]) -> list[str]:
    """Tightening a hard constraint must not admit previously infeasible keys."""
    before = set(loose.get("feasible_keys") or [])
    after = set(tight.get("feasible_keys") or [])
    extra = after - before
    if extra:
        return [f"tighter constraint admitted new feasible keys {sorted(extra)}"]
    return []


def check_cost_ceiling_consistency(plan: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    for item in _evaluated(plan):
        est = item.get("estimate") or {}
        violations = item.get("violations") or []
        cost_v = next((v for v in violations if v.get("kind") == "Cost"), None)
        if cost_v is None:
            continue
        observed = float(cost_v.get("observed") or est.get("cost_per_request") or 0.0)
        limit = float(cost_v.get("limit") or 0.0)
        if observed + EPS < limit:
            failures.append(f"{est.get('route_key')}: cost violation but {observed} < ceiling {limit}")
    return failures


def check_rate_utilization_monotone(series: list[tuple[float, dict[str, Any]]], route_key: str) -> list[str]:
    """Increasing request rate must not *decrease* modeled utilization of a fixed route."""
    failures: list[str] = []
    prev_rate: float | None = None
    prev_util: float | None = None
    for rate, plan in series:
        est = _est(plan, route_key)
        if est is None:
            continue
        util = float(est["utilization"])
        if prev_rate is not None and prev_util is not None and rate > prev_rate + EPS:
            if util + 1e-6 < prev_util:
                failures.append(
                    f"utilization dropped from {prev_util} at rps={prev_rate} to {util} at rps={rate} for {route_key}"
                )
        prev_rate, prev_util = rate, util
    return failures


def check_capacity_throughput_monotone(series: list[tuple[float, dict[str, Any]]], route_key: str) -> list[str]:
    """Reducing degraded_factor must not increase sustainable throughput of a fixed route."""
    ordered = sorted(series, key=lambda kv: kv[0], reverse=True)
    failures: list[str] = []
    prev_f: float | None = None
    prev_tp: float | None = None
    for factor, plan in ordered:
        est = _est(plan, route_key)
        if est is None:
            continue
        tp = float(est["throughput_rps"])
        if prev_f is not None and prev_tp is not None and factor < prev_f - EPS:
            if tp > prev_tp + 1e-6:
                failures.append(
                    f"throughput rose from {prev_tp} at factor={prev_f} to {tp} at factor={factor} for {route_key}"
                )
        prev_f, prev_tp = factor, tp
    return failures


def check_deterministic_recommendation(a: dict[str, Any], b: dict[str, Any]) -> list[str]:
    ka = ((a.get("recommended") or {}).get("estimate") or {}).get("route_key")
    kb = ((b.get("recommended") or {}).get("estimate") or {}).get("route_key")
    if ka != kb:
        return [f"recommendations diverged: {ka} vs {kb}"]
    if a.get("pareto_keys") != b.get("pareto_keys"):
        return ["pareto_keys diverged under identical inputs"]
    return []


def check_sim_reproducible(a: dict[str, Any], b: dict[str, Any]) -> list[str]:
    keys = ("origin", "n_arrivals", "n_served", "n_rejected", "p50_ms", "p95_ms", "p99_ms")
    failures = []
    for k in keys:
        if a.get(k) != b.get(k):
            failures.append(f"sim field {k} diverged: {a.get(k)} vs {b.get(k)}")
    return failures


def check_reference_formulas(plan: dict[str, Any], workload: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    for item in _evaluated(plan):
        route = item.get("route") or {}
        est = item.get("estimate") or {}
        failures.extend(compare_estimate(route, workload, est))
    return failures


def check_plan(plan: dict[str, Any], workload: dict[str, Any] | None = None) -> list[str]:
    failures = []
    failures.extend(check_pareto_undominated(plan))
    failures.extend(check_stricter_slo_cannot_heal(plan))
    failures.extend(check_cost_ceiling_consistency(plan))
    if workload is not None:
        failures.extend(check_reference_formulas(plan, workload))
    return failures


def assert_invariants(failures: list[str], *, context: str = "") -> None:
    if failures:
        prefix = f"{context}: " if context else ""
        raise InvariantFailure(prefix + "; ".join(failures))
