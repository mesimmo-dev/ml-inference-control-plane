//! Closed-form route estimates.
//!
//! Every quantity here is **modeled**, not measured. See `docs/models.md`
//! for the latency, queueing, cost, quality, and reliability formulas.

use crate::domain::{
    BatchingPolicy, ConstraintKind, ConstraintViolation, EstimateOrigin, ExhaustionPolicy,
    RerankStrategy, RetrievalStrategy, RouteConfig, RouteEstimate, WorkloadProfile,
};
use crate::units::{Milliseconds, Probability, Quality, Rps, Usd};
use crate::{MicpError, Result};

const P95_Z: f64 = 1.644_853_626_951_472_2;
const P99_Z: f64 = 2.326_347_874_040_840_8;
const MAX_SOJOURN_MS: f64 = 120_000.0;
const MAX_TOP_K: u32 = 1_000;
const MAX_CONTEXT: f64 = 1_000_000.0;

/// Estimate one route under a workload. Pure and deterministic.
pub fn estimate_route(route: &RouteConfig, workload: &WorkloadProfile) -> Result<RouteEstimate> {
    validate(route, workload)?;

    let service_ms = mean_service_ms(route, workload);
    let n = route
        .candidate
        .usable_concurrency()
        .min(workload.traffic.concurrency.max(1)) as f64;
    let lambda = workload.traffic.design_rps();
    let s_sec = service_ms / 1000.0;

    let (rho_raw, saturated) = if n <= 0.0 || !s_sec.is_finite() || s_sec <= 0.0 {
        (f64::INFINITY, true)
    } else {
        let r = lambda * s_sec / n;
        (r, !r.is_finite() || r >= 1.0)
    };
    let rho = if saturated { 1.0 } else { rho_raw.max(0.0) };
    let rho_q = if saturated { 0.99 } else { rho };

    let wait_ms = if n <= 0.0 {
        MAX_SOJOURN_MS
    } else {
        ((rho_q / (1.0 - rho_q)) * (service_ms / n)).min(60_000.0)
    };
    let sojourn = (service_ms + wait_ms).clamp(0.1, MAX_SOJOURN_MS);

    let cv = (route.candidate.latency.sigma_ms.get() / service_ms.max(1.0)).clamp(0.0, 1.2);
    let sigma_ln = cv * (1.0 + 0.5 * rho);
    let p50 = lognormal_percentile(sojourn, sigma_ln, 0.0).min(MAX_SOJOURN_MS);
    let p95 = lognormal_percentile(sojourn, sigma_ln, P95_Z)
        .max(p50)
        .min(MAX_SOJOURN_MS);
    let p99 = lognormal_percentile(sojourn, sigma_ln, P99_Z)
        .max(p95)
        .min(MAX_SOJOURN_MS);

    let quality = quality_proxy(route, workload);
    let cost = cost_per_request(route, workload);
    let slo_ms = workload.constraints.latency_slo.get();
    let slo_violation = Probability::clamp(lognormal_sf(sojourn, sigma_ln, slo_ms.max(0.0)));

    let sat_extra = if rho > 0.8 {
        (rho - 0.8) * route.candidate.reliability.saturation_error_slope.max(0.0)
    } else {
        0.0
    };
    let timeout_term = slo_violation.get()
        * route
            .candidate
            .reliability
            .timeout_as_failure
            .clamp(0.0, 1.0);
    let failure = if n <= 0.0 {
        1.0
    } else {
        (route.candidate.reliability.base_error_rate.get() + timeout_term + sat_extra)
            .clamp(0.0, 1.0)
    };

    let throughput = if n <= 0.0 || s_sec <= 0.0 {
        0.0
    } else if saturated {
        n / s_sec
    } else {
        lambda
    };

    let fallback_p = if route.candidate.fallback_to.is_some() {
        if saturated {
            1.0
        } else {
            slo_violation
                .get()
                .max(((rho - 0.85).max(0.0) / 0.15).min(1.0))
        }
    } else {
        0.0
    };

    Ok(RouteEstimate {
        origin: EstimateOrigin::Modeled,
        route_key: route.key(),
        model_id: route.candidate.id.clone(),
        p50_ms: Milliseconds::new(p50),
        p95_ms: Milliseconds::new(p95),
        p99_ms: Milliseconds::new(p99),
        throughput_rps: Rps::new(throughput),
        utilization: rho.clamp(0.0, 1.0),
        saturated,
        quality: Quality::clamp(quality),
        cost_per_request: Usd::new(cost),
        cost_per_1k: Usd::new(cost * 1000.0),
        slo_violation_prob: slo_violation,
        failure_prob: Probability::clamp(failure),
        fallback_activation_prob: Probability::clamp(fallback_p),
    })
}

/// Hard-constraint check against the workload SLO. Does not relax bounds.
pub fn constraint_violations(
    estimate: &RouteEstimate,
    workload: &WorkloadProfile,
) -> Vec<ConstraintViolation> {
    let c = &workload.constraints;
    let mut out = Vec::new();
    if estimate.p99_ms.get() > c.latency_slo.get() {
        out.push(ConstraintViolation {
            kind: ConstraintKind::Latency,
            message: format!(
                "modeled p99 {:.1}ms exceeds SLO {:.1}ms",
                estimate.p99_ms.get(),
                c.latency_slo.get()
            ),
            observed: estimate.p99_ms.get(),
            limit: c.latency_slo.get(),
        });
    }
    if estimate.quality.get() < c.quality_floor.get() {
        out.push(ConstraintViolation {
            kind: ConstraintKind::Quality,
            message: format!(
                "modeled quality {:.3} below floor {:.3}",
                estimate.quality.get(),
                c.quality_floor.get()
            ),
            observed: estimate.quality.get(),
            limit: c.quality_floor.get(),
        });
    }
    if estimate.cost_per_request.get() > c.cost_ceiling_per_request.get() {
        out.push(ConstraintViolation {
            kind: ConstraintKind::Cost,
            message: format!(
                "modeled cost ${:.5}/req exceeds ceiling ${:.5}",
                estimate.cost_per_request.get(),
                c.cost_ceiling_per_request.get()
            ),
            observed: estimate.cost_per_request.get(),
            limit: c.cost_ceiling_per_request.get(),
        });
    }
    let reliability = 1.0 - estimate.failure_prob.get();
    if reliability < c.reliability_target.get() {
        out.push(ConstraintViolation {
            kind: ConstraintKind::Reliability,
            message: format!(
                "modeled reliability {:.4} below target {:.4}",
                reliability,
                c.reliability_target.get()
            ),
            observed: reliability,
            limit: c.reliability_target.get(),
        });
    }
    if estimate.throughput_rps.get() + 1e-9 < c.min_capacity.get() {
        out.push(ConstraintViolation {
            kind: ConstraintKind::Capacity,
            message: format!(
                "modeled throughput {:.2} rps below required {:.2} rps",
                estimate.throughput_rps.get(),
                c.min_capacity.get()
            ),
            observed: estimate.throughput_rps.get(),
            limit: c.min_capacity.get(),
        });
    }
    if estimate.saturated && matches!(workload.exhaustion, ExhaustionPolicy::Reject) {
        out.push(ConstraintViolation {
            kind: ConstraintKind::Capacity,
            message: "modeled occupancy saturates replica capacity; exhaustion policy is reject"
                .into(),
            observed: estimate.utilization,
            limit: 1.0,
        });
    }
    out
}

pub fn is_feasible(estimate: &RouteEstimate, workload: &WorkloadProfile) -> bool {
    constraint_violations(estimate, workload).is_empty()
}

pub fn mean_service_ms(route: &RouteConfig, workload: &WorkloadProfile) -> f64 {
    let lat = &route.candidate.latency;
    let prompt = workload.prompt_tokens.mean.get().max(0.0);
    let ctx = workload
        .context_tokens
        .mean
        .get()
        .min(route.context_budget.get().max(0.0))
        .max(0.0);
    let out = workload.expected_output_tokens.mean.get().max(0.0);
    let mut s = lat.intercept_ms.get()
        + lat.ms_per_input_token.max(0.0) * (prompt + ctx)
        + lat.ms_per_output_token.max(0.0) * out;
    let k = route.retrieval.effective_k().min(MAX_TOP_K) as f64;
    if route.retrieval.strategy != RetrievalStrategy::None {
        s += lat.retrieval_ms_per_k.get().max(0.0) * k;
    }
    if route.retrieval.rerank != RerankStrategy::None {
        s += lat.rerank_ms.get().max(0.0);
    }
    if let BatchingPolicy::Window { max_wait_ms, .. } = route.batching {
        s += max_wait_ms.get().max(0.0) * 0.5;
    }
    s.max(0.1)
}

pub fn quality_proxy(route: &RouteConfig, workload: &WorkloadProfile) -> f64 {
    let q = &route.candidate.quality;
    let mut v = q.base.get();
    let ctx = workload
        .context_tokens
        .mean
        .get()
        .min(route.context_budget.get())
        .max(0.0);
    let sat = q.context_saturation.get().max(1.0);
    let fill = 1.0 - (-ctx / sat).exp();
    if route.retrieval.strategy != RetrievalStrategy::None {
        let k = route.retrieval.effective_k().min(MAX_TOP_K) as f64;
        v += q.retrieval_gain_per_k.max(0.0) * (1.0 + k).ln() * fill;
    }
    if route.retrieval.rerank != RerankStrategy::None {
        v += q.rerank_gain.max(0.0) * fill;
    }
    v.clamp(0.0, 1.0)
}

pub fn cost_per_request(route: &RouteConfig, workload: &WorkloadProfile) -> f64 {
    let c = &route.candidate.cost;
    let prompt = workload.prompt_tokens.mean.get().max(0.0);
    let ctx = workload
        .context_tokens
        .mean
        .get()
        .min(route.context_budget.get().max(0.0))
        .max(0.0);
    let out = workload.expected_output_tokens.mean.get().max(0.0);
    let mut usd = (prompt + ctx) / 1000.0 * c.usd_per_1k_input.get().max(0.0)
        + out / 1000.0 * c.usd_per_1k_output.get().max(0.0);
    if route.retrieval.strategy != RetrievalStrategy::None {
        usd += c.usd_per_retrieval.get().max(0.0);
    }
    if route.retrieval.rerank != RerankStrategy::None {
        usd += c.usd_per_rerank.get().max(0.0);
    }
    usd.max(0.0)
}

/// Abramowitz & Stegun 7.1.26.
fn erf_approx(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

fn norm_cdf(z: f64) -> f64 {
    if z <= -8.0 {
        return 0.0;
    }
    if z >= 8.0 {
        return 1.0;
    }
    0.5 * (1.0 + erf_approx(z / std::f64::consts::SQRT_2))
}

fn lognormal_percentile(mean: f64, sigma_ln: f64, z: f64) -> f64 {
    if mean <= 0.0 {
        return 0.0;
    }
    if sigma_ln <= 1e-12 {
        return mean;
    }
    let mu = mean.ln() - 0.5 * sigma_ln * sigma_ln;
    (mu + z * sigma_ln).exp()
}

fn lognormal_sf(mean: f64, sigma_ln: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if mean <= 0.0 {
        return 0.0;
    }
    if sigma_ln <= 1e-12 {
        return if mean > x { 1.0 } else { 0.0 };
    }
    let mu = mean.ln() - 0.5 * sigma_ln * sigma_ln;
    let z = (x.ln() - mu) / sigma_ln;
    1.0 - norm_cdf(z)
}

fn validate(route: &RouteConfig, workload: &WorkloadProfile) -> Result<()> {
    let checks = [
        ("mean_rps", workload.traffic.mean_rps.get()),
        ("prompt_mean", workload.prompt_tokens.mean.get()),
        ("context_mean", workload.context_tokens.mean.get()),
        ("output_mean", workload.expected_output_tokens.mean.get()),
        ("latency_slo", workload.constraints.latency_slo.get()),
        (
            "cost_ceiling",
            workload.constraints.cost_ceiling_per_request.get(),
        ),
        ("min_capacity", workload.constraints.min_capacity.get()),
        ("intercept_ms", route.candidate.latency.intercept_ms.get()),
        ("context_budget", route.context_budget.get()),
    ];
    for (name, v) in checks {
        if !v.is_finite() || v < 0.0 {
            return Err(MicpError::InvalidConfig(format!(
                "{name} must be finite and non-negative"
            )));
        }
    }
    if !workload.constraints.quality_floor.is_valid() {
        return Err(MicpError::InvalidConfig(
            "quality_floor must be in [0, 1]".into(),
        ));
    }
    if !workload.constraints.reliability_target.is_valid() {
        return Err(MicpError::InvalidConfig(
            "reliability_target must be in [0, 1]".into(),
        ));
    }
    if route.context_budget.get() > MAX_CONTEXT {
        return Err(MicpError::InvalidConfig(
            "context_budget exceeds engine cap".into(),
        ));
    }
    if let Some(b) = workload.traffic.burst {
        if !b.peak_multiplier.is_finite() || b.peak_multiplier <= 0.0 {
            return Err(MicpError::InvalidConfig(
                "burst peak_multiplier must be positive and finite".into(),
            ));
        }
    }
    Ok(())
}

/// Scalar helpers used by the WASM ABI (no struct marshaling).
#[allow(clippy::too_many_arguments)]
pub fn modeled_p99_ms(
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
    if ![
        intercept_ms,
        ms_per_in,
        ms_per_out,
        in_tokens,
        out_tokens,
        extra_ms,
        sigma_ms,
        lambda_rps,
        n_servers,
    ]
    .iter()
    .all(|x| x.is_finite() && *x >= 0.0)
    {
        return f64::NAN;
    }
    let service =
        (intercept_ms + ms_per_in * in_tokens + ms_per_out * out_tokens + extra_ms).max(0.1);
    let n = n_servers;
    let s_sec = service / 1000.0;
    let saturated = n <= 0.0 || lambda_rps * s_sec / n >= 1.0;
    let rho_q = if saturated {
        0.99
    } else {
        lambda_rps * s_sec / n
    };
    let wait = if n <= 0.0 {
        MAX_SOJOURN_MS
    } else {
        ((rho_q / (1.0 - rho_q)) * (service / n)).min(60_000.0)
    };
    let sojourn = (service + wait).clamp(0.1, MAX_SOJOURN_MS);
    let cv = (sigma_ms / service.max(1.0)).clamp(0.0, 1.2);
    let sigma_ln = cv * (1.0 + 0.5 * if saturated { 1.0 } else { rho_q });
    lognormal_percentile(sojourn, sigma_ln, P99_Z).min(MAX_SOJOURN_MS)
}

pub fn modeled_slo_violation_prob(mean_ms: f64, sigma_ms: f64, slo_ms: f64) -> f64 {
    if ![mean_ms, sigma_ms, slo_ms]
        .iter()
        .all(|x| x.is_finite() && *x >= 0.0)
    {
        return f64::NAN;
    }
    let cv = (sigma_ms / mean_ms.max(1.0)).clamp(0.0, 1.2);
    lognormal_sf(mean_ms.max(0.1), cv, slo_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        BatchingPolicy, BurstSpec, CapacityModel, CostModel, InferenceCandidate, LatencyCurve,
        QualityModel, ReliabilityModel, RetrievalConfig, RetrievalStrategy, SloConstraints,
        TokenDistribution, TrafficProfile,
    };
    use crate::units::Tokens;
    use crate::TrafficClass;

    fn candidate(id: &str) -> InferenceCandidate {
        InferenceCandidate {
            id: id.into(),
            latency: LatencyCurve {
                intercept_ms: Milliseconds::new(30.0),
                ms_per_input_token: 0.02,
                ms_per_output_token: 0.08,
                retrieval_ms_per_k: Milliseconds::new(2.0),
                rerank_ms: Milliseconds::new(20.0),
                sigma_ms: Milliseconds::new(8.0),
            },
            quality: QualityModel {
                base: Quality::new(0.72),
                retrieval_gain_per_k: 0.03,
                rerank_gain: 0.05,
                context_saturation: Tokens::new(2048.0),
            },
            cost: CostModel {
                usd_per_1k_input: Usd::new(0.04),
                usd_per_1k_output: Usd::new(0.08),
                usd_per_retrieval: Usd::new(0.0002),
                usd_per_rerank: Usd::new(0.0004),
            },
            reliability: ReliabilityModel {
                base_error_rate: Probability::new(0.004),
                timeout_as_failure: 0.5,
                saturation_error_slope: 0.2,
            },
            capacity: CapacityModel {
                max_concurrency: 16,
                max_tokens_per_sec: 40_000.0,
                degraded_factor: 1.0,
            },
            retrieval_capable: true,
            fallback_to: Some("fast".into()),
        }
    }

    fn workload() -> WorkloadProfile {
        WorkloadProfile {
            id: "test".into(),
            traffic: TrafficProfile {
                class: TrafficClass::Interactive,
                mean_rps: Rps::new(10.0),
                concurrency: 8,
                burst: None,
            },
            prompt_tokens: TokenDistribution::fixed(200.0),
            context_tokens: TokenDistribution::fixed(400.0),
            expected_output_tokens: TokenDistribution::fixed(100.0),
            retrieval: RetrievalConfig {
                strategy: RetrievalStrategy::Dense,
                top_k: 8,
                rerank: RerankStrategy::None,
                context_budget: Tokens::new(1024.0),
            },
            batching: BatchingPolicy::None,
            constraints: SloConstraints {
                latency_slo: Milliseconds::new(500.0),
                quality_floor: Quality::new(0.5),
                cost_ceiling_per_request: Usd::new(0.05),
                reliability_target: Probability::new(0.9),
                min_capacity: Rps::new(1.0),
            },
            exhaustion: ExhaustionPolicy::Reject,
        }
    }

    fn route_for(w: &WorkloadProfile) -> RouteConfig {
        RouteConfig::baseline(candidate("m1"), w)
    }

    #[test]
    fn percentiles_are_ordered() {
        let w = workload();
        let e = estimate_route(&route_for(&w), &w).unwrap();
        assert_eq!(e.origin, EstimateOrigin::Modeled);
        assert!(e.p50_ms.get() <= e.p95_ms.get() + 1e-9);
        assert!(e.p95_ms.get() <= e.p99_ms.get() + 1e-9);
        assert!(e.p50_ms.get() > 0.0);
    }

    #[test]
    fn higher_load_increases_p99() {
        let mut w = workload();
        let low = estimate_route(&route_for(&w), &w).unwrap();
        w.traffic.mean_rps = Rps::new(40.0);
        let high = estimate_route(&route_for(&w), &w).unwrap();
        assert!(high.p99_ms.get() > low.p99_ms.get());
        assert!(high.utilization > low.utilization);
    }

    #[test]
    fn reducing_top_k_reduces_latency() {
        let w = workload();
        let mut r = route_for(&w);
        let hi = estimate_route(&r, &w).unwrap();
        r.retrieval.top_k = 2;
        let lo = estimate_route(&r, &w).unwrap();
        assert!(lo.p50_ms.get() < hi.p50_ms.get());
    }

    #[test]
    fn disabling_rerank_reduces_latency_and_quality() {
        let mut w = workload();
        w.retrieval.rerank = RerankStrategy::CrossEncoder;
        let mut r = route_for(&w);
        r.retrieval.rerank = RerankStrategy::CrossEncoder;
        let with = estimate_route(&r, &w).unwrap();
        r.retrieval.rerank = RerankStrategy::None;
        let without = estimate_route(&r, &w).unwrap();
        assert!(without.p99_ms.get() < with.p99_ms.get());
        assert!(without.quality.get() < with.quality.get());
        assert!(without.cost_per_request.get() < with.cost_per_request.get());
    }

    #[test]
    fn cost_increases_with_tokens() {
        let mut w = workload();
        let a = estimate_route(&route_for(&w), &w).unwrap();
        w.prompt_tokens = TokenDistribution::fixed(2000.0);
        let b = estimate_route(&route_for(&w), &w).unwrap();
        assert!(b.cost_per_request.get() > a.cost_per_request.get());
        assert!((b.cost_per_1k.get() - b.cost_per_request.get() * 1000.0).abs() < 1e-9);
    }

    #[test]
    fn zero_concurrency_is_saturated_failure() {
        let w = workload();
        let mut r = route_for(&w);
        r.candidate.capacity.degraded_factor = 0.0;
        let e = estimate_route(&r, &w).unwrap();
        assert!(e.saturated);
        assert_eq!(e.throughput_rps.get(), 0.0);
        assert!((e.failure_prob.get() - 1.0).abs() < 1e-12);
        assert!(!is_feasible(&e, &w));
        assert!(constraint_violations(&e, &w)
            .iter()
            .any(|v| v.kind == ConstraintKind::Capacity));
    }

    #[test]
    fn tight_slo_is_a_latency_violation() {
        let mut w = workload();
        w.constraints.latency_slo = Milliseconds::new(1.0);
        let e = estimate_route(&route_for(&w), &w).unwrap();
        assert!(e.slo_violation_prob.get() > 0.5);
        assert!(constraint_violations(&e, &w)
            .iter()
            .any(|v| v.kind == ConstraintKind::Latency));
    }

    #[test]
    fn quality_floor_violation() {
        let mut w = workload();
        w.constraints.quality_floor = Quality::new(0.99);
        w.retrieval.strategy = RetrievalStrategy::None;
        let mut r = route_for(&w);
        r.retrieval.strategy = RetrievalStrategy::None;
        let e = estimate_route(&r, &w).unwrap();
        assert!(constraint_violations(&e, &w)
            .iter()
            .any(|v| v.kind == ConstraintKind::Quality));
    }

    #[test]
    fn cost_ceiling_violation() {
        let mut w = workload();
        w.constraints.cost_ceiling_per_request = Usd::new(1e-9);
        let e = estimate_route(&route_for(&w), &w).unwrap();
        assert!(constraint_violations(&e, &w)
            .iter()
            .any(|v| v.kind == ConstraintKind::Cost));
    }

    #[test]
    fn nan_input_is_rejected() {
        let mut w = workload();
        w.traffic.mean_rps = Rps::new(f64::NAN);
        assert!(estimate_route(&route_for(&w), &w).is_err());
    }

    #[test]
    fn negative_slo_is_rejected() {
        let mut w = workload();
        w.constraints.latency_slo = Milliseconds::new(-1.0);
        assert!(estimate_route(&route_for(&w), &w).is_err());
    }

    #[test]
    fn burst_sizes_to_peak() {
        let mut w = workload();
        let steady = estimate_route(&route_for(&w), &w).unwrap();
        w.traffic.burst = Some(BurstSpec {
            peak_multiplier: 20.0,
            period_s: 30.0,
            duty_cycle: 0.2,
        });
        let burst = estimate_route(&route_for(&w), &w).unwrap();
        assert!(burst.utilization > steady.utilization);
    }

    #[test]
    fn queue_policy_does_not_emit_reject_capacity_on_saturation() {
        let mut w = workload();
        w.traffic.mean_rps = Rps::new(10_000.0);
        w.exhaustion = ExhaustionPolicy::Queue {
            max_wait_ms: Milliseconds::new(5_000.0),
        };
        w.constraints.min_capacity = Rps::new(0.0);
        w.constraints.latency_slo = Milliseconds::new(MAX_SOJOURN_MS);
        w.constraints.reliability_target = Probability::new(0.0);
        let e = estimate_route(&route_for(&w), &w).unwrap();
        assert!(e.saturated);
        assert!(!constraint_violations(&e, &w)
            .iter()
            .any(|v| v.kind == ConstraintKind::Capacity
                && v.message.contains("exhaustion policy is reject")));
    }

    #[test]
    fn wasm_scalar_is_finite() {
        let v = modeled_p99_ms(30.0, 0.02, 0.08, 200.0, 100.0, 16.0, 8.0, 10.0, 8.0);
        assert!(v.is_finite() && v > 0.0);
        let p = modeled_slo_violation_prob(80.0, 10.0, 250.0);
        assert!(p.is_finite() && (0.0..=1.0).contains(&p));
        assert!(modeled_p99_ms(f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0).is_nan());
    }

    #[test]
    fn sigma_zero_collapses_percentiles() {
        let w = workload();
        let mut r = route_for(&w);
        r.candidate.latency.sigma_ms = Milliseconds::ZERO;
        w_force_low_rho(&mut r);
        let e = estimate_route(&r, &w).unwrap();
        assert!((e.p99_ms.get() - e.p50_ms.get()).abs() < 1.0 || e.saturated);
    }

    fn w_force_low_rho(r: &mut RouteConfig) {
        r.candidate.capacity.max_concurrency = 64;
        r.candidate.capacity.degraded_factor = 1.0;
    }

    #[test]
    fn retrieval_none_ignores_top_k_latency() {
        let w = workload();
        let mut r = route_for(&w);
        r.retrieval.strategy = RetrievalStrategy::None;
        r.retrieval.top_k = 100;
        let a = estimate_route(&r, &w).unwrap();
        r.retrieval.top_k = 1;
        let b = estimate_route(&r, &w).unwrap();
        assert!((a.p50_ms.get() - b.p50_ms.get()).abs() < 1e-9);
    }
}
