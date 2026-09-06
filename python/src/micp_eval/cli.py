"""Command-line interface: `micp-eval <command>`."""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Any

from .artifacts import write_experiment
from .benchmarks import local_bench
from .engine import HttpEngine
from .experiments import run_all_scenarios, run_scenario, run_simulations, run_sweep
from .invariants import InvariantFailure, assert_invariants
from .sensitivity import run_sensitivity
from .sweeps import SweepSpec, parse_axis


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="micp-eval",
        description="Reproducible evaluation layer around the Rust ML inference control plane.",
    )
    p.add_argument("--url", default="http://127.0.0.1:8080", help="micp-api base URL")
    p.add_argument("--out", type=Path, default=Path("python/artifacts"), help="artifact directory")
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("scenarios", help="list built-in scenarios")

    ev = sub.add_parser("evaluate", help="evaluate one or all scenarios via the Rust API")
    ev.add_argument("--scenario", default=None, help="scenario id; omit with --all")
    ev.add_argument("--all", action="store_true")
    ev.add_argument("--require-invariants", action="store_true")

    sw = sub.add_parser("sweep", help="bounded parameter sweep")
    sw.add_argument("--scenario", required=True)
    sw.add_argument("--axis", action="append", required=True, help="path=v1,v2,... (repeatable)")
    sw.add_argument("--budget", type=int, default=64)
    sw.add_argument("--name", default="sweep")

    se = sub.add_parser("sensitivity", help="one-at-a-time constraint sensitivity")
    se.add_argument("--scenario", required=True)
    se.add_argument("--axis", action="append", default=None, help="named axis (repeatable)")

    va = sub.add_parser("validate", help="evaluate and fail if invariants break")
    va.add_argument("--scenario", default=None)
    va.add_argument("--all", action="store_true")

    sub.add_parser("bench", help="local engineering timings against a live API")

    sim = sub.add_parser("simulate", help="seeded G/G/n summaries for one scenario")
    sim.add_argument("--scenario", required=True)
    sim.add_argument("--model", default=None)
    sim.add_argument("--seeds", default="1,2,3,5,8")
    sim.add_argument("--duration", type=float, default=2.0)

    su = sub.add_parser("summarize", help="print a stored results.json")
    su.add_argument("--path", type=Path, required=True)
    return p


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.cmd == "summarize":
        data = json.loads(Path(args.path).read_text(encoding="utf-8"))
        _print_summary(data)
        return 0

    engine = HttpEngine(args.url)
    out: Path = args.out

    if args.cmd == "scenarios":
        for card in engine.list_scenarios():
            print(f"{card['id']:28} {card.get('name', '')}")
        return 0

    if args.cmd == "evaluate":
        records = _eval_records(engine, args.scenario, args.all, args.require_invariants)
        for rec in records:
            dest = out / rec["experiment_id"]
            write_experiment(dest, rec)
            print(f"{rec['scenario_id']:28} {rec['result']['status']:12} {rec['result'].get('recommended_key')}")
        return 0 if all(r["validation"]["passed"] for r in records) else 1

    if args.cmd == "validate":
        records = _eval_records(engine, args.scenario, args.all, True)
        failures = [f for r in records for f in r["validation"]["failures"]]
        try:
            assert_invariants(failures, context="validate")
        except InvariantFailure as exc:
            print(str(exc), file=sys.stderr)
            return 1
        print("invariants passed")
        return 0

    if args.cmd == "sweep":
        axes = tuple(parse_axis(token) for token in args.axis)
        spec = SweepSpec(name=args.name, scenario_id=args.scenario, axes=axes, max_points=args.budget)
        rec = run_sweep(engine, spec)
        dest = out / rec["experiment_id"]
        write_experiment(dest, rec, rec["points"])
        print(f"wrote {dest} ({len(rec['points'])} points)")
        return 0

    if args.cmd == "sensitivity":
        rec = run_sensitivity(engine, args.scenario, axes=args.axis)
        dest = out / rec["experiment_id"]
        write_experiment(dest, rec, rec["points"])
        for block in rec["blocks"]:
            print(
                f"{block['axis']:18} transitions={len(block['transitions'])} boundary={block['infeasibility_boundary']}"
            )
        return 0

    if args.cmd == "simulate":
        seeds = [int(s) for s in args.seeds.split(",") if s.strip()]
        rec = run_simulations(engine, args.scenario, model_id=args.model, seeds=seeds, duration_s=args.duration)
        dest = out / rec["experiment_id"]
        write_experiment(dest, rec)
        print(json.dumps(rec["summary"]["p99_ms"], indent=2))
        return 0

    if args.cmd == "bench":
        rec = local_bench(engine)
        dest = out / "bench"
        write_experiment(dest, rec)
        print(json.dumps(rec["timings"], indent=2, default=str))
        return 0

    return 2


def _eval_records(engine: HttpEngine, scenario: str | None, all_flag: bool, require: bool) -> list[dict[str, Any]]:
    if all_flag or scenario is None:
        return run_all_scenarios(engine, require_invariants=require)
    return [run_scenario(engine, scenario, require_invariants=require)]


def _print_summary(data: dict[str, Any]) -> None:
    print(f"id={data.get('experiment_id')} kind={data.get('kind')} scenario={data.get('scenario_id')}")
    if "result" in data:
        r = data["result"]
        print(
            f"  status={r.get('status')} key={r.get('recommended_key')} "
            f"p99={r.get('p99_ms')} cost={r.get('cost_per_request')}"
        )
    if "points" in data:
        print(f"  points={len(data['points'])}")
    val = data.get("validation") or {}
    print(f"  validation_passed={val.get('passed')}")


if __name__ == "__main__":
    raise SystemExit(main())
