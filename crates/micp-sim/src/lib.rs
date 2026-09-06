//! Deterministic synthetic workloads.
//!
//! Inter-arrival times are exponential with rate `arrival_rate_rps`.
//! Traffic classes are drawn i.i.d. from `class_mix`. A tiny xorshift64
//! RNG is inlined so the crate has no `rand` dependency and a given
//! `(spec, seed)` pair is reproducible across compilers as long as
//! IEEE-754 `ln` is stable (it is, for the magnitude of values here).

mod queue;

pub use queue::{simulate, SimReport, SimSpec};

use micp_core::{MicpError, Result, TrafficClass};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClassWeight {
    pub class: TrafficClass,
    pub weight: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkloadSpec {
    pub arrival_rate_rps: f64,
    pub duration_s: f64,
    pub seed: u64,
    pub class_mix: Vec<ClassWeight>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimulatedRequest {
    pub t_s: f64,
    pub class: TrafficClass,
}

pub(crate) struct XorShift64(u64);

impl XorShift64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub(crate) fn next_f64(&mut self) -> f64 {
        let u = self.next_u64() >> 11;
        (u as f64) / ((1u64 << 53) as f64)
    }
}

pub fn generate(spec: &WorkloadSpec) -> Result<Vec<SimulatedRequest>> {
    if spec.arrival_rate_rps <= 0.0 {
        return Err(MicpError::InvalidConfig(
            "arrival_rate_rps must be positive".into(),
        ));
    }
    if spec.duration_s <= 0.0 {
        return Err(MicpError::InvalidConfig(
            "duration_s must be positive".into(),
        ));
    }
    let mix_sum: f64 = spec.class_mix.iter().map(|c| c.weight).sum();
    if spec.class_mix.is_empty() || mix_sum <= 0.0 {
        return Err(MicpError::InvalidConfig(
            "class_mix must have positive weight".into(),
        ));
    }

    let mut rng = XorShift64::new(spec.seed);
    let mut t = 0.0;
    let mut out = Vec::new();
    let expected = spec.arrival_rate_rps * spec.duration_s;
    out.reserve(expected.ceil() as usize + 8);

    while t < spec.duration_s {
        let u = rng.next_f64().clamp(1e-6, 1.0 - 1e-9);
        t += -u.ln() / spec.arrival_rate_rps;
        if t >= spec.duration_s {
            break;
        }
        let class = sample_class(&spec.class_mix, mix_sum, rng.next_f64());
        out.push(SimulatedRequest { t_s: t, class });
    }
    Ok(out)
}

fn sample_class(mix: &[ClassWeight], sum: f64, u: f64) -> TrafficClass {
    let mut acc = 0.0;
    let target = u * sum;
    for cw in mix {
        acc += cw.weight;
        if target <= acc {
            return cw.class;
        }
    }
    mix.last()
        .map(|c| c.class)
        .unwrap_or(TrafficClass::Interactive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(seed: u64) -> WorkloadSpec {
        WorkloadSpec {
            arrival_rate_rps: 50.0,
            duration_s: 2.0,
            seed,
            class_mix: vec![
                ClassWeight {
                    class: TrafficClass::Interactive,
                    weight: 0.8,
                },
                ClassWeight {
                    class: TrafficClass::Batch,
                    weight: 0.2,
                },
            ],
        }
    }

    #[test]
    fn same_seed_is_bit_identical() {
        let a = generate(&spec(7)).unwrap();
        let b = generate(&spec(7)).unwrap();
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn different_seeds_diverge() {
        let a = generate(&spec(7)).unwrap();
        let b = generate(&spec(8)).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn timestamps_are_monotonic_and_in_window() {
        let reqs = generate(&spec(1)).unwrap();
        let mut last = 0.0;
        for r in &reqs {
            assert!(r.t_s >= last);
            assert!(r.t_s < 2.0);
            last = r.t_s;
        }
        assert!(reqs.len() > 50 && reqs.len() < 200);
    }

    #[test]
    fn invalid_spec_is_rejected() {
        let mut s = spec(1);
        s.arrival_rate_rps = 0.0;
        assert!(generate(&s).is_err());
    }

    #[test]
    fn class_mix_is_roughly_honored() {
        let reqs = generate(&WorkloadSpec {
            arrival_rate_rps: 200.0,
            duration_s: 5.0,
            seed: 42,
            class_mix: vec![
                ClassWeight {
                    class: TrafficClass::Interactive,
                    weight: 0.9,
                },
                ClassWeight {
                    class: TrafficClass::Batch,
                    weight: 0.1,
                },
            ],
        })
        .unwrap();
        let interactive = reqs
            .iter()
            .filter(|r| r.class == TrafficClass::Interactive)
            .count() as f64;
        let frac = interactive / reqs.len() as f64;
        assert!(frac > 0.8 && frac < 0.98, "frac={frac}");
    }
}
