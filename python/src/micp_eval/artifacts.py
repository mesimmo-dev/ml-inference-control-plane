"""Experiment artifact I/O: JSON plus tabular CSV. No Parquet dependency."""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import pandas as pd

SCHEMA_VERSION = "micp-eval.v1"


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def canonical_dumps(obj: Any) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), default=_default)


def experiment_id(kind: str, payload: dict[str, Any]) -> str:
    """Stable 16-hex identifier of kind + canonical payload."""
    blob = canonical_dumps({"kind": kind, **payload})
    return hashlib.sha256(blob.encode("utf-8")).hexdigest()[:16]


def _default(obj: Any) -> Any:
    if hasattr(obj, "isoformat"):
        return obj.isoformat()
    raise TypeError(f"not JSON serializable: {type(obj)!r}")


def write_json(path: Path, obj: Any) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=2, default=_default) + "\n", encoding="utf-8")
    return path


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_csv(path: Path, rows: list[dict[str, Any]]) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    pd.DataFrame(rows).to_csv(path, index=False)
    return path


def read_csv(path: Path) -> pd.DataFrame:
    return pd.read_csv(path)


def write_experiment(directory: Path, record: dict[str, Any], rows: list[dict[str, Any]] | None = None) -> Path:
    """Write `manifest.json`, `results.json`, and optional `results.csv`."""
    directory.mkdir(parents=True, exist_ok=True)
    envelope = {
        "schema": SCHEMA_VERSION,
        **record,
    }
    write_json(directory / "results.json", envelope)
    write_json(
        directory / "manifest.json",
        {
            "schema": SCHEMA_VERSION,
            "experiment_id": record.get("experiment_id"),
            "kind": record.get("kind"),
            "scenario_id": record.get("scenario_id"),
            "timestamp": record.get("timestamp"),
            "git_sha": (record.get("environment") or {}).get("git_sha"),
            "n_rows": len(rows or record.get("points") or []),
        },
    )
    table = rows if rows is not None else record.get("points")
    if isinstance(table, list) and table and isinstance(table[0], dict):
        write_csv(directory / "results.csv", table)
    return directory


def flatten_plan(plan: dict[str, Any] | None) -> dict[str, Any]:
    """Tabular view of a RoutePlan for CSV export / later visualization."""
    if not plan:
        return {
            "feasible": False,
            "recommended_key": None,
            "model_id": None,
            "origin": None,
            "p50_ms": None,
            "p95_ms": None,
            "p99_ms": None,
            "throughput_rps": None,
            "utilization": None,
            "saturated": None,
            "quality": None,
            "cost_per_request": None,
            "cost_per_1k": None,
            "slo_violation_prob": None,
            "failure_prob": None,
            "fallback_activation_prob": None,
            "n_evaluated": 0,
            "n_feasible": 0,
            "n_pareto": 0,
        }
    rec = plan.get("recommended") or {}
    est = rec.get("estimate") or {}
    route = rec.get("route") or {}
    model_id = est.get("model_id")
    if model_id is None:
        model_id = (route.get("candidate") or {}).get("id")
    return {
        "feasible": rec != {},
        "recommended_key": est.get("route_key"),
        "model_id": model_id,
        "origin": est.get("origin"),
        "p50_ms": est.get("p50_ms"),
        "p95_ms": est.get("p95_ms"),
        "p99_ms": est.get("p99_ms"),
        "throughput_rps": est.get("throughput_rps"),
        "utilization": est.get("utilization"),
        "saturated": est.get("saturated"),
        "quality": est.get("quality"),
        "cost_per_request": est.get("cost_per_request"),
        "cost_per_1k": est.get("cost_per_1k"),
        "slo_violation_prob": est.get("slo_violation_prob"),
        "failure_prob": est.get("failure_prob"),
        "fallback_activation_prob": est.get("fallback_activation_prob"),
        "n_evaluated": len(plan.get("evaluated") or []),
        "n_feasible": len(plan.get("feasible_keys") or []),
        "n_pareto": len(plan.get("pareto_keys") or []),
        "pareto_keys": list(plan.get("pareto_keys") or []),
        "feasible_keys": list(plan.get("feasible_keys") or []),
        "degradations": list(rec.get("degradations") or []),
    }
