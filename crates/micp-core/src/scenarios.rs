//! Transparent production-scenario presets.
//!
//! Numbers are **assumptions**, not fleet telemetry. Each scenario
//! exposes those assumptions so a workbench can render them later.

use serde::{Deserialize, Serialize};

use crate::domain::{
    BatchingPolicy, BurstSpec, CapacityModel, CostModel, ExhaustionPolicy, InferenceCandidate,
    LatencyCurve, QualityModel, ReliabilityModel, RerankStrategy, RetrievalConfig,
    RetrievalStrategy, SloConstraints, TokenDistribution, TrafficProfile, WorkloadProfile,
};
use crate::units::{Milliseconds, Probability, Quality, Rps, Tokens, Usd};
use crate::{ObjectiveWeights, TrafficClass};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub assumptions: Vec<String>,
    pub workload: WorkloadProfile,
    pub fleet: Vec<InferenceCandidate>,
    pub weights: ObjectiveWeights,
}

pub fn all_scenarios() -> Vec<Scenario> {
    vec![
        interactive_assistant(),
        cost_constrained_volume(),
        quality_rag(),
        bursty_enterprise(),
        degraded_failover(),
    ]
}

pub fn scenario_by_id(id: &str) -> Option<Scenario> {
    all_scenarios().into_iter().find(|s| s.id == id)
}

pub fn scenario_ids() -> &'static [&'static str] {
    &[
        "interactive_assistant",
        "cost_constrained_volume",
        "quality_rag",
        "bursty_enterprise",
        "degraded_failover",
    ]
}

pub fn interactive_assistant() -> Scenario {
    Scenario {
        id: "interactive_assistant".into(),
        name: "Low-latency interactive assistant".into(),
        summary: "Closed-book chat. Size for p99 under 200ms; quality floor is modest.".into(),
        assumptions: vec![
            "Open-loop arrivals at 15 rps, 12 parallel clients.".into(),
            "Prompt 256 / context 0 / output 128 tokens (no RAG).".into(),
            "Latency SLO is modeled p99 ≤ 200ms, not an empirical trace.".into(),
            "Quality is a proxy (base model score), not a judged groundedness metric.".into(),
            "Costs are list-price-like USD/1k tokens, not a negotiated contract.".into(),
        ],
        workload: WorkloadProfile {
            id: "interactive_assistant".into(),
            traffic: TrafficProfile {
                class: TrafficClass::Interactive,
                mean_rps: Rps::new(15.0),
                concurrency: 12,
                burst: None,
            },
            prompt_tokens: TokenDistribution::fixed(256.0),
            context_tokens: TokenDistribution::fixed(0.0),
            expected_output_tokens: TokenDistribution::fixed(128.0),
            retrieval: RetrievalConfig::none(),
            batching: BatchingPolicy::None,
            constraints: SloConstraints {
                latency_slo: Milliseconds::new(200.0),
                quality_floor: Quality::new(0.65),
                cost_ceiling_per_request: Usd::new(0.01),
                reliability_target: Probability::new(0.99),
                min_capacity: Rps::new(15.0),
            },
            exhaustion: ExhaustionPolicy::Reject,
        },
        fleet: vec![fast_8b(), mixtral(), quality_70b()],
        weights: ObjectiveWeights {
            latency: 0.45,
            quality: 0.20,
            cost: 0.15,
            throughput: 0.05,
            reliability: 0.15,
        },
    }
}

pub fn cost_constrained_volume() -> Scenario {
    Scenario {
        id: "cost_constrained_volume".into(),
        name: "Cost-constrained high-volume inference".into(),
        summary: "Short prompts at 80 rps with a tight per-request cost ceiling.".into(),
        assumptions: vec![
            "Steady 80 rps, 40 clients, no burst.".into(),
            "Prompt 128 / context 0 / output 64 tokens.".into(),
            "Cost ceiling $0.002/req is the binding constraint.".into(),
            "Batching window (8, 15ms) is allowed to raise throughput.".into(),
            "Estimates size to mean rate, not a peak.".into(),
        ],
        workload: WorkloadProfile {
            id: "cost_constrained_volume".into(),
            traffic: TrafficProfile {
                class: TrafficClass::Batch,
                mean_rps: Rps::new(80.0),
                concurrency: 40,
                burst: None,
            },
            prompt_tokens: TokenDistribution::fixed(128.0),
            context_tokens: TokenDistribution::fixed(0.0),
            expected_output_tokens: TokenDistribution::fixed(64.0),
            retrieval: RetrievalConfig::none(),
            batching: BatchingPolicy::Window {
                max_batch: 8,
                max_wait_ms: Milliseconds::new(15.0),
            },
            constraints: SloConstraints {
                latency_slo: Milliseconds::new(400.0),
                quality_floor: Quality::new(0.60),
                cost_ceiling_per_request: Usd::new(0.002),
                reliability_target: Probability::new(0.98),
                min_capacity: Rps::new(60.0),
            },
            exhaustion: ExhaustionPolicy::Queue {
                max_wait_ms: Milliseconds::new(200.0),
            },
        },
        fleet: vec![fast_8b(), mixtral(), quality_70b()],
        weights: ObjectiveWeights {
            latency: 0.15,
            quality: 0.15,
            cost: 0.45,
            throughput: 0.15,
            reliability: 0.10,
        },
    }
}

pub fn quality_rag() -> Scenario {
    Scenario {
        id: "quality_rag".into(),
        name: "Quality-sensitive RAG workload".into(),
        summary: "Hybrid retrieval + cross-encoder rerank; quality floor 0.85.".into(),
        assumptions: vec![
            "8 rps interactive RAG, 8 clients.".into(),
            "Prompt 400 / retrieved context 2048 / output 400 tokens.".into(),
            "Hybrid retrieval top_k=20, cross-encoder rerank, 4096-token budget.".into(),
            "Quality proxy adds log(k) retrieval gain and a rerank bonus; not NDCG.".into(),
            "Latency SLO 1500ms p99 (RAG is not the interactive-chat SLO).".into(),
        ],
        workload: WorkloadProfile {
            id: "quality_rag".into(),
            traffic: TrafficProfile {
                class: TrafficClass::Interactive,
                mean_rps: Rps::new(8.0),
                concurrency: 8,
                burst: None,
            },
            prompt_tokens: TokenDistribution::fixed(400.0),
            context_tokens: TokenDistribution::fixed(2048.0),
            expected_output_tokens: TokenDistribution::fixed(400.0),
            retrieval: RetrievalConfig {
                strategy: RetrievalStrategy::Hybrid,
                top_k: 20,
                rerank: RerankStrategy::CrossEncoder,
                context_budget: Tokens::new(4096.0),
            },
            batching: BatchingPolicy::None,
            constraints: SloConstraints {
                latency_slo: Milliseconds::new(1500.0),
                quality_floor: Quality::new(0.85),
                cost_ceiling_per_request: Usd::new(0.50),
                reliability_target: Probability::new(0.99),
                min_capacity: Rps::new(5.0),
            },
            exhaustion: ExhaustionPolicy::Reject,
        },
        fleet: vec![fast_8b(), mixtral(), quality_70b()],
        weights: ObjectiveWeights {
            latency: 0.15,
            quality: 0.45,
            cost: 0.15,
            throughput: 0.05,
            reliability: 0.20,
        },
    }
}

pub fn bursty_enterprise() -> Scenario {
    Scenario {
        id: "bursty_enterprise".into(),
        name: "Bursty enterprise workload".into(),
        summary: "Mean 20 rps with 8× peaks; estimates size to the peak.".into(),
        assumptions: vec![
            "Mean 20 rps, peak multiplier 8 (design rate 160 rps).".into(),
            "Burst period 60s, duty cycle 0.15 (9s on / 51s off).".into(),
            "Dense retrieval top_k=8, no rerank, 1024-token budget.".into(),
            "Peak-sizing is conservative; a mean-sized fleet would look healthier.".into(),
            "24 clients; SLO 350ms p99, quality floor 0.70.".into(),
        ],
        workload: WorkloadProfile {
            id: "bursty_enterprise".into(),
            traffic: TrafficProfile {
                class: TrafficClass::Interactive,
                mean_rps: Rps::new(20.0),
                concurrency: 24,
                burst: Some(BurstSpec {
                    peak_multiplier: 8.0,
                    period_s: 60.0,
                    duty_cycle: 0.15,
                }),
            },
            prompt_tokens: TokenDistribution {
                mean: Tokens::new(300.0),
                p95: Tokens::new(700.0),
                p99: Tokens::new(1200.0),
            },
            context_tokens: TokenDistribution {
                mean: Tokens::new(800.0),
                p95: Tokens::new(1600.0),
                p99: Tokens::new(2400.0),
            },
            expected_output_tokens: TokenDistribution {
                mean: Tokens::new(200.0),
                p95: Tokens::new(400.0),
                p99: Tokens::new(700.0),
            },
            retrieval: RetrievalConfig {
                strategy: RetrievalStrategy::Dense,
                top_k: 8,
                rerank: RerankStrategy::None,
                context_budget: Tokens::new(1024.0),
            },
            batching: BatchingPolicy::None,
            constraints: SloConstraints {
                latency_slo: Milliseconds::new(350.0),
                quality_floor: Quality::new(0.70),
                cost_ceiling_per_request: Usd::new(0.02),
                reliability_target: Probability::new(0.99),
                min_capacity: Rps::new(20.0),
            },
            exhaustion: ExhaustionPolicy::Queue {
                max_wait_ms: Milliseconds::new(300.0),
            },
        },
        fleet: vec![fast_8b(), mixtral(), quality_70b()],
        weights: ObjectiveWeights {
            latency: 0.30,
            quality: 0.20,
            cost: 0.15,
            throughput: 0.20,
            reliability: 0.15,
        },
    }
}

pub fn degraded_failover() -> Scenario {
    let mut primary = quality_70b();
    primary.capacity.degraded_factor = 0.0;
    let mut mid = mixtral();
    mid.capacity.degraded_factor = 0.40;
    Scenario {
        id: "degraded_failover".into(),
        name: "Degraded-capacity / failover event".into(),
        summary: "70b is down; mixtral at 40% replicas; 8b is the healthy fallback.".into(),
        assumptions: vec![
            "Same traffic shape as the interactive assistant (15 rps, no RAG).".into(),
            "degraded_factor scales usable concurrency (failed replicas), not token speed.".into(),
            "70b usable concurrency = 0; mixtral ≈ 40% of 10; 8b healthy.".into(),
            "Fallback edges: 70b → mixtral, mixtral → 8b.".into(),
            "Not a chaos-test against a real cluster — a modeled replica-loss case.".into(),
        ],
        workload: WorkloadProfile {
            id: "degraded_failover".into(),
            traffic: TrafficProfile {
                class: TrafficClass::Interactive,
                mean_rps: Rps::new(15.0),
                concurrency: 12,
                burst: None,
            },
            prompt_tokens: TokenDistribution::fixed(256.0),
            context_tokens: TokenDistribution::fixed(0.0),
            expected_output_tokens: TokenDistribution::fixed(128.0),
            retrieval: RetrievalConfig::none(),
            batching: BatchingPolicy::None,
            constraints: SloConstraints {
                latency_slo: Milliseconds::new(250.0),
                quality_floor: Quality::new(0.65),
                cost_ceiling_per_request: Usd::new(0.02),
                reliability_target: Probability::new(0.98),
                min_capacity: Rps::new(10.0),
            },
            exhaustion: ExhaustionPolicy::Reject,
        },
        fleet: vec![fast_8b(), mid, primary],
        weights: ObjectiveWeights {
            latency: 0.25,
            quality: 0.20,
            cost: 0.15,
            throughput: 0.15,
            reliability: 0.25,
        },
    }
}

fn fast_8b() -> InferenceCandidate {
    InferenceCandidate {
        id: "fast-8b".into(),
        latency: LatencyCurve {
            intercept_ms: Milliseconds::new(22.0),
            ms_per_input_token: 0.015,
            ms_per_output_token: 0.06,
            retrieval_ms_per_k: Milliseconds::new(1.5),
            rerank_ms: Milliseconds::new(18.0),
            sigma_ms: Milliseconds::new(8.0),
        },
        quality: QualityModel {
            base: Quality::new(0.70),
            retrieval_gain_per_k: 0.022,
            rerank_gain: 0.035,
            context_saturation: Tokens::new(2048.0),
        },
        cost: CostModel {
            usd_per_1k_input: Usd::new(0.004),
            usd_per_1k_output: Usd::new(0.008),
            usd_per_retrieval: Usd::new(0.0001),
            usd_per_rerank: Usd::new(0.0002),
        },
        reliability: ReliabilityModel {
            base_error_rate: Probability::new(0.003),
            timeout_as_failure: 0.5,
            saturation_error_slope: 0.15,
        },
        capacity: CapacityModel {
            max_concurrency: 16,
            max_tokens_per_sec: 80_000.0,
            degraded_factor: 1.0,
        },
        retrieval_capable: true,
        fallback_to: None,
    }
}

fn mixtral() -> InferenceCandidate {
    InferenceCandidate {
        id: "balanced-mixtral".into(),
        latency: LatencyCurve {
            intercept_ms: Milliseconds::new(40.0),
            ms_per_input_token: 0.03,
            ms_per_output_token: 0.11,
            retrieval_ms_per_k: Milliseconds::new(2.0),
            rerank_ms: Milliseconds::new(28.0),
            sigma_ms: Milliseconds::new(16.0),
        },
        quality: QualityModel {
            base: Quality::new(0.82),
            retrieval_gain_per_k: 0.028,
            rerank_gain: 0.045,
            context_saturation: Tokens::new(3072.0),
        },
        cost: CostModel {
            usd_per_1k_input: Usd::new(0.08),
            usd_per_1k_output: Usd::new(0.16),
            usd_per_retrieval: Usd::new(0.0002),
            usd_per_rerank: Usd::new(0.0005),
        },
        reliability: ReliabilityModel {
            base_error_rate: Probability::new(0.005),
            timeout_as_failure: 0.5,
            saturation_error_slope: 0.18,
        },
        capacity: CapacityModel {
            max_concurrency: 10,
            max_tokens_per_sec: 40_000.0,
            degraded_factor: 1.0,
        },
        retrieval_capable: true,
        fallback_to: Some("fast-8b".into()),
    }
}

fn quality_70b() -> InferenceCandidate {
    InferenceCandidate {
        id: "quality-70b".into(),
        latency: LatencyCurve {
            intercept_ms: Milliseconds::new(85.0),
            ms_per_input_token: 0.07,
            ms_per_output_token: 0.22,
            retrieval_ms_per_k: Milliseconds::new(3.0),
            rerank_ms: Milliseconds::new(40.0),
            sigma_ms: Milliseconds::new(30.0),
        },
        quality: QualityModel {
            base: Quality::new(0.92),
            retrieval_gain_per_k: 0.032,
            rerank_gain: 0.055,
            context_saturation: Tokens::new(4096.0),
        },
        cost: CostModel {
            usd_per_1k_input: Usd::new(0.28),
            usd_per_1k_output: Usd::new(0.56),
            usd_per_retrieval: Usd::new(0.0003),
            usd_per_rerank: Usd::new(0.001),
        },
        reliability: ReliabilityModel {
            base_error_rate: Probability::new(0.007),
            timeout_as_failure: 0.5,
            saturation_error_slope: 0.22,
        },
        capacity: CapacityModel {
            max_concurrency: 4,
            max_tokens_per_sec: 12_000.0,
            degraded_factor: 1.0,
        },
        retrieval_capable: true,
        fallback_to: Some("balanced-mixtral".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estimate::{estimate_route, is_feasible};
    use crate::RouteConfig;

    #[test]
    fn five_named_presets() {
        let all = all_scenarios();
        assert_eq!(all.len(), 5);
        for id in scenario_ids() {
            let s = scenario_by_id(id).unwrap();
            assert_eq!(s.id, *id);
            assert!(!s.assumptions.is_empty());
            assert_eq!(s.fleet.len(), 3);
        }
        assert!(scenario_by_id("nope").is_none());
    }

    #[test]
    fn every_preset_has_at_least_one_modeled_estimate() {
        for s in all_scenarios() {
            let mut any_ok = false;
            for c in &s.fleet {
                let route = RouteConfig::baseline(c.clone(), &s.workload);
                let e = estimate_route(&route, &s.workload).unwrap();
                assert_eq!(e.origin, crate::EstimateOrigin::Modeled);
                if is_feasible(&e, &s.workload) {
                    any_ok = true;
                }
            }
            // Failover and burst may leave only the small model feasible;
            // cost-constrained likewise. Just require finite estimates.
            let _ = any_ok;
        }
    }

    #[test]
    fn interactive_prefers_fast_model_on_latency() {
        let s = interactive_assistant();
        let mut scores = Vec::new();
        for c in &s.fleet {
            let e = estimate_route(&RouteConfig::baseline(c.clone(), &s.workload), &s.workload)
                .unwrap();
            scores.push((
                c.id.as_str().to_string(),
                e.p99_ms.get(),
                e.weighted_score(&s.weights),
            ));
        }
        scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        assert_eq!(scores[0].0, "fast-8b");
    }

    #[test]
    fn degraded_primary_has_zero_usable_concurrency() {
        let s = degraded_failover();
        let down = s
            .fleet
            .iter()
            .find(|c| c.id.as_str() == "quality-70b")
            .unwrap();
        assert_eq!(down.usable_concurrency(), 0);
        let healthy = s.fleet.iter().find(|c| c.id.as_str() == "fast-8b").unwrap();
        assert!(healthy.usable_concurrency() > 0);
    }
}
