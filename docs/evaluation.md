# Evaluation architecture

`micp_eval` is an **off-path** Python layer. It orchestrates experiments,
sweeps, sensitivity, invariant checks, and artifact export. It does not
sit on the request path and it does not replace the Rust engine.

```
micp-eval CLI / pytest
        │  HTTP JSON
        ▼
    micp-api  (Axum)     ← source of modeled / simulated numbers
        │
        ▼
    micp-core / router / sim
```

Python talks to the engine through the existing HTTP boundary
(`/v1/scenarios`, `/v1/evaluate`, `/v1/simulate`). Phase 3 adds two
additive request fields:

- `scenario`: a full scenario body (the same JSON as `GET /v1/scenarios/{id}`)
- `allow_infeasible`: when true, an empty feasible set is `200` with
  `recommended: null` plus the evaluated/violation payload (needed for
  sweeps that must observe infeasibility boundaries)

Default behaviour of `/v1/evaluate` is unchanged (`409` when nothing is feasible).

## Why pandas, not Polars

Experiment tables are small (tens to a few thousand rows). pandas 2.x
gives CSV round-trip, `pandas-stubs` for mypy, and a well-known
dataframe API for a later workbench. Polars would add a second frame
library without a query-engine workload to justify it.

SciPy is **not** a dependency. Percentiles and percentile-bootstrap CIs
are NumPy. Hypothesis is a *dev* dependency, used for grid and path
round-trip properties.

## Experiment lifecycle

1. Load a built-in scenario from the engine (`GET /v1/scenarios/{id}`).
2. Optionally patch nested fields (`workload.traffic.mean_rps`,
   `fleet[id=fast-8b].capacity.degraded_factor`, `fleet.*…`).
3. `POST /v1/evaluate` with `allow_infeasible=true`.
4. Record the plan (evaluated routes, feasible set, Pareto keys,
   recommendation) plus environment metadata.
5. Run invariants. Failures are test failures.
6. Write `manifest.json` + `results.json` + `results.csv`.

Every experiment has a 16-hex `experiment_id` = SHA-256 of a canonical
JSON payload (kind, scenario, axes, budget, seed). The same spec on the
same code produces the same id.

## Sweep methodology

Axes are explicit lists, never an unbounded search. The cartesian
product is **reduced** until it fits `max_points`:

- downsample the longest axis, keeping endpoints
- if every axis is already length 2, take a deterministic prefix that
  still includes the first and last corners

There is no Latin-hypercube sampler and no adaptive design. That is
intentional: the grid is inspectable.

## Sensitivity methodology

One-at-a-time. Named axes (`latency_slo`, `mean_rps`, `cost_ceiling`,
`quality_floor`, `context_budget`, `top_k`, `degraded_factor`) each get
a small bounded recipe around the scenario baseline. The report records:

- recommendation transitions (route key or feasibility changed)
- first infeasibility boundary along the axis
- first saturation threshold

These are descriptive, not causal claims about a production fleet.

## Statistical assumptions

- Closed-form `/v1/evaluate` numbers are **modeled** (`origin=modeled`).
- `/v1/simulate` numbers are **simulated** (`origin=simulated`).
- `empirical` is unused.
- Across-seed summaries of p99 / n_served use a percentile bootstrap CI
  on the mean. The CI is labeled *simulation-derived uncertainty*. It is
  not an empirical production interval and it is not a p-value.

Python `micp_eval.workloads.generate_arrivals` is a NumPy helper for
experiment scripts. It is **not** bit-identical with `micp-sim`.

## Reference validator

`micp_eval.reference` re-implements the documented cost, quality, mean
service time, and occupancy formulas from `docs/models.md`. It exists
to catch docs/engine drift. It does **not** re-host p99 lognormal
percentiles, Pareto selection, or the degradation ladder. Engine
numbers always win; a mismatch is a bug to inspect.

## Benchmark methodology

`micp-eval bench` times local HTTP round-trips (evaluate, a tiny sweep,
simulate) and a Python-only O(n²) Pareto growth curve. These are
**local synthetic/engineering timings**. They include Python / pandas /
platform / git SHA so a later reader can see they are not a capacity
plan. Criterion numbers from `cargo bench -p micp-core --bench engine`
remain the Rust estimator microbenchmark.

## Artifact schema

```
python/artifacts/<experiment_id>/
  manifest.json     # id, kind, scenario, timestamp, git SHA, row count
  results.json      # full envelope (schema = micp-eval.v1)
  results.csv       # tabular points when present
```

`results.json` fields: `experiment_id`, `kind`, `timestamp`,
`scenario_id`, `seed`, `input`, `inventory` (evaluate), `result` or
`points` or `reports`, `validation`, `environment`, `origin`.

Do not commit generated `python/artifacts/` trees. A tiny curated
example lives in `python/examples/artifacts/`.

## Example commands

Start the engine, then:

```bash
cargo build -p micp-api --release
./target/release/micp-api   # 0.0.0.0:8080

python/.venv/bin/micp-eval scenarios
python/.venv/bin/micp-eval evaluate --scenario interactive_assistant --out python/artifacts
python/.venv/bin/micp-eval evaluate --all --out python/artifacts
python/.venv/bin/micp-eval sweep --scenario interactive_assistant \
    --axis workload.traffic.mean_rps=5,15,30,60 \
    --axis workload.constraints.latency_slo=80,200,400 \
    --budget 16 --out python/artifacts
python/.venv/bin/micp-eval sensitivity --scenario quality_rag --out python/artifacts
python/.venv/bin/micp-eval validate --all
python/.venv/bin/micp-eval simulate --scenario interactive_assistant --model fast-8b
python/.venv/bin/micp-eval bench --out python/artifacts
python/.venv/bin/micp-eval summarize --path python/artifacts/<id>/results.json
```

Representative evaluate output (shape, not a measured SLO):

```json
{
  "kind": "evaluate",
  "scenario_id": "interactive_assistant",
  "result": {
    "status": "ok",
    "origin": "modeled",
    "recommended_key": "fast-8b:k0:none:c0:none",
    "p99_ms": 57.7,
    "quality": 0.70,
    "cost_per_request": 0.002,
    "n_pareto": 1
  },
  "validation": { "passed": true, "failures": [] }
}
```

The numeric fields above are illustrative. Read a live `results.json`
for the engine's current modeled values.

## Invariants

Checked in pytest (and `micp-eval validate`):

- Pareto members are not dominated by another feasible route
- A latency/cost violation's observed value is not below its limit
- Tightening a hard constraint cannot admit previously infeasible keys
- Raising request rate does not decrease modeled utilization of a fixed route
- Lowering `degraded_factor` does not increase that route's throughput
- Identical inputs produce identical recommendations
- Identical `(scenario, seed)` simulations are bit-equal on reported fields
- Documented cost / quality / occupancy match the engine within tolerance
