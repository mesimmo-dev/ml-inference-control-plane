//! SLO tracking: success ratio, remaining error budget, and burn rate.
//!
//! A request is a success when it both completes without error *and*
//! meets the latency budget. Burn rate is actual error rate divided by
//! the allowed error rate (`1 - success_target`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SloSpec {
    pub name: String,
    /// Target success ratio in `(0, 1]`, e.g. `0.999`.
    pub success_target: f64,
    pub latency_ms: f64,
}

impl SloSpec {
    pub fn allowed_error_rate(&self) -> f64 {
        (1.0 - self.success_target).max(0.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SloState {
    pub spec: SloSpec,
    pub requests: u64,
    pub successes: u64,
}

impl SloState {
    pub fn new(spec: SloSpec) -> Self {
        Self {
            spec,
            requests: 0,
            successes: 0,
        }
    }

    pub fn record(&mut self, ok: bool, latency_ms: f64) {
        self.requests += 1;
        if ok && latency_ms <= self.spec.latency_ms {
            self.successes += 1;
        }
    }

    pub fn success_ratio(&self) -> f64 {
        if self.requests == 0 {
            1.0
        } else {
            self.successes as f64 / self.requests as f64
        }
    }

    pub fn error_rate(&self) -> f64 {
        1.0 - self.success_ratio()
    }

    /// Fraction of the error budget still unused. `1.0` is a fully
    /// healthy window; `0.0` is fully burned. Uninitialized windows
    /// report `1.0`.
    pub fn error_budget_remaining(&self) -> f64 {
        let allowed = self.spec.allowed_error_rate();
        if allowed <= 0.0 {
            return if self.error_rate() <= 0.0 { 1.0 } else { 0.0 };
        }
        (1.0 - self.error_rate() / allowed).clamp(0.0, 1.0)
    }

    /// How fast the budget is being consumed relative to a perfectly
    /// paced burn (`1.0` = on pace to exhaust at the end of the window).
    pub fn burn_rate(&self) -> f64 {
        let allowed = self.spec.allowed_error_rate();
        if allowed <= 0.0 {
            return if self.error_rate() <= 0.0 {
                0.0
            } else {
                f64::INFINITY
            };
        }
        self.error_rate() / allowed
    }

    /// Whether new traffic should be admitted given a remaining-budget floor.
    pub fn admitting(&self, min_error_budget: f64) -> bool {
        self.error_budget_remaining() >= min_error_budget
    }
}

/// Compare a *modeled* SLO-miss probability to the allowed error rate.
/// This does not update the empirical window.
pub fn modeled_meets_target(spec: &SloSpec, slo_violation_prob: f64) -> bool {
    if !slo_violation_prob.is_finite() {
        return false;
    }
    slo_violation_prob <= spec.allowed_error_rate() + 1e-12
}

/// Scalar helper used by the WASM ABI (no struct marshaling).
pub fn burn_rate(success_target: f64, requests: f64, successes: f64) -> f64 {
    if !(success_target.is_finite() && requests.is_finite() && successes.is_finite()) {
        return f64::NAN;
    }
    let allowed = (1.0 - success_target).max(0.0);
    if requests <= 0.0 {
        return 0.0;
    }
    let error_rate = 1.0 - (successes / requests).clamp(0.0, 1.0);
    if allowed <= 0.0 {
        return if error_rate <= 0.0 {
            0.0
        } else {
            f64::INFINITY
        };
    }
    error_rate / allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SloSpec {
        SloSpec {
            name: "interactive-p99".into(),
            success_target: 0.999,
            latency_ms: 250.0,
        }
    }

    #[test]
    fn empty_window_is_healthy() {
        let s = SloState::new(spec());
        assert_eq!(s.success_ratio(), 1.0);
        assert_eq!(s.error_budget_remaining(), 1.0);
        assert_eq!(s.burn_rate(), 0.0);
        assert!(s.admitting(0.2));
    }

    #[test]
    fn one_failure_in_thousand_is_on_pace() {
        let mut s = SloState::new(spec());
        for _ in 0..999 {
            s.record(true, 80.0);
        }
        s.record(false, 80.0);
        assert!((s.error_rate() - 0.001).abs() < 1e-12);
        assert!((s.burn_rate() - 1.0).abs() < 1e-9);
        assert!((s.error_budget_remaining() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn latency_miss_counts_as_error() {
        let mut s = SloState::new(spec());
        s.record(true, 400.0);
        assert_eq!(s.successes, 0);
        assert_eq!(s.requests, 1);
    }

    #[test]
    fn ten_x_burn_trips_admission_floor() {
        let mut s = SloState::new(spec());
        for _ in 0..990 {
            s.record(true, 80.0);
        }
        for _ in 0..10 {
            s.record(false, 80.0);
        }
        assert!((s.burn_rate() - 10.0).abs() < 1e-9);
        assert!(!s.admitting(0.2));
    }

    #[test]
    fn wasm_scalar_agrees_with_state() {
        let mut s = SloState::new(spec());
        for _ in 0..100 {
            s.record(true, 80.0);
        }
        s.record(false, 80.0);
        let scalar = burn_rate(0.999, s.requests as f64, s.successes as f64);
        assert!((scalar - s.burn_rate()).abs() < 1e-12);
    }

    #[test]
    fn modeled_miss_prob_against_target() {
        let s = spec();
        assert!(modeled_meets_target(&s, 0.0));
        assert!(modeled_meets_target(&s, 0.001));
        assert!(!modeled_meets_target(&s, 0.01));
        assert!(!modeled_meets_target(&s, f64::NAN));
    }
}
