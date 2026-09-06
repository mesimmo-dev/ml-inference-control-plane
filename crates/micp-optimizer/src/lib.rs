//! Traffic-share allocation under capacity.
//!
//! 1. Drop infeasible candidates.
//! 2. Softmax over scores → desired shares.
//! 3. Clip each share to `capacity_rps / demand_rps`.
//! 4. Renormalize remaining mass onto unsaturated candidates.
//!
//! If aggregate capacity cannot absorb demand, shares still sum to 1
//! and are proportional to capacity (the caller is expected to shed
//! or queue; this crate does not invent a drop policy).

use micp_core::{
    partition_fleet, MicpError, ModelId, ModelProfile, ObjectiveWeights, RequestConstraints, Result,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Share {
    pub model_id: ModelId,
    pub share: f64,
    pub score: f64,
    pub cap_rps: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Allocation {
    pub shares: Vec<Share>,
    pub demand_rps: f64,
    pub served_rps: f64,
}

impl Allocation {
    pub fn share_of(&self, id: &str) -> f64 {
        self.shares
            .iter()
            .find(|s| s.model_id.as_str() == id)
            .map(|s| s.share)
            .unwrap_or(0.0)
    }
}

pub fn allocate(
    fleet: &[ModelProfile],
    constraints: &RequestConstraints,
    weights: &ObjectiveWeights,
    demand_rps: f64,
) -> Result<Allocation> {
    let demand = demand_rps.max(0.0);
    let (feasible, _) = partition_fleet(fleet, constraints);
    if feasible.is_empty() {
        return Err(MicpError::NoFeasibleModel);
    }

    let scores: Vec<f64> = feasible.iter().map(|m| m.score(weights, demand)).collect();
    let max_s = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let unnorm: Vec<f64> = scores.iter().map(|s| (*s - max_s).exp()).collect();
    let z: f64 = unnorm.iter().sum();
    let mut desired: Vec<f64> = unnorm.iter().map(|u| u / z).collect();

    if demand > 0.0 {
        let caps: Vec<f64> = feasible
            .iter()
            .map(|m| (m.capacity_rps / demand).clamp(0.0, 1.0))
            .collect();
        // Water-filling: repeatedly clip and redistribute.
        for _ in 0..feasible.len() {
            let mut overflow = 0.0;
            let mut room = 0.0;
            for i in 0..desired.len() {
                if desired[i] > caps[i] {
                    overflow += desired[i] - caps[i];
                    desired[i] = caps[i];
                } else {
                    room += caps[i] - desired[i];
                }
            }
            if overflow <= 1e-12 || room <= 1e-12 {
                break;
            }
            for i in 0..desired.len() {
                if desired[i] + 1e-12 < caps[i] {
                    let take = overflow * ((caps[i] - desired[i]) / room);
                    desired[i] += take;
                }
            }
        }
    }

    let sum: f64 = desired.iter().sum();
    if sum > 0.0 {
        for s in desired.iter_mut() {
            *s /= sum;
        }
    }

    let served = if demand == 0.0 {
        0.0
    } else {
        feasible
            .iter()
            .zip(desired.iter())
            .map(|(m, s)| (*s * demand).min(m.capacity_rps))
            .sum()
    };

    let shares = feasible
        .iter()
        .zip(scores.iter())
        .zip(desired.iter())
        .map(|((m, score), share)| Share {
            model_id: m.id.clone(),
            share: *share,
            score: *score,
            cap_rps: m.capacity_rps,
        })
        .collect();

    Ok(Allocation {
        shares,
        demand_rps: demand,
        served_rps: served,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_models() -> Vec<ModelProfile> {
        vec![
            ModelProfile {
                id: "a".into(),
                latency_p99_ms: 80.0,
                quality: 0.8,
                cost_per_1k_tokens: 0.1,
                capacity_rps: 100.0,
                error_rate: 0.01,
            },
            ModelProfile {
                id: "b".into(),
                latency_p99_ms: 90.0,
                quality: 0.8,
                cost_per_1k_tokens: 0.1,
                capacity_rps: 10.0,
                error_rate: 0.01,
            },
        ]
    }

    fn constraints() -> RequestConstraints {
        RequestConstraints::interactive_default()
    }

    #[test]
    fn shares_sum_to_one() {
        let a = allocate(
            &two_models(),
            &constraints(),
            &ObjectiveWeights::balanced(),
            50.0,
        )
        .unwrap();
        let sum: f64 = a.shares.iter().map(|s| s.share).sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn small_model_is_capped_under_high_demand() {
        let a = allocate(
            &two_models(),
            &constraints(),
            &ObjectiveWeights::balanced(),
            100.0,
        )
        .unwrap();
        // b can take at most 10/100 = 0.10 of demand.
        assert!(a.share_of("b") <= 0.10 + 1e-9);
        assert!(a.share_of("a") >= 0.90 - 1e-9);
    }

    #[test]
    fn infeasible_fleet_errors() {
        let tight = RequestConstraints {
            traffic_class: micp_core::TrafficClass::Interactive,
            max_latency_ms: 1.0,
            min_quality: 1.0,
            max_cost_per_1k: 0.0,
            require_error_rate_below: 0.0,
        };
        assert_eq!(
            allocate(&two_models(), &tight, &ObjectiveWeights::balanced(), 10.0).unwrap_err(),
            MicpError::NoFeasibleModel
        );
    }

    #[test]
    fn better_score_gets_more_share_when_capacity_allows() {
        let a = allocate(
            &two_models(),
            &constraints(),
            &ObjectiveWeights::balanced(),
            20.0,
        )
        .unwrap();
        assert!(a.share_of("a") > a.share_of("b"));
    }
}
