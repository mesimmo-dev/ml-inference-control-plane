//! Seeded discrete-event queueing simulation.
//!
//! Output is [`EstimateOrigin::Simulated`], never empirical. Arrival
//! times follow the existing exponential generator (or a piecewise
//! burst schedule). Service times come from the same closed-form
//! latency curve as the model, plus optional lognormal noise.

use micp_core::{
    mean_service_ms, EstimateOrigin, ExhaustionPolicy, RouteConfig, Tokens, WorkloadProfile,
};
use serde::{Deserialize, Serialize};

use crate::{generate, ClassWeight, WorkloadSpec, XorShift64};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimSpec {
    pub duration_s: f64,
    pub seed: u64,
    /// Multiply service time by a lognormal factor (long-tail).
    pub long_tail: bool,
    /// Extra scale on usable concurrency (degraded capacity). Applied
    /// on top of the candidate's own degraded_factor.
    pub capacity_factor: f64,
    /// Optional closed-model ramp: (start_concurrency, end_concurrency).
    pub concurrency_ramp: Option<(u32, u32)>,
    pub max_events: usize,
}

impl Default for SimSpec {
    fn default() -> Self {
        Self {
            duration_s: 5.0,
            seed: 1,
            long_tail: false,
            capacity_factor: 1.0,
            concurrency_ramp: None,
            max_events: 20_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimReport {
    pub origin: EstimateOrigin,
    pub n_arrivals: u64,
    pub n_served: u64,
    pub n_rejected: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub mean_sojourn_ms: f64,
    pub mean_wait_ms: f64,
    pub utilization: f64,
    pub notes: Vec<String>,
}

/// Run a deterministic G/G/n simulation of `route` under `workload`.
pub fn simulate(
    route: &RouteConfig,
    workload: &WorkloadProfile,
    spec: &SimSpec,
) -> Result<SimReport, micp_core::MicpError> {
    if spec.duration_s <= 0.0 || spec.max_events == 0 {
        return Err(micp_core::MicpError::InvalidConfig(
            "duration_s and max_events must be positive".into(),
        ));
    }
    let mut n = ((route.candidate.usable_concurrency() as f64) * spec.capacity_factor.max(0.0))
        .floor() as usize;
    if let Some((start, _)) = spec.concurrency_ramp {
        n = n.min(start.max(1) as usize);
    }
    let mut rng = XorShift64::new(spec.seed ^ 0x9E37_79B9);

    let mean_rps = workload.traffic.mean_rps.get().max(0.0);
    let arrivals = burst_arrivals(workload, spec, mean_rps)?;

    if n == 0 {
        return Ok(SimReport {
            origin: EstimateOrigin::Simulated,
            n_arrivals: arrivals.len() as u64,
            n_served: 0,
            n_rejected: arrivals.len() as u64,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            mean_sojourn_ms: 0.0,
            mean_wait_ms: 0.0,
            utilization: 1.0,
            notes: notes(spec, "no usable servers; all arrivals rejected"),
        });
    }

    let mut free_at = vec![0.0_f64; n];
    let mut sojourns = Vec::new();
    let mut waits = Vec::new();
    let mut rejected = 0u64;
    let mut busy = 0.0;

    let max_wait_s = match workload.exhaustion {
        ExhaustionPolicy::Reject => 0.0,
        ExhaustionPolicy::Queue { max_wait_ms } => max_wait_ms.get().max(0.0) / 1000.0,
    };

    for (idx, t) in arrivals.iter().copied().enumerate() {
        if idx >= spec.max_events {
            break;
        }
        let server = free_at
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap();
        let start = free_at[server].max(t);
        let wait = start - t;
        if wait > max_wait_s + 1e-12 {
            rejected += 1;
            continue;
        }
        let service_s = sample_service_s(route, workload, spec, &mut rng);
        let finish = start + service_s;
        busy += service_s;
        free_at[server] = finish;
        sojourns.push((finish - t) * 1000.0);
        waits.push(wait * 1000.0);
    }

    let served = sojourns.len() as u64;
    sojourns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let horizon = spec.duration_s.max(1e-9);
    let util = (busy / (horizon * n as f64)).clamp(0.0, 1.0);
    Ok(SimReport {
        origin: EstimateOrigin::Simulated,
        n_arrivals: arrivals.len().min(spec.max_events) as u64,
        n_served: served,
        n_rejected: rejected,
        p50_ms: percentile(&sojourns, 50.0),
        p95_ms: percentile(&sojourns, 95.0),
        p99_ms: percentile(&sojourns, 99.0),
        mean_sojourn_ms: mean(&sojourns),
        mean_wait_ms: mean(&waits),
        utilization: util,
        notes: notes(spec, "discrete-event G/G/n; not a production trace"),
    })
}

fn burst_arrivals(
    workload: &WorkloadProfile,
    spec: &SimSpec,
    mean_rps: f64,
) -> Result<Vec<f64>, micp_core::MicpError> {
    match workload.traffic.burst {
        None => {
            let ws = WorkloadSpec {
                arrival_rate_rps: mean_rps.max(1e-6),
                duration_s: spec.duration_s,
                seed: spec.seed,
                class_mix: vec![ClassWeight {
                    class: workload.traffic.class,
                    weight: 1.0,
                }],
            };
            Ok(generate(&ws)?.into_iter().map(|r| r.t_s).collect())
        }
        Some(b) => {
            // Piecewise-constant rate: duty_cycle of each period at peak.
            let period = b.period_s.max(1e-6);
            let duty = b.duty_cycle.clamp(1e-6, 1.0);
            let peak = mean_rps * b.peak_multiplier.max(0.0);
            let mut rng = XorShift64::new(spec.seed);
            let mut t = 0.0;
            let mut out = Vec::new();
            while t < spec.duration_s && out.len() < spec.max_events {
                let in_peak = (t % period) < period * duty;
                let rate = if in_peak {
                    peak.max(1e-6)
                } else {
                    mean_rps.max(1e-6)
                };
                let u = rng.next_f64().clamp(1e-6, 1.0 - 1e-9);
                t += -u.ln() / rate;
                if t < spec.duration_s {
                    out.push(t);
                }
            }
            Ok(out)
        }
    }
}

fn sample_service_s(
    route: &RouteConfig,
    workload: &WorkloadProfile,
    spec: &SimSpec,
    rng: &mut XorShift64,
) -> f64 {
    let mut wl = workload.clone();
    wl.prompt_tokens.mean = Tokens::new(sample_tokens(workload.prompt_tokens, rng));
    wl.context_tokens.mean = Tokens::new(sample_tokens(workload.context_tokens, rng));
    wl.expected_output_tokens.mean =
        Tokens::new(sample_tokens(workload.expected_output_tokens, rng));
    let mut ms = mean_service_ms(route, &wl);
    if spec.long_tail {
        let u = rng.next_f64().max(f64::EPSILON);
        // lognormal factor with median 1, σ≈0.4
        let z = approx_norminv(u);
        ms *= (0.4 * z).exp();
    }
    (ms / 1000.0).max(1e-6)
}

fn sample_tokens(dist: micp_core::TokenDistribution, rng: &mut XorShift64) -> f64 {
    let mean = dist.mean.get().max(0.0);
    if (dist.p99.get() - mean).abs() < 1e-9 {
        return mean;
    }
    let u = rng.next_f64().clamp(1e-6, 1.0 - 1e-6);
    // Interpolate between mean and p99 on a log-ish scale.
    if u < 0.5 {
        mean * (0.5 + u)
    } else {
        let t = (u - 0.5) / 0.5;
        mean + t * (dist.p99.get() - mean)
    }
}

fn approx_norminv(u: f64) -> f64 {
    // Beasley-Springer-Moro rational approximation, sufficient for sim noise.
    let a = [
        2.50662823884,
        -18.61500062529,
        41.39119773534,
        -25.44106049637,
    ];
    let b = [
        -8.47351093090,
        23.08336743743,
        -21.06224101826,
        3.13082909833,
    ];
    let c = [
        0.3374754822726147,
        0.9761690190273683,
        0.1607979714918209,
        0.0276438810333863,
        0.0038405729373609,
        0.0003951896511919,
        0.0000321767881768,
        0.0000002888167364,
        0.0000003960314277,
    ];
    let y = u - 0.5;
    if y.abs() < 0.42 {
        let r = y * y;
        y * (((a[3] * r + a[2]) * r + a[1]) * r + a[0])
            / ((((b[3] * r + b[2]) * r + b[1]) * r + b[0]) * r + 1.0)
    } else {
        let mut r = if y > 0.0 { 1.0 - u } else { u };
        r = (-r.ln()).ln();
        let mut x = c[0];
        let mut p = r;
        for ci in c.iter().skip(1) {
            x += ci * p;
            p *= r;
        }
        if y < 0.0 {
            -x
        } else {
            x
        }
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((q / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn notes(spec: &SimSpec, extra: &str) -> Vec<String> {
    vec![
        extra.into(),
        format!(
            "seed={}, duration_s={}, long_tail={}, capacity_factor={}",
            spec.seed, spec.duration_s, spec.long_tail, spec.capacity_factor
        ),
        "values are simulated, not production measurements".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use micp_core::scenarios::interactive_assistant;
    use micp_core::RouteConfig;

    fn setup() -> (RouteConfig, WorkloadProfile) {
        let s = interactive_assistant();
        let cand = s.fleet.iter().find(|c| c.id.as_str() == "fast-8b").unwrap();
        (RouteConfig::baseline(cand.clone(), &s.workload), s.workload)
    }

    #[test]
    fn same_seed_is_reproducible() {
        let (r, w) = setup();
        let spec = SimSpec {
            duration_s: 2.0,
            seed: 9,
            max_events: 5000,
            ..SimSpec::default()
        };
        let a = simulate(&r, &w, &spec).unwrap();
        let b = simulate(&r, &w, &spec).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.origin, EstimateOrigin::Simulated);
        assert!(a.n_served > 0);
    }

    #[test]
    fn different_seeds_diverge() {
        let (r, w) = setup();
        let mut spec = SimSpec {
            duration_s: 2.0,
            seed: 1,
            long_tail: true,
            max_events: 5000,
            ..SimSpec::default()
        };
        let a = simulate(&r, &w, &spec).unwrap();
        spec.seed = 2;
        let b = simulate(&r, &w, &spec).unwrap();
        assert_ne!(
            (a.p99_ms, a.mean_sojourn_ms, a.n_served),
            (b.p99_ms, b.mean_sojourn_ms, b.n_served)
        );
    }

    #[test]
    fn long_tail_raises_p99() {
        let (r, w) = setup();
        let mut spec = SimSpec {
            duration_s: 3.0,
            seed: 4,
            long_tail: false,
            max_events: 8000,
            ..SimSpec::default()
        };
        let base = simulate(&r, &w, &spec).unwrap();
        spec.long_tail = true;
        let tail = simulate(&r, &w, &spec).unwrap();
        assert!(tail.p99_ms >= base.p99_ms);
    }

    #[test]
    fn degraded_capacity_rejects_more() {
        let (r, w) = setup();
        let mut spec = SimSpec {
            duration_s: 2.0,
            seed: 3,
            capacity_factor: 1.0,
            max_events: 5000,
            ..SimSpec::default()
        };
        let healthy = simulate(&r, &w, &spec).unwrap();
        spec.capacity_factor = 0.0;
        let down = simulate(&r, &w, &spec).unwrap();
        assert_eq!(down.n_served, 0);
        assert!(down.n_rejected >= healthy.n_rejected);
    }

    #[test]
    fn percentiles_ordered_when_served() {
        let (r, w) = setup();
        let spec = SimSpec {
            duration_s: 2.0,
            seed: 11,
            max_events: 4000,
            ..SimSpec::default()
        };
        let rep = simulate(&r, &w, &spec).unwrap();
        assert!(rep.p50_ms <= rep.p95_ms + 1e-9);
        assert!(rep.p95_ms <= rep.p99_ms + 1e-9);
    }
}
