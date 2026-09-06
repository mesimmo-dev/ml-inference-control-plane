//! Inventory snapshot types used by the existing config/API.
//!
//! These are *calibrated summaries* (a p99, a quality number, a unit
//! cost) rather than the full serving model. Prefer [`crate::InferenceCandidate`]
//! for new engine paths. [`ModelProfile::score`] is kept byte-stable so
//! the TypeScript scaffolding and WASM ABI continue to match.

use serde::{Deserialize, Serialize};

use crate::{MicpError, Result};

/// Newtype model identifier. Kept as a string so fleet inventories can
/// use the names operators already have (model registry IDs, endpoints).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(pub String);

impl ModelId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ModelId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ModelId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficClass {
    Interactive,
    Batch,
    Offline,
}

/// Observed / configured serving profile for one candidate in the fleet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: ModelId,
    pub latency_p99_ms: f64,
    pub quality: f64,
    pub cost_per_1k_tokens: f64,
    pub capacity_rps: f64,
    pub error_rate: f64,
}

/// Hard constraints attached to a request (or to a traffic class via policy).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RequestConstraints {
    pub traffic_class: TrafficClass,
    pub max_latency_ms: f64,
    pub min_quality: f64,
    pub max_cost_per_1k: f64,
    pub require_error_rate_below: f64,
}

impl RequestConstraints {
    pub fn interactive_default() -> Self {
        Self {
            traffic_class: TrafficClass::Interactive,
            max_latency_ms: 250.0,
            min_quality: 0.70,
            max_cost_per_1k: 0.50,
            require_error_rate_below: 0.02,
        }
    }
}

/// Relative importance of the five competing objectives.
///
/// Values need not sum to 1; [`ObjectiveWeights::normalized`] projects
/// them onto the unit simplex. All-zero weights fall back to balanced.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveWeights {
    pub latency: f64,
    pub quality: f64,
    pub cost: f64,
    pub throughput: f64,
    pub reliability: f64,
}

impl ObjectiveWeights {
    pub fn balanced() -> Self {
        Self {
            latency: 0.2,
            quality: 0.2,
            cost: 0.2,
            throughput: 0.2,
            reliability: 0.2,
        }
    }

    pub fn sum(self) -> f64 {
        self.latency + self.quality + self.cost + self.throughput + self.reliability
    }

    pub fn normalized(self) -> Self {
        let s = self.sum();
        if s <= 0.0 {
            return Self::balanced();
        }
        Self {
            latency: self.latency / s,
            quality: self.quality / s,
            cost: self.cost / s,
            throughput: self.throughput / s,
            reliability: self.reliability / s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub model_id: ModelId,
    pub score: f64,
    pub feasible: bool,
    pub rejected: Vec<CandidateRejection>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateRejection {
    pub model_id: ModelId,
    pub reasons: Vec<String>,
}

impl ModelProfile {
    /// Reasons this candidate is infeasible under `constraints`. Empty
    /// means the candidate is eligible to be scored.
    pub fn violations(&self, constraints: &RequestConstraints) -> Vec<String> {
        let mut reasons = Vec::new();
        if self.latency_p99_ms > constraints.max_latency_ms {
            reasons.push(format!(
                "latency p99 {:.1}ms exceeds max {:.1}ms",
                self.latency_p99_ms, constraints.max_latency_ms
            ));
        }
        if self.quality < constraints.min_quality {
            reasons.push(format!(
                "quality {:.3} below min {:.3}",
                self.quality, constraints.min_quality
            ));
        }
        if self.cost_per_1k_tokens > constraints.max_cost_per_1k {
            reasons.push(format!(
                "cost {:.3}/1k exceeds max {:.3}",
                self.cost_per_1k_tokens, constraints.max_cost_per_1k
            ));
        }
        if self.error_rate > constraints.require_error_rate_below {
            reasons.push(format!(
                "error rate {:.4} exceeds max {:.4}",
                self.error_rate, constraints.require_error_rate_below
            ));
        }
        if self.capacity_rps <= 0.0 {
            reasons.push("capacity_rps must be positive".into());
        }
        reasons
    }

    pub fn is_feasible(&self, constraints: &RequestConstraints) -> bool {
        self.violations(constraints).is_empty()
    }

    /// Higher is better. Inputs are assumed already filtered.
    pub fn score(&self, weights: &ObjectiveWeights, demand_rps: f64) -> f64 {
        let w = weights.normalized();
        let demand = demand_rps.max(0.0);
        let latency = 1.0 / (1.0 + self.latency_p99_ms / 100.0);
        let cost = 1.0 / (1.0 + self.cost_per_1k_tokens);
        let throughput = self.capacity_rps / (self.capacity_rps + demand);
        let reliability = 1.0 - self.error_rate.clamp(0.0, 1.0);
        let quality = self.quality.clamp(0.0, 1.0);
        w.latency * latency
            + w.quality * quality
            + w.cost * cost
            + w.throughput * throughput
            + w.reliability * reliability
    }
}

/// Split a fleet into (feasible, rejections) under `constraints`.
pub fn partition_fleet<'a>(
    fleet: &'a [ModelProfile],
    constraints: &RequestConstraints,
) -> (Vec<&'a ModelProfile>, Vec<CandidateRejection>) {
    let mut feasible = Vec::new();
    let mut rejected = Vec::new();
    for model in fleet {
        let reasons = model.violations(constraints);
        if reasons.is_empty() {
            feasible.push(model);
        } else {
            rejected.push(CandidateRejection {
                model_id: model.id.clone(),
                reasons,
            });
        }
    }
    (feasible, rejected)
}

pub fn require_finite_nonneg(name: &str, v: f64) -> Result<()> {
    if !v.is_finite() || v < 0.0 {
        Err(MicpError::InvalidConfig(format!(
            "{name} must be a finite non-negative number"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast() -> ModelProfile {
        ModelProfile {
            id: "fast".into(),
            latency_p99_ms: 80.0,
            quality: 0.72,
            cost_per_1k_tokens: 0.04,
            capacity_rps: 120.0,
            error_rate: 0.004,
        }
    }

    fn slow_quality() -> ModelProfile {
        ModelProfile {
            id: "quality".into(),
            latency_p99_ms: 400.0,
            quality: 0.95,
            cost_per_1k_tokens: 0.40,
            capacity_rps: 10.0,
            error_rate: 0.01,
        }
    }

    #[test]
    fn interactive_default_accepts_fast_rejects_slow() {
        let c = RequestConstraints::interactive_default();
        assert!(fast().is_feasible(&c));
        assert!(!slow_quality().is_feasible(&c));
        assert!(slow_quality()
            .violations(&c)
            .iter()
            .any(|r| r.contains("latency")));
    }

    #[test]
    fn balanced_weights_prefer_fast_under_interactive_load() {
        let w = ObjectiveWeights::balanced();
        let demand = 20.0;
        assert!(fast().score(&w, demand) > slow_quality().score(&w, demand));
    }

    #[test]
    fn quality_heavy_weights_can_prefer_slower_model() {
        let w = ObjectiveWeights {
            latency: 0.05,
            quality: 0.80,
            cost: 0.05,
            throughput: 0.05,
            reliability: 0.05,
        };
        let demand = 5.0;
        assert!(slow_quality().score(&w, demand) > fast().score(&w, demand));
    }

    #[test]
    fn zero_weights_normalize_to_balanced() {
        let z = ObjectiveWeights {
            latency: 0.0,
            quality: 0.0,
            cost: 0.0,
            throughput: 0.0,
            reliability: 0.0,
        }
        .normalized();
        assert!((z.sum() - 1.0).abs() < 1e-12);
        assert!((z.latency - 0.2).abs() < 1e-12);
    }

    #[test]
    fn partition_reports_rejections() {
        let fleet = vec![fast(), slow_quality()];
        let (ok, no) = partition_fleet(&fleet, &RequestConstraints::interactive_default());
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].id.as_str(), "fast");
        assert_eq!(no.len(), 1);
        assert_eq!(no[0].model_id.as_str(), "quality");
    }
}
