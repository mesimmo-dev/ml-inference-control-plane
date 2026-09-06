# ML Inference Control Plane

SLO-aware control plane for adaptive model routing and inference
optimization under competing **latency**, **quality**, **cost**,
**throughput**, **traffic**, and **reliability** constraints.

This repository is the systems foundation: a Rust workspace for the
latency-sensitive core, a Python package for evaluation and experiment
sweeps, and a TypeScript/React workbench. It is not yet a complete
product — the crates compile, the baseline algorithms are unit-tested,
and the service/API scaffolding boots.

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
# GET /health
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

The workbench will load this module in a later iteration. Until then the
crate still builds and its native tests run as part of `cargo test`.

## Status

Architecture and compile/run scaffolding only. Routing, policy, SLO,
optimizer, and simulation implement baseline algorithms with unit tests;
they are not production-hardened, and the workbench is a shell.

## License

Apache-2.0. See [LICENSE](LICENSE).
