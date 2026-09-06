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
                     │   /health  /v1/route  /v1/allocate  │
                     │   tracing init (OTel exporters TBD) │
                     └──────────────────┬──────────────────┘
                                        │
          ┌──────────────┬──────────────┼──────────────┬──────────────┐
          ▼              ▼              ▼              ▼              ▼
     micp-policy    micp-slo      micp-router   micp-optimizer   micp-sim
          │              │              │              │              │
          └──────────────┴──────────────┴──────────────┴──────────────┘
                                        │
                                        ▼
                                   micp-core
                          (types, constraints, scoring)
```

Python (`micp_eval`) lives **off** this path. It generates workloads,
sweeps weight/constraint grids, and validates outcome distributions. It
never sits on the request path.

## Decision pipeline

A routing decision is a pure function of (fleet, request, policies,
weights, demand):

1. **Policy evaluation** (`micp-policy`) — matching policies tighten
   the request's latency/quality/cost/reliability bounds and may
   allow/deny specific model IDs. Policies are ordered by priority;
   later policies can only *narrow* the feasible set.
2. **Feasibility filter** (`micp-core`) — drop candidates that violate
   the effective constraints.
3. **Admission** (`micp-slo`) — if the active SLO's remaining error
   budget is below a configured floor, only candidates that improve
   (or do not worsen) the budget are admitted.
4. **Score** (`micp-core`) — remaining candidates are scored with
   normalized objective weights:
   - latency: `1 / (1 + p99_ms / 100)`
   - quality: reported quality in `[0, 1]`
   - cost: `1 / (1 + cost_per_1k)`
   - throughput: `capacity / (capacity + demand)`
   - reliability: `1 - error_rate`
5. **Select or allocate**
   - `micp-router` picks the single highest-scoring feasible candidate.
   - `micp-optimizer` converts scores into capacity-capped traffic
     shares (softmax over scores, then clip-and-renormalize by
     remaining RPS).

No step mutates shared state on the scoring path. SLO windows are the
only stateful component; they are updated from observed outcomes, not
from the decision itself.

## Constraints

The engine treats the five (plus traffic class) as hard filters first,
soft objectives second:

| Constraint | Hard filter | Soft objective |
| --- | --- | --- |
| Latency | `p99 <= max_latency_ms` | minimize |
| Quality | `quality >= min_quality` | maximize |
| Cost | `cost_per_1k <= max_cost` | minimize |
| Reliability | `error_rate <= max_error_rate` | maximize |
| Throughput / capacity | allocation caps at `capacity_rps` | prefer headroom |
| Traffic class | policy match | — |

If the feasible set is empty the API returns `409` / `NoFeasibleModel`
rather than silently relaxing constraints. Relaxation is a policy
choice, not a router fallback.

## Workload simulation

`micp-sim` emits a deterministic exponential inter-arrival process
(xorshift64, seed in the spec) mixed across traffic classes. Python
mirrors this for larger sweeps so experiment scripts do not have to
link against Rust. The two generators must agree on the arrival
contract (rate, duration, class mix, seed); they are not required to
produce bit-identical timestamps.

## Observability

`micp-api` initializes a `tracing` subscriber. The `telemetry` module
is the future OpenTelemetry attach point (resource attributes, W3C
trace context, metrics for decision latency, feasible-set size, SLO
burn). Exporters are intentionally not included in this revision:
they add a large dependency surface before there is a stable request
path to instrument.

## WASM boundary

Only *pure* functions cross into the browser: scoring a candidate and
computing SLO burn. Policy evaluation and allocation stay server-side.
The WASM crate uses a C ABI so `wasm32-unknown-unknown` + `rustc` is
sufficient; no bindgen toolchain is required to reproduce the module.

## What this repository is not

- A model server, tokenizer, or GPU scheduler
- A service mesh or generic API gateway
- A training / experiment-tracking platform
- A Kubernetes operator

Those systems consume or feed this control plane; they are not in-tree.
