# Architecture

The control plane sits **in front of** model-serving runtimes. It does
not train models and it does not own GPU scheduling. It decides *which*
candidate in a known fleet should handle a request (or what share of
traffic each candidate should receive) given live constraints.

```
                     ┌─────────────────────────────────────┐
                     │           Workbench (TS/React)      │
                     │     inspect decisions / SLOs / WASM │
                     └──────────────────┬──────────────────┘
                                        │ HTTP
                     ┌──────────────────▼──────────────────┐
                     │          micp-api  (Axum)           │
                     │  /health /v1/route /v1/allocate     │
                     │  /v1/scenarios /v1/evaluate         │
                     │  /v1/recommend /v1/simulate         │
                     └──────────────────┬──────────────────┘
                                        │
          ┌──────────────┬──────────────┼──────────────┬──────────────┐
          ▼              ▼              ▼              ▼              ▼
     micp-policy    micp-slo      micp-router   micp-optimizer   micp-sim
     (monotonic     (error        (plan +       (Pareto +        (seeded
      tighten +      budget)       inventory     water-fill)      G/G/n)
      degrade)                     route)
          │              │              │              │              │
          └──────────────┴──────────────┴──────────────┴──────────────┘
                                        │
                                        ▼
                                   micp-core
                    units, inventory, domain, estimate, scenarios
```

Python (`micp_eval`) lives **off** this path. It drives `micp-api` over
HTTP: scenario evaluation, bounded parameter sweeps, sensitivity,
invariant checks, and artifact export. A small reference module
re-implements documented cost/quality/occupancy formulas for
cross-checks and is labeled as such. Python never sits on the request
path. See [docs/evaluation.md](docs/evaluation.md).

## Two type layers

1. **Inventory** (`ModelProfile`) — compact config/API snapshot
   (p99, quality, unit cost, capacity, error rate). Kept so the
   existing TOML config, `/v1/route`, `/v1/allocate`, WASM score ABI,
   and TypeScript scaffolding stay compatible.
2. **Engine** (`InferenceCandidate` + `WorkloadProfile`) — typed
   serving model with units, retrieval, batching, token
   distributions, SLO, exhaustion policy. Used by `/v1/evaluate`,
   `/v1/recommend`, `/v1/simulate`, and the scenario presets.

`InferenceCandidate::from_inventory` is a documented adapter, not a
fit to production telemetry.

## Decision pipeline (engine)

A plan is a pure function of (fleet, workload, weights):

1. **Expand** (`micp-policy`) — each candidate plus a small
   degradation ladder: disable rerank, halve `top_k`, halve context,
   alter batching, switch to `fallback_to`. Constraints are never
   relaxed; extra routes are added.
2. **Estimate** (`micp-core`) — closed-form p50/p95/p99, utilization,
   quality proxy, USD/request, SLO-miss and failure probabilities.
   Origin = `modeled`.
3. **Feasibility** — hard filters (latency SLO, quality floor, cost
   ceiling, reliability target, min capacity, reject-on-saturation).
   Violations are structured (`ConstraintKind` + observed/limit).
4. **Pareto** (`micp-optimizer`) — non-dominated set on latency,
   quality, cost, reliability.
5. **Recommend** — highest five-weight score on the front; tie-break
   is the lexicographically smaller route key.

Inventory `/v1/route` still uses the original score-and-pick path
for the compact snapshot types.

If the feasible set is empty the API returns `409` /
`NoFeasibleModel`.

## Built-in scenarios

`micp-core::scenarios` ships five inspectable presets (assumptions
included in the struct, served at `/v1/scenarios`):

| id | Intent |
| --- | --- |
| `interactive_assistant` | Closed-book chat, p99 ≤ 200ms |
| `cost_constrained_volume` | 80 rps, tight USD/req |
| `quality_rag` | Hybrid retrieval + rerank, quality floor 0.85 |
| `bursty_enterprise` | 8× peaks; estimates size to peak |
| `degraded_failover` | 70b down, mixtral at 40% replicas |

## Observability

`micp-api` initializes a `tracing` subscriber. The `telemetry` module
is the future OpenTelemetry attach point. Exporters are not included
in this revision.

## WASM boundary

The `micp-wasm` crate keeps a C ABI (`extern "C"`, `f64` in/out):

- `micp_score_candidate` — inventory score (unchanged)
- `micp_slo_burn_rate` — empirical-window burn (unchanged)
- `micp_modeled_p99_ms` — closed-form p99
- `micp_slo_violation_prob` — lognormal SLO miss

`wasm32-unknown-unknown` + `rustc` is sufficient; no bindgen.

## Models, limitations

See [docs/models.md](docs/models.md). Headline limits:

- Queueing is M/M/n, not a GPU runtime
- Quality is a proxy, not a judge
- Cost is list-price-like, not a bill
- Burst estimates size to peak
- Simulation is seeded G/G/n, tagged `simulated`

## What this repository is not

- A model server, tokenizer, or GPU scheduler
- A service mesh or generic API gateway
- A training / experiment-tracking platform
- A Kubernetes operator
