# Serving models

All formulas in the Rust engine are **closed-form or seeded simulations**.
They are not empirical benchmarks and they are not production traces.
[`EstimateOrigin`](../crates/micp-core/src/domain.rs) is `modeled` or
`simulated`; `empirical` is reserved and unused.

## Units

Rust newtypes keep milliseconds, tokens, USD, request-rates, and
probabilities from being added together. Serialized JSON still uses
unit-bearing field names (`p99_ms`, `cost_per_request`) so the
TypeScript scaffolding does not need the wrappers.

## Latency

Mean service time (ms):

```
S = intercept
  + ms_per_input_token  * (prompt_mean + min(context_mean, budget))
  + ms_per_output_token * output_mean
  + retrieval_ms_per_k  * top_k          if retrieval ≠ none
  + rerank_ms                             if rerank ≠ none
  + max_wait_ms / 2                       if batching = window
```

Queueing is an **M/M/n approximation**: `n` is
`min(usable_concurrency, offered_concurrency)`, `λ` is the design
arrival rate (mean, or mean × peak_multiplier when bursty — peak
sizing is conservative). Occupancy `ρ = λ S / n`. When `ρ ≥ 1` the
route is marked saturated; wait is evaluated at `ρ = 0.99` and
capped at 60s so the numbers stay finite.

Sojourn `T = S + Wq` with `Wq = (ρ/(1-ρ)) * (S/n)`.

Percentiles assume a lognormal sojourn. `σ_ln = cv * (1 + 0.5 ρ)`
where `cv = sigma_ms / S`. Then

```
μ = ln(T) - σ_ln² / 2
p50 = exp(μ)
p95 = exp(μ + 1.64485 σ_ln)
p99 = exp(μ + 2.32635 σ_ln)
```

This is a first-order systems model, not a claim about a real GPU
runtime.

## Throughput and saturation

- If `ρ < 1`: throughput = λ
- If `ρ ≥ 1`: throughput = n / S  (replicas fully busy)
- `degraded_factor` scales **usable concurrency** (failed replicas),
  not per-token speed
- Exhaustion policy `Reject` makes saturation a hard capacity
  violation; `Queue` admits the wait into p99 instead

## Quality proxy

```
q = base
  + retrieval_gain_per_k * ln(1 + k) * fill    if retrieval ≠ none
  + rerank_gain * fill                         if rerank ≠ none
fill = 1 - exp(-context_used / saturation)
```

Clamped to `[0, 1]`. This is **not** judged groundedness, NDCG, or a
win-rate.

## Cost

```
USD/req = (prompt + context_used)/1000 * usd_per_1k_input
        + output/1000 * usd_per_1k_output
        + usd_per_retrieval     if retrieval ≠ none
        + usd_per_rerank        if rerank ≠ none
```

`cost_per_1k` is `USD/req * 1000`. Figures are list-price-like
assumptions.

## Reliability and SLO miss

```
P(T > SLO) = 1 - Φ((ln(SLO) - μ) / σ_ln)     (lognormal survival)
failure    = base_error_rate
           + P(T > SLO) * timeout_as_failure
           + max(0, ρ - 0.8) * saturation_error_slope
```

`timeout_as_failure = 0.5` means half of modeled SLO misses are
treated as failed requests (client timeout / retry). Independent of
the empirical SLO window in `micp-slo`.

Fallback activation (only if `fallback_to` is set):

```
max(P(T > SLO), clamp((ρ - 0.85) / 0.15, 0, 1))
```

or `1` when saturated. A heuristic, not a measured failover rate.

## Pareto recommendation

Feasible routes (no hard-constraint violations) are compared on four
objectives: minimize p99, maximize quality, minimize USD/req,
maximize `1 - failure`. The non-dominated set is the front.

The recommended point maximises the existing five-weight score
(latency, quality, cost, `1 - utilization`, reliability) **on the
front only**. Ties break on the lexicographically smaller route key
(`model:k:rerank:context:batch`).

## Simulation

`micp-sim::simulate` is a seeded G/G/n: exponential or piecewise-burst
arrivals, FCFS, service time from the same `S` (optional lognormal
noise when `long_tail` is set). Reject vs queue follows the
exhaustion policy. Reports are tagged `origin = simulated`.

## Inventory adapter

`InferenceCandidate::from_inventory` expands a compact
`ModelProfile` so the existing config/API fleet can participate.
Calibration (also an assumption): 40% of inventory p99 is intercept,
30% input at 1024 tokens, 30% output at 256 tokens; Little's law for
concurrency; 40/60 input/output cost split. Do not treat the result
as a fitted runtime model.

## What this will not do

- Fit latency from traces
- Own GPU scheduling, KV-cache, or continuous batching internals
- Produce empirically calibrated quality or cost
- Silently relax an SLO (degradation adds *extra* candidates;
  constraints stay monotonic)
