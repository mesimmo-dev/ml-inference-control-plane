from __future__ import annotations

import pytest

from micp_eval.cli import _print_summary, build_parser
from micp_eval.sweeps import parse_axis


def test_parser_subcommands() -> None:
    p = build_parser()
    args = p.parse_args(["evaluate", "--all"])
    assert args.cmd == "evaluate"
    assert args.all
    args = p.parse_args(["sweep", "--scenario", "interactive_assistant", "--axis", "workload.traffic.mean_rps=1,2"])
    assert args.cmd == "sweep"
    assert parse_axis(args.axis[0]).values == (1, 2)


def test_summarize_prints(capsys: pytest.CaptureFixture[str]) -> None:
    data = {
        "experiment_id": "x",
        "kind": "evaluate",
        "scenario_id": "interactive_assistant",
        "result": {
            "status": "ok",
            "recommended_key": "fast-8b:k0:none:c0:none",
            "p99_ms": 80.0,
            "cost_per_request": 0.002,
        },
        "validation": {"passed": True},
    }
    _print_summary(data)
    out = capsys.readouterr().out
    assert "interactive_assistant" in out
    assert "fast-8b" in out
