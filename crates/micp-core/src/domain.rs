//! Production domain model: candidates, workloads, retrieval, batching, SLOs.

use serde::{Deserialize, Serialize};

use crate::units::{Milliseconds, Probability, Quality, Rps, Tokens, Usd};
use crate::{ModelId, ModelProfile, TrafficClass};

/// How a route should retrieve supporting context. `None` means a
/// closed-book / no-RAG path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalStrategy {
    None,
    Sparse,
    Dense,
    Hybrid,
}

impl RetrievalStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sparse => "sparse",
            Self::Dense => "dense",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankStrategy {
    None,
    CrossEncoder,
    Llm,
}

impl RerankStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CrossEncoder => "cross_encoder",
            Self::Llm => "llm",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub strategy: RetrievalStrategy,
    pub top_k: u32,
    pub rerank: RerankStrategy,
    pub context_budget: Tokens,
}

impl RetrievalConfig {
    pub fn none() -> Self {
        Self {
            strategy: RetrievalStrategy::None,
            top_k: 0,
            rerank: RerankStrategy::None,
            context_budget: Tokens::new(0.0),
        }
    }

    pub fn effective_k(&self) -> u32 {
        if self.strategy == RetrievalStrategy::None {
            0
        } else {
            self.top_k
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchingPolicy {
    None,
    Window {
        max_batch: u32,
        max_wait_ms: Milliseconds,
    },
}

impl BatchingPolicy {
    pub fn tag(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Window { .. } => "window",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TokenDistribution {
    pub mean: Tokens,
    pub p95: Tokens,
    pub p99: Tokens,
}

impl TokenDistribution {
    pub fn fixed(n: f64) -> Self {
        let t = Tokens::new(n);
        Self {
            mean: t,
            p95: t,
            p99: t,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BurstSpec {
    /// Peak arrival rate = mean_rps * peak_multiplier.
    pub peak_multiplier: f64,
    pub period_s: f64,
    /// Fraction of each period spent at peak. In `(0, 1]`.
    pub duty_cycle: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrafficProfile {
    pub class: TrafficClass,
    pub mean_rps: Rps,
    /// Offered client parallelism. Servers used = min(max_concurrency, this).
    pub concurrency: u32,
    pub burst: Option<BurstSpec>,
}

impl TrafficProfile {
    /// Arrival rate the *estimate* sizes to. Burst scenarios size to peak
    /// (conservative). Steady traffic uses the mean.
    pub fn design_rps(&self) -> f64 {
        let base = self.mean_rps.get().max(0.0);
        match self.burst {
            Some(b) if b.peak_multiplier.is_finite() && b.peak_multiplier > 1.0 => {
                base * b.peak_multiplier
            }
            _ => base,
        }
    }
}

/// What to do when modelled occupancy exceeds replica capacity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExhaustionPolicy {
    Reject,
    Queue { max_wait_ms: Milliseconds },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SloConstraints {
    pub latency_slo: Milliseconds,
    pub quality_floor: Quality,
    pub cost_ceiling_per_request: Usd,
    pub reliability_target: Probability,
    pub min_capacity: Rps,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkloadProfile {
    pub id: String,
    pub traffic: TrafficProfile,
    pub prompt_tokens: TokenDistribution,
    pub context_tokens: TokenDistribution,
    pub expected_output_tokens: TokenDistribution,
    pub retrieval: RetrievalConfig,
    pub batching: BatchingPolicy,
    pub constraints: SloConstraints,
    pub exhaustion: ExhaustionPolicy,
}

/// Linear-ish latency curve. See `docs/models.md`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LatencyCurve {
    pub intercept_ms: Milliseconds,
    pub ms_per_input_token: f64,
    pub ms_per_output_token: f64,
    pub retrieval_ms_per_k: Milliseconds,
    pub rerank_ms: Milliseconds,
    pub sigma_ms: Milliseconds,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CostModel {
    pub usd_per_1k_input: Usd,
    pub usd_per_1k_output: Usd,
    pub usd_per_retrieval: Usd,
    pub usd_per_rerank: Usd,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QualityModel {
    pub base: Quality,
    pub retrieval_gain_per_k: f64,
    pub rerank_gain: f64,
    pub context_saturation: Tokens,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReliabilityModel {
    pub base_error_rate: Probability,
    /// Fraction of SLO misses treated as failed requests (client timeout).
    pub timeout_as_failure: f64,
    pub saturation_error_slope: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapacityModel {
    pub max_concurrency: u32,
    pub max_tokens_per_sec: f64,
    /// 1.0 = healthy. Scales usable concurrency (failed replicas), not
    /// per-token speed.
    pub degraded_factor: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InferenceCandidate {
    pub id: ModelId,
    pub latency: LatencyCurve,
    pub quality: QualityModel,
    pub cost: CostModel,
    pub reliability: ReliabilityModel,
    pub capacity: CapacityModel,
    pub retrieval_capable: bool,
    /// If this candidate fails SLO/capacity, try this smaller/faster id.
    pub fallback_to: Option<ModelId>,
}

impl InferenceCandidate {
    /// Expand a compact inventory row into a serving model.
    ///
    /// Calibration (documented, not empirical):
    /// - Little's law: `max_concurrency ≈ capacity_rps * p99_s`
    /// - 40% of inventory p99 is intercept, 30% input, 30% output at a
    ///   default 1024-in / 256-out operating point
    /// - σ = 15% of inventory p99
    /// - unit cost split 40/60 input/output
    pub fn from_inventory(p: &ModelProfile) -> Self {
        let p99 = p.latency_p99_ms.max(1.0);
        let conc = ((p.capacity_rps.max(0.0) * (p99 / 1000.0)).round() as u32).max(1);
        Self {
            id: p.id.clone(),
            latency: LatencyCurve {
                intercept_ms: Milliseconds::new(p99 * 0.40),
                ms_per_input_token: p99 * 0.30 / 1024.0,
                ms_per_output_token: p99 * 0.30 / 256.0,
                retrieval_ms_per_k: Milliseconds::new(2.0),
                rerank_ms: Milliseconds::new(25.0),
                sigma_ms: Milliseconds::new(p99 * 0.15),
            },
            quality: QualityModel {
                base: Quality::clamp(p.quality),
                retrieval_gain_per_k: 0.02,
                rerank_gain: 0.04,
                context_saturation: Tokens::new(2048.0),
            },
            cost: CostModel {
                usd_per_1k_input: Usd::new(p.cost_per_1k_tokens * 0.40),
                usd_per_1k_output: Usd::new(p.cost_per_1k_tokens * 0.60),
                usd_per_retrieval: Usd::new(0.0002),
                usd_per_rerank: Usd::new(0.0004),
            },
            reliability: ReliabilityModel {
                base_error_rate: Probability::clamp(p.error_rate),
                timeout_as_failure: 0.5,
                saturation_error_slope: 0.2,
            },
            capacity: CapacityModel {
                max_concurrency: conc,
                max_tokens_per_sec: p.capacity_rps.max(0.0) * (1024.0 + 256.0),
                degraded_factor: 1.0,
            },
            retrieval_capable: true,
            fallback_to: None,
        }
    }

    pub fn usable_concurrency(&self) -> u32 {
        let f = self.capacity.degraded_factor;
        if !f.is_finite() || f <= 0.0 {
            return 0;
        }
        ((self.capacity.max_concurrency as f64) * f).floor() as u32
    }
}

/// A concrete operating point: one model + retrieval/batching/context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteConfig {
    pub candidate: InferenceCandidate,
    pub retrieval: RetrievalConfig,
    pub batching: BatchingPolicy,
    pub context_budget: Tokens,
}

impl RouteConfig {
    pub fn baseline(candidate: InferenceCandidate, workload: &WorkloadProfile) -> Self {
        let mut retrieval = workload.retrieval.clone();
        if !candidate.retrieval_capable {
            retrieval = RetrievalConfig::none();
        }
        let context_budget = if retrieval.strategy == RetrievalStrategy::None {
            Tokens::new(0.0)
        } else {
            retrieval.context_budget
        };
        Self {
            candidate,
            retrieval,
            batching: workload.batching,
            context_budget,
        }
    }

    /// Stable, comparable identity used for tie-breaking and de-dup.
    pub fn key(&self) -> String {
        format!(
            "{}:k{}:{}:c{:.0}:{}",
            self.candidate.id.as_str(),
            self.retrieval.effective_k(),
            self.retrieval.rerank.as_str(),
            self.context_budget.get(),
            self.batching.tag()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateOrigin {
    /// Closed-form system model. Not a measurement.
    Modeled,
    /// Seeded discrete-event simulation. Not a production trace.
    Simulated,
    /// Reserved for telemetry-backed estimates. Unused in this phase.
    Empirical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteEstimate {
    pub origin: EstimateOrigin,
    pub route_key: String,
    pub model_id: ModelId,
    pub p50_ms: Milliseconds,
    pub p95_ms: Milliseconds,
    pub p99_ms: Milliseconds,
    pub throughput_rps: Rps,
    pub utilization: f64,
    pub saturated: bool,
    pub quality: Quality,
    pub cost_per_request: Usd,
    pub cost_per_1k: Usd,
    pub slo_violation_prob: Probability,
    pub failure_prob: Probability,
    pub fallback_activation_prob: Probability,
}

impl RouteEstimate {
    /// Weighted score used to pick among a Pareto set. Higher is better.
    pub fn weighted_score(&self, weights: &crate::ObjectiveWeights) -> f64 {
        let w = weights.normalized();
        let latency = 1.0 / (1.0 + self.p99_ms.get() / 100.0);
        let cost = 1.0 / (1.0 + self.cost_per_request.get() * 1000.0);
        let throughput = 1.0 - self.utilization.clamp(0.0, 1.0);
        let reliability = 1.0 - self.failure_prob.get().clamp(0.0, 1.0);
        w.latency * latency
            + w.quality * self.quality.get().clamp(0.0, 1.0)
            + w.cost * cost
            + w.throughput * throughput
            + w.reliability * reliability
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    Latency,
    Quality,
    Cost,
    Reliability,
    Capacity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstraintViolation {
    pub kind: ConstraintKind,
    pub message: String,
    pub observed: f64,
    pub limit: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationAction {
    DisableRerank,
    ReduceRetrieval,
    ReduceContext,
    AlterBatching,
    FallbackModel,
    Queue,
    Reject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_rps_uses_peak_when_bursty() {
        let t = TrafficProfile {
            class: TrafficClass::Interactive,
            mean_rps: Rps::new(10.0),
            concurrency: 8,
            burst: Some(BurstSpec {
                peak_multiplier: 5.0,
                period_s: 30.0,
                duty_cycle: 0.2,
            }),
        };
        assert!((t.design_rps() - 50.0).abs() < 1e-12);
    }

    #[test]
    fn from_inventory_sets_positive_concurrency() {
        let p = ModelProfile {
            id: "m".into(),
            latency_p99_ms: 80.0,
            quality: 0.7,
            cost_per_1k_tokens: 0.04,
            capacity_rps: 120.0,
            error_rate: 0.01,
        };
        let c = InferenceCandidate::from_inventory(&p);
        assert!(c.capacity.max_concurrency >= 1);
        assert_eq!(c.usable_concurrency(), c.capacity.max_concurrency);
        let mut d = c.clone();
        d.capacity.degraded_factor = 0.0;
        assert_eq!(d.usable_concurrency(), 0);
    }
}
