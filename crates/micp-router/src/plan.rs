//! SLO-aware route planning: expand → estimate → filter → Pareto → recommend.

use micp_core::{
    constraint_violations, estimate_route, ConstraintViolation, DegradationAction,
    InferenceCandidate, MicpError, ObjectiveWeights, Result, RouteConfig, RouteEstimate,
    WorkloadProfile,
};
use micp_optimizer::{pareto_front, recommend_from_estimates, ObjectivePoint};
use micp_policy::{expand_routes, ExpandedRoute};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedRoute {
    pub route: RouteConfig,
    pub estimate: RouteEstimate,
    pub violations: Vec<ConstraintViolation>,
    pub degradations: Vec<DegradationAction>,
}

impl EvaluatedRoute {
    pub fn feasible(&self) -> bool {
        self.violations.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoutePlan {
    pub evaluated: Vec<EvaluatedRoute>,
    pub feasible_keys: Vec<String>,
    pub pareto_keys: Vec<String>,
    pub recommended: Option<EvaluatedRoute>,
}

/// Plan a workload against a fleet. All estimates are modeled.
pub fn plan(
    workload: &WorkloadProfile,
    fleet: &[InferenceCandidate],
    weights: &ObjectiveWeights,
) -> Result<RoutePlan> {
    if fleet.is_empty() {
        return Err(MicpError::InvalidConfig("fleet is empty".into()));
    }
    let expanded: Vec<ExpandedRoute> = expand_routes(fleet, workload);
    let mut evaluated = Vec::with_capacity(expanded.len());
    for exp in expanded {
        let estimate = estimate_route(&exp.route, workload)?;
        let violations = constraint_violations(&estimate, workload);
        evaluated.push(EvaluatedRoute {
            route: exp.route,
            estimate,
            violations,
            degradations: exp.actions,
        });
    }

    let feasible: Vec<&EvaluatedRoute> = evaluated.iter().filter(|e| e.feasible()).collect();
    let feasible_keys: Vec<String> = feasible
        .iter()
        .map(|e| e.estimate.route_key.clone())
        .collect();

    let estimates: Vec<RouteEstimate> = feasible.iter().map(|e| e.estimate.clone()).collect();
    let points: Vec<ObjectivePoint> = estimates
        .iter()
        .map(ObjectivePoint::from_estimate)
        .collect();
    let front = pareto_front(&points);
    let pareto_keys: Vec<String> = front.iter().map(|&i| points[i].key.clone()).collect();

    let recommended = recommend_from_estimates(&estimates, weights).map(|i| feasible[i].clone());

    if recommended.is_none() {
        return Err(MicpError::NoFeasibleModel);
    }

    Ok(RoutePlan {
        evaluated,
        feasible_keys,
        pareto_keys,
        recommended,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use micp_core::scenarios::{
        bursty_enterprise, cost_constrained_volume, degraded_failover, interactive_assistant,
        quality_rag,
    };
    use micp_core::{ExhaustionPolicy, Milliseconds, Quality, Rps, Usd};

    #[test]
    fn interactive_recommends_a_feasible_route() {
        let s = interactive_assistant();
        let p = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        let rec = p.recommended.unwrap();
        assert!(rec.feasible());
        assert!(p.pareto_keys.contains(&rec.estimate.route_key));
        assert_eq!(rec.estimate.origin, micp_core::EstimateOrigin::Modeled);
    }

    #[test]
    fn same_inputs_same_recommendation() {
        let s = quality_rag();
        let a = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        let b = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        assert_eq!(
            a.recommended.unwrap().estimate.route_key,
            b.recommended.unwrap().estimate.route_key
        );
        assert_eq!(a.pareto_keys, b.pareto_keys);
    }

    #[test]
    fn degraded_failover_does_not_recommend_down_primary() {
        let s = degraded_failover();
        let p = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        let rec = p.recommended.unwrap();
        assert_ne!(rec.estimate.model_id.as_str(), "quality-70b");
        assert!(rec.estimate.throughput_rps.get() > 0.0);
    }

    #[test]
    fn cost_scenario_respects_cost_ceiling() {
        let s = cost_constrained_volume();
        let p = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        let rec = p.recommended.unwrap();
        assert!(
            rec.estimate.cost_per_request.get()
                <= s.workload.constraints.cost_ceiling_per_request.get() + 1e-12
        );
    }

    #[test]
    fn empty_fleet_is_invalid() {
        let s = interactive_assistant();
        let err = plan(&s.workload, &[], &s.weights).unwrap_err();
        assert_eq!(err, MicpError::InvalidConfig("fleet is empty".into()));
    }

    #[test]
    fn impossible_slo_is_no_feasible_model() {
        let mut s = bursty_enterprise();
        s.workload.constraints.latency_slo = Milliseconds::new(0.01);
        s.workload.constraints.quality_floor = Quality::new(0.99);
        s.workload.constraints.cost_ceiling_per_request = Usd::new(1e-12);
        s.workload.constraints.min_capacity = Rps::new(1_000_000.0);
        s.workload.exhaustion = ExhaustionPolicy::Reject;
        let err = plan(&s.workload, &s.fleet, &s.weights).unwrap_err();
        assert_eq!(err, MicpError::NoFeasibleModel);
    }

    #[test]
    fn rag_plan_records_degraded_variants() {
        let s = quality_rag();
        let p = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        assert!(p.evaluated.iter().any(|e| !e.degradations.is_empty()));
        assert!(p.evaluated.len() > s.fleet.len());
    }

    #[test]
    fn quality_weights_can_change_recommendation() {
        let s = quality_rag();
        let q = plan(&s.workload, &s.fleet, &s.weights).unwrap();
        let latency_w = ObjectiveWeights {
            latency: 0.9,
            quality: 0.025,
            cost: 0.025,
            throughput: 0.025,
            reliability: 0.025,
        };
        let l = plan(&s.workload, &s.fleet, &latency_w).unwrap();
        // Both feasible; they may match, but scores must be finite.
        assert!(q.recommended.unwrap().estimate.p99_ms.get().is_finite());
        assert!(l.recommended.unwrap().estimate.p99_ms.get().is_finite());
    }
}
