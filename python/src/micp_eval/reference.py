"""Independent Python reference of the *documented* closed-form formulas.

This module exists so evaluation can cross-check cost, quality, mean
service time, and occupancy against `docs/models.md` without treating
Python as the engine.

It is **not** a substitute for `micp-api`. Engine numbers always come
from Rust. A mismatch means the docs or the engine drifted.
"""

from __future__ import annotations

import math
from typing import Any

MAX_TOP_K = 1_000


def _f(value: Any, default: float = 0.0) -> float:
    try:
        x = float(value)
    except (TypeError, ValueError):
        return default
    return x if math.isfinite(x) else default


def _retrieval_none(retrieval: dict[str, Any]) -> bool:
    return str(retrieval.get("strategy", "none")).lower() == "none"


def _rerank_none(retrieval: dict[str, Any]) -> bool:
    return str(retrieval.get("rerank", "none")).lower() == "none"


def _effective_k(retrieval: dict[str, Any]) -> int:
    if _retrieval_none(retrieval):
        return 0
    return min(int(retrieval.get("top_k") or 0), MAX_TOP_K)


def mean_service_ms(route: dict[str, Any], workload: dict[str, Any]) -> float:
    cand = route["candidate"]
    lat = cand["latency"]
    retrieval = route.get("retrieval") or {}
    prompt = max(_f(workload["prompt_tokens"]["mean"]), 0.0)
    budget = max(_f(route.get("context_budget")), 0.0)
    ctx = max(min(_f(workload["context_tokens"]["mean"]), budget), 0.0)
    out = max(_f(workload["expected_output_tokens"]["mean"]), 0.0)
    s = (
        _f(lat["intercept_ms"])
        + max(_f(lat["ms_per_input_token"]), 0.0) * (prompt + ctx)
        + max(_f(lat["ms_per_output_token"]), 0.0) * out
    )
    k = float(_effective_k(retrieval))
    if not _retrieval_none(retrieval):
        s += max(_f(lat.get("retrieval_ms_per_k")), 0.0) * k
    if not _rerank_none(retrieval):
        s += max(_f(lat.get("rerank_ms")), 0.0)
    batching = route.get("batching") or {}
    if str(batching.get("type", "none")).lower() == "window":
        s += max(_f(batching.get("max_wait_ms")), 0.0) * 0.5
    return max(s, 0.1)


def quality_proxy(route: dict[str, Any], workload: dict[str, Any]) -> float:
    q = route["candidate"]["quality"]
    v = _f(q["base"])
    retrieval = route.get("retrieval") or {}
    ctx = max(min(_f(workload["context_tokens"]["mean"]), _f(route.get("context_budget"))), 0.0)
    sat = max(_f(q.get("context_saturation"), 1.0), 1.0)
    fill = 1.0 - math.exp(-ctx / sat)
    if not _retrieval_none(retrieval):
        k = float(_effective_k(retrieval))
        v += max(_f(q.get("retrieval_gain_per_k")), 0.0) * math.log(1.0 + k) * fill
    if not _rerank_none(retrieval):
        v += max(_f(q.get("rerank_gain")), 0.0) * fill
    return min(max(v, 0.0), 1.0)


def cost_per_request(route: dict[str, Any], workload: dict[str, Any]) -> float:
    c = route["candidate"]["cost"]
    retrieval = route.get("retrieval") or {}
    prompt = max(_f(workload["prompt_tokens"]["mean"]), 0.0)
    budget = max(_f(route.get("context_budget")), 0.0)
    ctx = max(min(_f(workload["context_tokens"]["mean"]), budget), 0.0)
    out = max(_f(workload["expected_output_tokens"]["mean"]), 0.0)
    usd = (prompt + ctx) / 1000.0 * max(_f(c["usd_per_1k_input"]), 0.0) + out / 1000.0 * max(
        _f(c["usd_per_1k_output"]), 0.0
    )
    if not _retrieval_none(retrieval):
        usd += max(_f(c.get("usd_per_retrieval")), 0.0)
    if not _rerank_none(retrieval):
        usd += max(_f(c.get("usd_per_rerank")), 0.0)
    return max(usd, 0.0)


def occupancy(route: dict[str, Any], workload: dict[str, Any]) -> tuple[float, bool, float]:
    """Return (rho, saturated, n_servers) using the documented M/M/n sizing."""
    cand = route["candidate"]
    cap = cand["capacity"]
    factor = _f(cap.get("degraded_factor"), 1.0)
    usable = 0 if (not math.isfinite(factor) or factor <= 0.0) else math.floor(_f(cap["max_concurrency"]) * factor)
    offered = max(int(workload["traffic"].get("concurrency") or 1), 1)
    n = float(min(int(usable), offered))
    service = mean_service_ms(route, workload)
    s_sec = service / 1000.0
    traffic = workload["traffic"]
    lam = max(_f(traffic["mean_rps"]), 0.0)
    burst = traffic.get("burst")
    if isinstance(burst, dict):
        peak = _f(burst.get("peak_multiplier"), 1.0)
        if math.isfinite(peak) and peak > 1.0:
            lam *= peak
    if n <= 0.0 or not math.isfinite(s_sec) or s_sec <= 0.0:
        return math.inf, True, n
    rho = lam * s_sec / n
    saturated = (not math.isfinite(rho)) or rho >= 1.0
    return (1.0 if saturated else max(rho, 0.0)), saturated, n


def compare_estimate(
    route: dict[str, Any],
    workload: dict[str, Any],
    estimate: dict[str, Any],
    *,
    atol: float = 1e-6,
    rtol: float = 1e-6,
) -> list[str]:
    """Compare Rust estimate fields that have a documented closed form.

    Latency percentiles are *not* compared: they include the queueing
    lognormal transform, which this validator does not re-host.
    """
    failures: list[str] = []
    ref_cost = cost_per_request(route, workload)
    eng_cost = _f(estimate.get("cost_per_request"))
    if not math.isclose(ref_cost, eng_cost, rel_tol=rtol, abs_tol=atol):
        failures.append(f"cost_per_request reference={ref_cost} engine={eng_cost}")
    ref_q = quality_proxy(route, workload)
    eng_q = _f(estimate.get("quality"))
    if not math.isclose(ref_q, eng_q, rel_tol=rtol, abs_tol=1e-5):
        failures.append(f"quality reference={ref_q} engine={eng_q}")
    rho, saturated, _n = occupancy(route, workload)
    eng_rho = _f(estimate.get("utilization"))
    if math.isfinite(rho) and not math.isclose(min(rho, 1.0), eng_rho, rel_tol=1e-4, abs_tol=1e-4):
        failures.append(f"utilization reference={rho} engine={eng_rho}")
    eng_sat = bool(estimate.get("saturated"))
    if saturated != eng_sat:
        failures.append(f"saturated reference={saturated} engine={eng_sat}")
    return failures
