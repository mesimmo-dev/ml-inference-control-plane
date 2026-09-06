from __future__ import annotations

from pathlib import Path

from micp_eval.artifacts import (
    experiment_id,
    flatten_plan,
    read_json,
    write_experiment,
    write_json,
)

from fakes import make_plan, route_item


def test_experiment_id_is_stable() -> None:
    payload = {"scenario_id": "interactive_assistant", "axes": [1, 2, 3]}
    assert experiment_id("sweep", payload) == experiment_id("sweep", payload)
    assert experiment_id("sweep", payload) != experiment_id("evaluate", payload)


def test_write_roundtrip(tmp_path: Path) -> None:
    plan = make_plan(
        [
            route_item(
                key="a:k0:none:c0:none",
                p99=80.0,
                quality=0.7,
                cost=0.001,
                util=0.2,
                failure=0.01,
                feasible=True,
            )
        ]
    )
    rec = {
        "experiment_id": "abc",
        "kind": "evaluate",
        "scenario_id": "toy",
        "timestamp": "2026-01-01T00:00:00+00:00",
        "result": flatten_plan(plan),
        "points": [{"i": 0, **flatten_plan(plan)}],
        "environment": {"git_sha": None},
    }
    dest = write_experiment(tmp_path / "abc", rec)
    loaded = read_json(dest / "results.json")
    assert loaded["experiment_id"] == "abc"
    assert (dest / "results.csv").is_file()
    assert (dest / "manifest.json").is_file()


def test_json_roundtrip(tmp_path: Path) -> None:
    path = write_json(tmp_path / "x.json", {"a": 1})
    assert read_json(path) == {"a": 1}


def test_flatten_empty() -> None:
    row = flatten_plan(None)
    assert row["feasible"] is False
    assert row["n_evaluated"] == 0
