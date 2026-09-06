//! Single-request routing: policy → feasible set → highest score.
//!
//! Tie-break is lexicographic on model id so equal scores are stable
//! across processes (important for replay and tests).

mod plan;

pub use plan::{plan, plan_lenient, EvaluatedRoute, RoutePlan};

use micp_core::{
    partition_fleet, MicpError, ModelProfile, ObjectiveWeights, RequestConstraints, Result,
    RoutingDecision,
};
use micp_policy::PolicySet;

pub fn route(
    fleet: &[ModelProfile],
    constraints: &RequestConstraints,
    weights: &ObjectiveWeights,
    demand_rps: f64,
) -> Result<RoutingDecision> {
    route_with_policy(fleet, constraints, weights, demand_rps, None)
}

pub fn route_with_policy(
    fleet: &[ModelProfile],
    constraints: &RequestConstraints,
    weights: &ObjectiveWeights,
    demand_rps: f64,
    policies: Option<&PolicySet>,
) -> Result<RoutingDecision> {
    let (candidates, rejected) = if let Some(set) = policies {
        let (filtered, effective) = set.filter_fleet(fleet, constraints.clone());
        let rejected: Vec<_> = fleet
            .iter()
            .filter(|m| !filtered.iter().any(|f| f.id == m.id))
            .map(|m| {
                let mut reasons = m.violations(&effective.constraints);
                if !effective.admits(&m.id) {
                    reasons.push("denied by policy".into());
                }
                micp_core::CandidateRejection {
                    model_id: m.id.clone(),
                    reasons,
                }
            })
            .collect();
        (filtered.into_iter().cloned().collect::<Vec<_>>(), rejected)
    } else {
        let (feasible, rejected) = partition_fleet(fleet, constraints);
        (feasible.into_iter().cloned().collect::<Vec<_>>(), rejected)
    };

    let winner = candidates.iter().max_by(|a, b| {
        let sa = a.score(weights, demand_rps);
        let sb = b.score(weights, demand_rps);
        sa.partial_cmp(&sb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.id.as_str().cmp(a.id.as_str()))
    });

    match winner {
        Some(model) => Ok(RoutingDecision {
            model_id: model.id.clone(),
            score: model.score(weights, demand_rps),
            feasible: true,
            rejected,
        }),
        None => Err(MicpError::NoFeasibleModel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use micp_core::{ModelId, TrafficClass};
    use micp_policy::Policy;

    fn fleet() -> Vec<ModelProfile> {
        vec![
            ModelProfile {
                id: "fast".into(),
                latency_p99_ms: 80.0,
                quality: 0.72,
                cost_per_1k_tokens: 0.04,
                capacity_rps: 120.0,
                error_rate: 0.004,
            },
            ModelProfile {
                id: "quality".into(),
                latency_p99_ms: 220.0,
                quality: 0.91,
                cost_per_1k_tokens: 0.35,
                capacity_rps: 18.0,
                error_rate: 0.008,
            },
            ModelProfile {
                id: "expensive".into(),
                latency_p99_ms: 90.0,
                quality: 0.88,
                cost_per_1k_tokens: 0.80,
                capacity_rps: 30.0,
                error_rate: 0.003,
            },
        ]
    }

    #[test]
    fn routes_to_fast_under_balanced_interactive() {
        let d = route(
            &fleet(),
            &RequestConstraints::interactive_default(),
            &ObjectiveWeights::balanced(),
            10.0,
        )
        .unwrap();
        assert_eq!(d.model_id, ModelId::from("fast"));
        assert!(d.feasible);
        assert!(d
            .rejected
            .iter()
            .any(|r| r.model_id.as_str() == "expensive"));
    }

    #[test]
    fn policy_can_force_quality_model() {
        let mut p = Policy {
            name: "only-quality".into(),
            priority: 1,
            enabled: true,
            match_class: Some(TrafficClass::Interactive),
            max_latency_ms: None,
            min_quality: None,
            max_cost_per_1k: None,
            max_error_rate: None,
            allow_models: Some(vec!["quality".into()]),
            deny_models: None,
        };
        p.max_latency_ms = Some(250.0);
        let set = PolicySet { policies: vec![p] };
        let d = route_with_policy(
            &fleet(),
            &RequestConstraints::interactive_default(),
            &ObjectiveWeights::balanced(),
            10.0,
            Some(&set),
        )
        .unwrap();
        assert_eq!(d.model_id, ModelId::from("quality"));
    }

    #[test]
    fn empty_feasible_set_is_an_error() {
        let tight = RequestConstraints {
            traffic_class: TrafficClass::Interactive,
            max_latency_ms: 10.0,
            min_quality: 0.99,
            max_cost_per_1k: 0.01,
            require_error_rate_below: 0.0001,
        };
        let err = route(&fleet(), &tight, &ObjectiveWeights::balanced(), 1.0).unwrap_err();
        assert_eq!(err, MicpError::NoFeasibleModel);
    }

    #[test]
    fn equal_scores_break_ties_by_id() {
        let a = ModelProfile {
            id: "aaa".into(),
            latency_p99_ms: 100.0,
            quality: 0.8,
            cost_per_1k_tokens: 0.1,
            capacity_rps: 50.0,
            error_rate: 0.01,
        };
        let mut b = a.clone();
        b.id = "bbb".into();
        let d = route(
            &[b, a],
            &RequestConstraints::interactive_default(),
            &ObjectiveWeights::balanced(),
            1.0,
        )
        .unwrap();
        assert_eq!(d.model_id.as_str(), "aaa");
    }
}
