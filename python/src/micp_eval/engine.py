"""Evaluation interface to the Rust control-plane engine.

`HttpEngine` is the source of modeled/simulated numbers. `FixtureEngine`
replays recorded API responses for tests that must not spawn a process.
Neither path reimplements the router in Python.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from .client import EngineError, MicpClient

SCENARIO_IDS: tuple[str, ...] = (
    "interactive_assistant",
    "cost_constrained_volume",
    "quality_rag",
    "bursty_enterprise",
    "degraded_failover",
)


@dataclass(frozen=True)
class PlanOutcome:
    """Result of one engine evaluation.

    `status`:
      - `ok` — recommendation present, origin is modeled
      - `no_feasible` — engine found no feasible route
      - `error` — invalid config or transport failure
    """

    status: str
    plan: dict[str, Any] | None
    error: str | None = None
    http_status: int | None = None

    @property
    def feasible(self) -> bool:
        return self.status == "ok" and self.plan is not None and self.plan.get("recommended") is not None


class Engine(Protocol):
    def list_scenarios(self) -> list[dict[str, Any]]: ...

    def get_scenario(self, scenario_id: str) -> dict[str, Any]: ...

    def evaluate(self, scenario: dict[str, Any], *, allow_infeasible: bool = True) -> PlanOutcome: ...

    def simulate(
        self,
        scenario: dict[str, Any],
        *,
        model_id: str | None = None,
        seed: int = 1,
        duration_s: float = 2.0,
        long_tail: bool = False,
    ) -> dict[str, Any]: ...


class HttpEngine:
    """Talks to a running `micp-api` process."""

    def __init__(self, base_url: str, timeout_s: float = 15.0) -> None:
        self.client = MicpClient(base_url, timeout_s=timeout_s)
        self.base_url = base_url.rstrip("/")

    def list_scenarios(self) -> list[dict[str, Any]]:
        status, body = self.client.get("/v1/scenarios")
        if status != 200 or not isinstance(body, list):
            raise EngineError(f"list_scenarios failed: HTTP {status}", status=status, body=body)
        return body

    def get_scenario(self, scenario_id: str) -> dict[str, Any]:
        status, body = self.client.get(f"/v1/scenarios/{scenario_id}")
        if status != 200 or not isinstance(body, dict):
            raise EngineError(
                f"unknown scenario {scenario_id}" if status == 404 else f"get_scenario failed: HTTP {status}",
                status=status,
                body=body,
            )
        return body

    def evaluate(self, scenario: dict[str, Any], *, allow_infeasible: bool = True) -> PlanOutcome:
        status, body = self.client.post(
            "/v1/evaluate",
            {"scenario": scenario, "allow_infeasible": allow_infeasible},
        )
        if status == 409:
            return PlanOutcome(status="no_feasible", plan=None, error=_error_message(body), http_status=409)
        if status == 400:
            return PlanOutcome(status="error", plan=None, error=_error_message(body), http_status=400)
        if status != 200 or not isinstance(body, dict):
            raise EngineError(f"evaluate failed: HTTP {status}", status=status, body=body)
        rec = body.get("recommended")
        return PlanOutcome(
            status="ok" if rec is not None else "no_feasible",
            plan=body,
            error=None if rec is not None else "no feasible model",
            http_status=status,
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
        payload: dict[str, Any] = {
            "scenario": scenario,
            "seed": seed,
            "duration_s": duration_s,
            "long_tail": long_tail,
        }
        if model_id is not None:
            payload["model_id"] = model_id
        status, body = self.client.post("/v1/simulate", payload)
        if status != 200 or not isinstance(body, dict):
            raise EngineError(f"simulate failed: HTTP {status}", status=status, body=body)
        return body


class FixtureEngine:
    """Replay recorded `/v1/scenarios` and `/v1/evaluate` JSON.

    Patched scenarios are rejected — this is a replay tape, not a model.
    """

    def __init__(self, root: Path) -> None:
        self.root = Path(root)

    def list_scenarios(self) -> list[dict[str, Any]]:
        cards = []
        for sid in SCENARIO_IDS:
            path = self.root / "scenarios" / f"{sid}.json"
            if not path.is_file():
                continue
            raw = json.loads(path.read_text(encoding="utf-8"))
            cards.append(
                {
                    "id": raw["id"],
                    "name": raw.get("name", raw["id"]),
                    "summary": raw.get("summary", ""),
                    "assumptions": raw.get("assumptions", []),
                }
            )
        return cards

    def get_scenario(self, scenario_id: str) -> dict[str, Any]:
        path = self.root / "scenarios" / f"{scenario_id}.json"
        if not path.is_file():
            raise EngineError(f"unknown scenario {scenario_id}", status=404)
        return json.loads(path.read_text(encoding="utf-8"))

    def evaluate(self, scenario: dict[str, Any], *, allow_infeasible: bool = True) -> PlanOutcome:
        sid = str(scenario.get("id", ""))
        recorded = self.get_scenario(sid)
        if scenario != recorded:
            raise EngineError("FixtureEngine cannot evaluate a patched scenario; use HttpEngine")
        plan_path = self.root / "plans" / f"{sid}.json"
        if not plan_path.is_file():
            raise EngineError(f"no recorded plan for {sid}")
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        rec = plan.get("recommended")
        if rec is None and not allow_infeasible:
            return PlanOutcome(status="no_feasible", plan=None, error="no feasible model", http_status=409)
        return PlanOutcome(
            status="ok" if rec is not None else "no_feasible",
            plan=plan,
            error=None if rec is not None else "no feasible model",
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
        sid = str(scenario.get("id", ""))
        mid = model_id or "default"
        path = self.root / "simulate" / f"{sid}_{mid}_s{seed}.json"
        if not path.is_file():
            raise EngineError(f"no recorded simulation for {sid} {mid} seed={seed}")
        return json.loads(path.read_text(encoding="utf-8"))


def _error_message(body: Any) -> str:
    if isinstance(body, dict) and "error" in body:
        return str(body["error"])
    return str(body)
