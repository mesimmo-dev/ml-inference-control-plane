# ML Inference Control Plane

SLO-aware control plane for adaptive model routing and inference
optimization under competing **latency**, **quality**, **cost**,
**throughput**, **traffic**, and **reliability** constraints.

An executable ML systems environment: a Rust engine on the request path,
a Python evaluation layer off that path, and a TypeScript/React workbench
that talks to the live engine. It does **not** train models, own GPUs, or
report empirical production telemetry.

## Live Workbench

**Interactive deployment:** https://swift-civic-plum-silver.grok.me

**Core stack:** Rust · Tokio/Axum · Python · TypeScript/React · WebAssembly

> Engineering/research prototype. Reported system quantities are modeled, simulated, or local synthetic benchmarks unless explicitly identified as empirical.

## Why this problem

A serving fleet is a constrained multi-objective system. Tightening p99
usually taxes quality or cost; raising retrieval depth taxes latency;
degraded replicas tax capacity. Operators need a control plane that:

1. expands a small, inspectable set of routes (including degradations)
2. estimates each route under an explicit model
3. filters on hard SLOs without silently relaxing them
4. returns the Pareto set and a deterministic recommendation

Guessing a model in an application router hides those trade-offs.
This repository makes them executable and labeled.

## Language responsibilities

| Layer | Language | Role |
| --- | --- | --- |
| Routing, policy, optimizer, SLO, simulation | Rust | Deterministic core on the request path |
| Control-plane HTTP API | Rust (Tokio + Axum) | Async service boundary; optionally serves the workbench |
| Evaluation, workloads, sweeps, stats | Python | Off-path experiments and artifact export |
| Systems workbench | TypeScript + React | Interactive inspection of live plans |
| Browser-side inventory helpers | Rust → WASM C ABI | `ModelProfile::score`, modeled p99, SLO-miss helpers |

The workbench **does not** reimplement routing or Pareto selection in
JavaScript. `POST /v1/evaluate` and `POST /v1/simulate` are the source
of modeled and simulated numbers. WASM covers only the compact inventory
score and closed-form helpers; if the module fails to load, a labeled
TypeScript port of `ModelProfile::score` is used as a **local synthetic
benchmark**, never as a route plan.

## Architecture

```
Workbench (TS/React)          micp_eval (Python)
        │  HTTP                        │  HTTP
        └──────────────┬───────────────┘
                       ▼
                 micp-api (Axum)
         /health /v1/scenarios /v1/evaluate
         /v1/recommend /v1/simulate /v1/route
                       │
     policy · slo · router · optimizer · sim
                       │
                   micp-core
```

See [ARCHITECTURE.md](ARCHITECTURE.md), [docs/models.md](docs/models.md),
[docs/evaluation.md](docs/evaluation.md), and
[docs/workbench.md](docs/workbench.md).

## Estimate origins

| Tag | Meaning |
| --- | --- |
| **MODELED** | Closed-form M/M/n + lognormal sojourn. Not a measurement. |
| **SIMULATED** | Seeded G/G/n discrete-event run. Not a production trace. |
| **LOCAL SYNTHETIC BENCHMARK** | Inventory score helper (WASM or TS port). Not a route plan. |
| **EMPIRICAL** | Reserved. Unused in this repository. |

## Repository layout

```
.
├── crates/                 Rust workspace
│   ├── micp-core           Units, domain, inventory, estimate, scenarios
│   ├── micp-policy         Ordered policy + degradation expansion
│   ├── micp-slo            SLO windows, error budget, burn rate
│   ├── micp-router         Feasible-set routing / plan
│   ├── micp-optimizer      Pareto + recommendation
│   ├── micp-sim            Seeded workload generation + G/G/n
│   ├── micp-api            Axum control-plane service
│   └── micp-wasm           C-ABI WASM exports
├── python/                 `micp_eval` evaluation package
├── web/                    React workbench
├── configs/                Default fleet, weights, policies, SLOs
├── docker/                 API image
└── docs/
```

## Prerequisites

- Rust stable (`rust-toolchain.toml`), with `rustfmt` and `clippy`
- Python 3.10+
- Node.js 20+
- Optional: `wasm32-unknown-unknown` target for the WASM crate
- Optional: Docker, to build the API image

## Build, test, run

From the repository root:

```bash
# Rust workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p micp-api --release

# Python evaluation package
python3 -m venv python/.venv
python/.venv/bin/pip install -e "python[dev]"
python/.venv/bin/pytest python/tests -q

# Workbench
cd web && npm ci && npm test && npm run typecheck && npm run build
```

Control-plane API (default bind `0.0.0.0:8080`):

```bash
cargo run -p micp-api --release
# GET  /health
# GET  /v1/scenarios
# POST /v1/evaluate    {"scenario_id":"interactive_assistant"}
# POST /v1/evaluate    {"scenario": {...}, "allow_infeasible": true}
# POST /v1/simulate    {"scenario_id":"interactive_assistant","model_id":"fast-8b"}
```

Combined API + workbench (after `cd web && npm run build`):

```bash
MICP_WEB_DIST=web/dist cargo run -p micp-api --release
# If web/dist exists, micp-api serves it automatically.
```

Workbench in development (Vite proxies `/v1` and `/health` to the API on
`127.0.0.1:8081`):

```bash
MICP_BIND=127.0.0.1:8081 cargo run -p micp-api --release
cd web && npm run dev
```

## WebAssembly

```bash
rustup target add wasm32-unknown-unknown
cargo build -p micp-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/micp_wasm.wasm web/public/micp_wasm.wasm
```

Exports: `micp_score_candidate`, `micp_slo_burn_rate`,
`micp_modeled_p99_ms`, `micp_slo_violation_prob` (all `f64` C ABI).
No `wasm-bindgen`.

## Reproducibility

- Rust: committed `Cargo.lock`, `cargo test --workspace`
- Python: `micp-eval` experiment ids are SHA-256 of a canonical spec;
  curated fixtures live in `python/examples/artifacts/`
- Workbench: committed `package-lock.json`; live numbers come from the
  engine, not from checked-in charts
- Simulation: seeded; same spec → same report

## Modeling limitations (conservative)

- Queueing is M/M/n or seeded G/G/n, not a GPU runtime
- Quality is a proxy, not a judge
- Cost is list-price-like, not a bill
- Burst estimates size to peak
- Degraded capacity scales concurrency, not token speed
- Empirical origin is unused

## License

Apache-2.0. See [LICENSE](LICENSE).
