//! Ordered policy evaluation.
//!
//! Policies never *relax* a constraint. Matching policies may only
//! tighten numeric bounds or shrink the allow-list. This keeps the
//! composition of independently authored rules monotonic.

use micp_core::{ModelId, ModelProfile, RequestConstraints, TrafficClass};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub name: String,
    /// Lower value evaluates first.
    pub priority: u32,
    pub enabled: bool,
    pub match_class: Option<TrafficClass>,
    pub max_latency_ms: Option<f64>,
    pub min_quality: Option<f64>,
    pub max_cost_per_1k: Option<f64>,
    pub max_error_rate: Option<f64>,
    pub allow_models: Option<Vec<ModelId>>,
    pub deny_models: Option<Vec<ModelId>>,
}

impl Policy {
    fn matches(&self, class: TrafficClass) -> bool {
        self.enabled && self.match_class.map(|c| c == class).unwrap_or(true)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolicySet {
    pub policies: Vec<Policy>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectivePolicy {
    pub constraints: RequestConstraints,
    pub allow: Option<Vec<ModelId>>,
    pub deny: Vec<ModelId>,
    pub applied: Vec<String>,
}

impl PolicySet {
    pub fn sorted(&self) -> Vec<&Policy> {
        let mut p: Vec<&Policy> = self.policies.iter().collect();
        p.sort_by_key(|x| x.priority);
        p
    }

    /// Fold matching policies onto `base`. Numeric bounds take the
    /// intersection (min of maxima, max of minima). Allow-lists intersect;
    /// deny-lists union.
    pub fn evaluate(&self, base: RequestConstraints) -> EffectivePolicy {
        let class = base.traffic_class;
        let mut constraints = base;
        let mut allow: Option<Vec<ModelId>> = None;
        let mut deny: Vec<ModelId> = Vec::new();
        let mut applied = Vec::new();

        for policy in self.sorted() {
            if !policy.matches(class) {
                continue;
            }
            applied.push(policy.name.clone());
            if let Some(v) = policy.max_latency_ms {
                constraints.max_latency_ms = constraints.max_latency_ms.min(v);
            }
            if let Some(v) = policy.min_quality {
                constraints.min_quality = constraints.min_quality.max(v);
            }
            if let Some(v) = policy.max_cost_per_1k {
                constraints.max_cost_per_1k = constraints.max_cost_per_1k.min(v);
            }
            if let Some(v) = policy.max_error_rate {
                constraints.require_error_rate_below = constraints.require_error_rate_below.min(v);
            }
            if let Some(ids) = &policy.allow_models {
                allow = Some(intersect_allow(allow, ids));
            }
            if let Some(ids) = &policy.deny_models {
                for id in ids {
                    if !deny.iter().any(|d| d == id) {
                        deny.push(id.clone());
                    }
                }
            }
        }

        EffectivePolicy {
            constraints,
            allow,
            deny,
            applied,
        }
    }

    pub fn filter_fleet<'a>(
        &self,
        fleet: &'a [ModelProfile],
        base: RequestConstraints,
    ) -> (Vec<&'a ModelProfile>, EffectivePolicy) {
        let effective = self.evaluate(base);
        let filtered = fleet
            .iter()
            .filter(|m| effective.admits(&m.id) && m.is_feasible(&effective.constraints))
            .collect();
        (filtered, effective)
    }
}

impl EffectivePolicy {
    pub fn admits(&self, id: &ModelId) -> bool {
        if self.deny.iter().any(|d| d == id) {
            return false;
        }
        match &self.allow {
            None => true,
            Some(allow) => allow.iter().any(|a| a == id),
        }
    }
}

fn intersect_allow(current: Option<Vec<ModelId>>, incoming: &[ModelId]) -> Vec<ModelId> {
    match current {
        None => incoming.to_vec(),
        Some(existing) => existing
            .into_iter()
            .filter(|id| incoming.iter().any(|i| i == id))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> RequestConstraints {
        RequestConstraints::interactive_default()
    }

    fn policy(name: &str, priority: u32) -> Policy {
        Policy {
            name: name.into(),
            priority,
            enabled: true,
            match_class: Some(TrafficClass::Interactive),
            max_latency_ms: None,
            min_quality: None,
            max_cost_per_1k: None,
            max_error_rate: None,
            allow_models: None,
            deny_models: None,
        }
    }

    #[test]
    fn later_policy_cannot_relax_latency() {
        let mut tight = policy("tight", 10);
        tight.max_latency_ms = Some(100.0);
        let mut loose = policy("loose", 20);
        loose.max_latency_ms = Some(500.0);
        let set = PolicySet {
            policies: vec![tight, loose],
        };
        let effective = set.evaluate(base());
        assert!((effective.constraints.max_latency_ms - 100.0).abs() < f64::EPSILON);
        assert_eq!(effective.applied, vec!["tight", "loose"]);
    }

    #[test]
    fn allow_lists_intersect() {
        let mut a = policy("a", 1);
        a.allow_models = Some(vec!["m1".into(), "m2".into()]);
        let mut b = policy("b", 2);
        b.allow_models = Some(vec!["m2".into(), "m3".into()]);
        let set = PolicySet {
            policies: vec![a, b],
        };
        let effective = set.evaluate(base());
        assert_eq!(effective.allow.unwrap(), vec![ModelId::from("m2")]);
    }

    #[test]
    fn deny_takes_precedence_over_allow() {
        let mut p = policy("p", 1);
        p.allow_models = Some(vec!["m1".into(), "m2".into()]);
        p.deny_models = Some(vec!["m1".into()]);
        let set = PolicySet { policies: vec![p] };
        let effective = set.evaluate(base());
        assert!(effective.admits(&"m2".into()));
        assert!(!effective.admits(&"m1".into()));
    }

    #[test]
    fn disabled_or_other_class_is_ignored() {
        let mut disabled = policy("off", 1);
        disabled.enabled = false;
        disabled.max_latency_ms = Some(10.0);
        let mut batch = policy("batch", 2);
        batch.match_class = Some(TrafficClass::Batch);
        batch.max_latency_ms = Some(10.0);
        let set = PolicySet {
            policies: vec![disabled, batch],
        };
        let effective = set.evaluate(base());
        assert_eq!(effective.constraints.max_latency_ms, base().max_latency_ms);
        assert!(effective.applied.is_empty());
    }
}
