//! Browser-facing C ABI.
//!
//! Compiled with `--target wasm32-unknown-unknown`. Exports take and
//! return `f64` so the workbench can call them via `WebAssembly.instantiate`
//! without `wasm-bindgen`. `#[no_mangle]` is required for a stable C ABI
//! and is the only reason this crate allows `unsafe_code`.

#![allow(unsafe_code)]

use micp_core::{modeled_p99_ms, modeled_slo_violation_prob, ModelProfile, ObjectiveWeights};
use micp_slo::burn_rate;

/// Score one candidate. Returns `NaN` on non-finite input.
///
/// Argument order is stable and documented for the JS loader:
/// `latency_p99_ms, quality, cost_per_1k, capacity_rps, error_rate,
///  demand_rps, w_latency, w_quality, w_cost, w_throughput, w_reliability`.
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn micp_score_candidate(
    latency_p99_ms: f64,
    quality: f64,
    cost_per_1k: f64,
    capacity_rps: f64,
    error_rate: f64,
    demand_rps: f64,
    w_latency: f64,
    w_quality: f64,
    w_cost: f64,
    w_throughput: f64,
    w_reliability: f64,
) -> f64 {
    score_candidate(
        latency_p99_ms,
        quality,
        cost_per_1k,
        capacity_rps,
        error_rate,
        demand_rps,
        w_latency,
        w_quality,
        w_cost,
        w_throughput,
        w_reliability,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn score_candidate(
    latency_p99_ms: f64,
    quality: f64,
    cost_per_1k: f64,
    capacity_rps: f64,
    error_rate: f64,
    demand_rps: f64,
    w_latency: f64,
    w_quality: f64,
    w_cost: f64,
    w_throughput: f64,
    w_reliability: f64,
) -> f64 {
    let inputs = [
        latency_p99_ms,
        quality,
        cost_per_1k,
        capacity_rps,
        error_rate,
        demand_rps,
        w_latency,
        w_quality,
        w_cost,
        w_throughput,
        w_reliability,
    ];
    if inputs.iter().any(|x| !x.is_finite()) {
        return f64::NAN;
    }
    let model = ModelProfile {
        id: "wasm".into(),
        latency_p99_ms,
        quality,
        cost_per_1k_tokens: cost_per_1k,
        capacity_rps,
        error_rate,
    };
    let weights = ObjectiveWeights {
        latency: w_latency,
        quality: w_quality,
        cost: w_cost,
        throughput: w_throughput,
        reliability: w_reliability,
    };
    model.score(&weights, demand_rps)
}

#[no_mangle]
pub extern "C" fn micp_slo_burn_rate(success_target: f64, requests: f64, successes: f64) -> f64 {
    burn_rate(success_target, requests, successes)
}

/// Modeled p99 (ms) from the closed-form queueing model.
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn micp_modeled_p99_ms(
    intercept_ms: f64,
    ms_per_in: f64,
    ms_per_out: f64,
    in_tokens: f64,
    out_tokens: f64,
    extra_ms: f64,
    sigma_ms: f64,
    lambda_rps: f64,
    n_servers: f64,
) -> f64 {
    modeled_p99_ms(
        intercept_ms,
        ms_per_in,
        ms_per_out,
        in_tokens,
        out_tokens,
        extra_ms,
        sigma_ms,
        lambda_rps,
        n_servers,
    )
}

#[no_mangle]
pub extern "C" fn micp_slo_violation_prob(mean_ms: f64, sigma_ms: f64, slo_ms: f64) -> f64 {
    modeled_slo_violation_prob(mean_ms, sigma_ms, slo_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use micp_core::ObjectiveWeights;

    #[test]
    fn wasm_score_matches_core() {
        let model = ModelProfile {
            id: "wasm".into(),
            latency_p99_ms: 80.0,
            quality: 0.72,
            cost_per_1k_tokens: 0.04,
            capacity_rps: 120.0,
            error_rate: 0.004,
        };
        let w = ObjectiveWeights::balanced();
        let via_abi = score_candidate(
            80.0, 0.72, 0.04, 120.0, 0.004, 10.0, 0.2, 0.2, 0.2, 0.2, 0.2,
        );
        assert!((via_abi - model.score(&w, 10.0)).abs() < 1e-12);
    }

    #[test]
    fn non_finite_input_is_nan() {
        let v = score_candidate(f64::NAN, 0.5, 0.1, 10.0, 0.01, 1.0, 0.2, 0.2, 0.2, 0.2, 0.2);
        assert!(v.is_nan());
    }

    #[test]
    fn burn_rate_export_is_finite_for_healthy_window() {
        let v = micp_slo_burn_rate(0.999, 1000.0, 1000.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn modeled_p99_export_is_positive_finite() {
        let v = micp_modeled_p99_ms(30.0, 0.02, 0.08, 200.0, 100.0, 0.0, 8.0, 10.0, 8.0);
        assert!(v.is_finite() && v > 0.0);
        assert!(micp_slo_violation_prob(80.0, 10.0, 250.0).is_finite());
    }
}
