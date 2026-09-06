"""Test doubles. Not the Rust engine."""

from __future__ import annotations

from copy import deepcopy
from typing import Any

from micp_eval.engine import PlanOutcome
from micp_eval.patch import get_path


def route_item(
    *,
    key: str,
    p99: float,
    quality: float,
    cost: float,
    util: float,
    failure: float,
    feasible: bool,
    saturated: bool = False,
    throughput: float = 10.0,
    slo: float = 200.0,
) -> dict[str, Any]:
    model = key.split(":")[0]
    est = {
        "origin": "modeled",
        "route_key": key,
        "model_id": model,
        "p50_ms": p99 * 0.6,
        "p95_ms": p99 * 0.9,
        "p99_ms": p99,
        "throughput_rps": throughput,
        "utilization": util,
        "saturated": saturated,
        "quality": quality,
        "cost_per_request": cost,
        "cost_per_1k": cost * 1000.0,
        "slo_violation_prob": 0.01,
        "failure_prob": failure,
        "fallback_activation_prob": 0.0,
    }
    violations = []
    if not feasible:
        kind = "Latency" if p99 > slo else "Cost"
        limit = slo if kind == "Latency" else cost * 0.5
        violations.append(
            {"kind": kind, "message": "synthetic", "observed": p99 if kind == "Latency" else cost, "limit": limit}
        )
    return {
        "route": {
            "candidate": {
                "id": model,
                "latency": {
                    "intercept_ms": 20.0,
                    "ms_per_input_token": 0.01,
                    "ms_per_output_token": 0.05,
                    "retrieval_ms_per_k": 1.0,
                    "rerank_ms": 10.0,
                    "sigma_ms": 5.0,
                },
                "quality": {
                    "base": quality,
                    "retrieval_gain_per_k": 0.0,
                    "rerank_gain": 0.0,
                    "context_saturation": 2048.0,
                },
                "cost": {
                    "usd_per_1k_input": 0.004,
                    "usd_per_1k_output": 0.008,
                    "usd_per_retrieval": 0.0,
                    "usd_per_rerank": 0.0,
                },
                "reliability": {
                    "base_error_rate": failure,
                    "timeout_as_failure": 0.5,
                    "saturation_error_slope": 0.2,
                },
                "capacity": {"max_concurrency": 16, "max_tokens_per_sec": 1e4, "degraded_factor": 1.0},
                "retrieval_capable": True,
                "fallback_to": None,
            },
            "retrieval": {"strategy": "none", "top_k": 0, "rerank": "none", "context_budget": 0.0},
            "batching": {"type": "none"},
            "context_budget": 0.0,
        },
        "estimate": est,
        "violations": violations,
        "degradations": [],
    }


def make_plan(items: list[dict[str, Any]]) -> dict[str, Any]:
    feasible = [i for i in items if not i["violations"]]
    keys = [i["estimate"]["route_key"] for i in feasible]
    rec = feasible[0] if feasible else None
    return {
        "evaluated": items,
        "feasible_keys": keys,
        "pareto_keys": keys[:],
        "recommended": rec,
    }


def toy_scenario(scenario_id: str = "interactive_assistant") -> dict[str, Any]:
    return {
        "id": scenario_id,
        "name": scenario_id,
        "summary": "toy",
        "assumptions": ["synthetic test double; not the Rust engine"],
        "workload": {
            "id": scenario_id,
            "traffic": {"class": "interactive", "mean_rps": 15.0, "concurrency": 8, "burst": None},
            "prompt_tokens": {"mean": 256.0, "p95": 256.0, "p99": 256.0},
            "context_tokens": {"mean": 0.0, "p95": 0.0, "p99": 0.0},
            "expected_output_tokens": {"mean": 128.0, "p95": 128.0, "p99": 128.0},
            "retrieval": {"strategy": "none", "top_k": 0, "rerank": "none", "context_budget": 0.0},
            "batching": {"type": "none"},
            "constraints": {
                "latency_slo": 200.0,
                "quality_floor": 0.65,
                "cost_ceiling_per_request": 0.01,
                "reliability_target": 0.99,
                "min_capacity": 10.0,
            },
            "exhaustion": {"type": "reject"},
        },
        "fleet": [
            {
                "id": "fast-8b",
                "capacity": {"max_concurrency": 16, "max_tokens_per_sec": 1e4, "degraded_factor": 1.0},
            }
        ],
        "weights": {
            "latency": 0.45,
            "quality": 0.2,
            "cost": 0.15,
            "throughput": 0.05,
            "reliability": 0.15,
        },
    }


class FakeEngine:
    """Deterministic stand-in used when tests must not spawn micp-api."""

    def __init__(self, scenario: dict[str, Any] | None = None) -> None:
        self.scenario = scenario or toy_scenario()
        self.sim_calls = 0

    def list_scenarios(self) -> list[dict[str, Any]]:
        s = self.scenario
        return [{"id": s["id"], "name": s["name"], "summary": s["summary"], "assumptions": s["assumptions"]}]

    def get_scenario(self, scenario_id: str) -> dict[str, Any]:
        if scenario_id != self.scenario["id"]:
            from micp_eval.client import EngineError

            raise EngineError(f"unknown scenario {scenario_id}", status=404)
        return deepcopy(self.scenario)

    def evaluate(self, scenario: dict[str, Any], *, allow_infeasible: bool = True) -> PlanOutcome:
        rps = float(get_path(scenario, "workload.traffic.mean_rps"))
        slo = float(get_path(scenario, "workload.constraints.latency_slo"))
        ceiling = float(get_path(scenario, "workload.constraints.cost_ceiling_per_request"))
        factor = float((scenario["fleet"][0].get("capacity") or {}).get("degraded_factor", 1.0))
        p99 = 40.0 + rps * 0.5
        util = min(1.0, 0.02 * rps / max(factor, 1e-6))
        cost = 0.002
        throughput = 50.0 * max(factor, 0.0)
        feasible = p99 <= slo and cost <= ceiling and factor > 0
        item = route_item(
            key="fast-8b:k0:none:c0:none",
            p99=p99,
            quality=0.7,
            cost=cost,
            util=util,
            failure=0.004,
            feasible=feasible,
            saturated=util >= 1.0,
            throughput=throughput,
            slo=slo,
        )
        plan = make_plan([item])
        if not feasible and not allow_infeasible:
            return PlanOutcome(status="no_feasible", plan=None, error="no feasible model", http_status=409)
        return PlanOutcome(
            status="ok" if feasible else "no_feasible",
            plan=plan,
            error=None if feasible else "no feasible model",
            http_status=200,
        )

    def simulate(
        self,
        scenario: dict[str, Any],
        *,
        model_id: str | None = None,
        seed: int = 1,
        duration_s: float = 2.0,
        long_tail: bool = False,
    ) -> dict[str, Any]:
        self.sim_calls += 1
        return {
            "origin": "simulated",
            "n_arrivals": 20 + seed,
            "n_served": 18 + seed,
            "n_rejected": 0,
            "p50_ms": 30.0 + seed,
            "p95_ms": 40.0 + seed,
            "p99_ms": 50.0 + seed,
            "mean_sojourn_ms": 32.0,
            "mean_wait_ms": 1.0,
            "utilization": 0.2,
            "notes": [f"seed={seed}", "synthetic test double"],
        }
