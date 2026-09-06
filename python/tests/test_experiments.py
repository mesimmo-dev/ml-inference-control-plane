from __future__ import annotations

from pathlib import Path

from micp_eval.artifacts import write_experiment
from micp_eval.experiments import run_all_scenarios, run_scenario, run_simulations, run_sweep
from micp_eval.sweeps import SweepAxis, SweepSpec

from fakes import FakeEngine


def test_evaluate_toy_scenario() -> None:
    rec = run_scenario(FakeEngine(), "interactive_assistant")
    assert rec["kind"] == "evaluate"
    assert rec["result"]["origin"] == "modeled"
    assert rec["result"]["feasible"] is True
    assert rec["experiment_id"] == run_scenario(FakeEngine(), "interactive_assistant")["experiment_id"]


def test_run_all() -> None:
    recs = run_all_scenarios(FakeEngine())
    assert len(recs) == 1
    assert recs[0]["scenario_id"] == "interactive_assistant"


def test_sweep_is_reproducible() -> None:
    spec = SweepSpec(
        name="rps",
        scenario_id="interactive_assistant",
        axes=(SweepAxis("workload.traffic.mean_rps", (5.0, 15.0, 40.0, 80.0)),),
        max_points=8,
    )
    a = run_sweep(FakeEngine(), spec)
    b = run_sweep(FakeEngine(), spec)
    assert a["experiment_id"] == b["experiment_id"]
    assert [r["recommended_key"] for r in a["points"]] == [r["recommended_key"] for r in b["points"]]
    assert a["points"][0]["p_workload_traffic_mean_rps"] == 5.0


def test_sweep_hits_infeasible_boundary() -> None:
    spec = SweepSpec(
        name="slo",
        scenario_id="interactive_assistant",
        axes=(SweepAxis("workload.constraints.latency_slo", (10.0, 50.0, 200.0)),),
        max_points=8,
    )
    rec = run_sweep(FakeEngine(), spec)
    statuses = [r["status"] for r in rec["points"]]
    assert "no_feasible" in statuses
    assert "ok" in statuses


def test_simulations_summarize(tmp_path: Path) -> None:
    rec = run_simulations(FakeEngine(), "interactive_assistant", seeds=[1, 2, 3])
    assert rec["origin"] == "simulated"
    assert rec["summary"]["p99_ms"]["n"] == 3
    assert "simulation-derived" in rec["summary"]["label"]
    dest = write_experiment(tmp_path / rec["experiment_id"], rec)
    assert (dest / "results.json").is_file()
