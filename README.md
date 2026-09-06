# ML Inference Control Plane

SLO-aware control plane for adaptive model routing and inference
optimization under competing **latency**, **quality**, **cost**,
**throughput**, **traffic**, and **reliability** constraints.

This repository is a polyglot systems foundation: a Rust workspace for
the latency-sensitive control-plane engine, a Python package for
evaluation sweeps, and a TypeScript/React workbench shell. The Rust
engine is implemented; the workbench is still a compile/run scaffold.

## Why the languages split this way

| Layer | Language | Role |
| --- | --- | --- |
| Routing, policy, optimizer, SLO, simulation | Rust | Deterministic, allocation-aware core on the request path |
| Control-plane HTTP API | Rust (Tokio + Axum) | Async service boundary around that core |
| Evaluation, workloads, sweeps, stats | Python | Experimentation and statistical validation off the request path |
| Systems workbench | TypeScript + React | Interactive inspection of decisions and constraints |
| Browser-side scoring | Rust → WASM | Same scoring/SLO math in the workbench, no TS reimplementation |

OpenTelemetry is wired as a tracing/init hook in the API crate. Exporters
and auto-instrumentation land later, once the request path is stable.

## Repository layout

```
.
├── crates/                 Rust workspace
│   ├── micp-core           Shared domain types, constraints, scoring
│   ├── micp-policy         Ordered policy evaluation
│   ├── micp-slo            SLO windows, error budget, burn rate
│   ├── micp-router         Feasible-set routing
│   ├── micp-optimizer      Traffic-share allocation under capacity
│   ├── micp-sim            Deterministic workload generation
│   ├── micp-api            Axum control-plane service
│   └── micp-wasm           C-ABI WASM exports of core scoring
├── python/                 `micp_eval` evaluation package
├── web/                    React workbench
├── configs/                Default fleet, weights, policies, SLOs
├── docker/                 API image
└── docs/                   Development and reproducibility notes
```

## Prerequisites

- Rust stable (`rust-toolchain.toml`), with `rustfmt` and `clippy`
- Python 3.10+
- Node.js 20+
- Optional: `wasm32-unknown-unknown` target for the WASM crate
- Optional: Docker, to build the API image

## Build and test

From the repository root:

```bash
# Rust workspace (API binary + all crates)
cargo test --workspace
cargo build -p micp-api --release

# Python evaluation package
python3 -m venv python/.venv
python/.venv/bin/pip install -e "python[dev]"
python/.venv/bin/pytest python/tests -q

# Workbench
cd web && npm install && npm test && npm run build
```

Run the control-plane API (listens on `0.0.0.0:8080` by default):

```bash
cargo run -p micp-api
# GET  /health
# GET  /v1/scenarios
# POST /v1/evaluate    {"scenario_id":"interactive_assistant"}
# POST /v1/recommend   {"scenario_id":"quality_rag"}
# POST /v1/simulate    {"scenario_id":"interactive_assistant","model_id":"fast-8b"}
# POST /v1/route
# POST /v1/allocate
```

Workbench (development):

```bash
cd web && npm run dev
```

See [docs/development.md](docs/development.md) for toolchain pins,
WASM, Docker, and CI.

## WebAssembly

The `micp-wasm` crate is a `cdylib` with a stable C ABI (`extern "C"`).
It compiles with the `wasm32-unknown-unknown` target and **does not**
require `wasm-bindgen` or `wasm-pack`. That is intentional: the ABI is
small (scalar scoring/SLO helpers) and the toolchain is just `rustc`.

```bash
rustup target add wasm32-unknown-unknown
cargo build -p micp-wasm --target wasm32-unknown-unknown --release
```

Exports: `micp_score_candidate`, `micp_slo_burn_rate`,
`micp_modeled_p99_ms`, `micp_slo_violation_prob` (all `f64` C ABI).
The workbench will load this module in a later iteration. Until then
the crate still builds and its native tests run as part of `cargo test`.

## Status

Rust systems core (Phase 2): typed domain, closed-form route
estimates, SLO feasibility, degradation ladder, Pareto
recommendation, seeded G/G/n simulation, five scenario presets,
Axum endpoints, WASM C ABI. The TypeScript workbench is still a
shell; Python evaluation is still the Phase 1 scaffold.

See [ARCHITECTURE.md](ARCHITECTURE.md) and [docs/models.md](docs/models.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
