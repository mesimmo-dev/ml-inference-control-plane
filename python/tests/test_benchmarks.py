from __future__ import annotations

import pytest

from micp_eval.benchmarks import _python_pareto_cost, generate_fleet, local_bench

from fakes import FakeEngine


def test_generate_fleet_rejects_non_positive() -> None:
    with pytest.raises(ValueError):
        generate_fleet(0)


def test_python_pareto_grows_with_n() -> None:
    assert _python_pareto_cost(4, seed=0) >= 1
    assert _python_pareto_cost(16, seed=0) >= 1


def test_local_bench_against_fake() -> None:
    rec = local_bench(FakeEngine(), rounds=2)
    assert rec["label"].startswith("local synthetic")
    assert "evaluate:interactive_assistant" in rec["timings"]
    assert rec["timings"]["python_pareto_growth"][0]["n"] == 4
