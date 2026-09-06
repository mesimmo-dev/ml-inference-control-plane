//! Production-style degradation and fallback expansion.
//!
//! The ladder never relaxes a constraint. It only produces *additional*
//! candidate routes (disable rerank, reduce k, cut context, alter
//! batching, switch to a declared fallback model) so the optimizer can
//! pick among them. Duplicates are suppressed by [`RouteConfig::key`].

use std::collections::HashMap;

use micp_core::{
    BatchingPolicy, DegradationAction, InferenceCandidate, Milliseconds, RerankStrategy,
    RetrievalStrategy, RouteConfig, Tokens, WorkloadProfile,
};

/// One expanded operating point and the actions that produced it from baseline.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedRoute {
    pub route: RouteConfig,
    pub actions: Vec<DegradationAction>,
}

/// Expand each fleet member into a small, deterministic set of variants.
pub fn expand_routes(
    fleet: &[InferenceCandidate],
    workload: &WorkloadProfile,
) -> Vec<ExpandedRoute> {
    let mut out: Vec<ExpandedRoute> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for cand in fleet {
        for var in variants_for(cand, workload, fleet) {
            let key = var.route.key();
            if let Some(&i) = index.get(&key) {
                // Same operating point, extra reason (e.g. declared fallback
                // lands on a degrade of a fleet member already expanded).
                for action in var.actions {
                    if !out[i].actions.contains(&action) {
                        out[i].actions.push(action);
                    }
                }
            } else {
                index.insert(key, out.len());
                out.push(var);
            }
        }
    }
    out.sort_by_key(|a| a.route.key());
    out
}

fn variants_for(
    cand: &InferenceCandidate,
    workload: &WorkloadProfile,
    fleet: &[InferenceCandidate],
) -> Vec<ExpandedRoute> {
    let mut out = Vec::new();
    let base = RouteConfig::baseline(cand.clone(), workload);
    out.push(ExpandedRoute {
        route: base.clone(),
        actions: Vec::new(),
    });

    if base.retrieval.rerank != RerankStrategy::None {
        let mut r = base.clone();
        r.retrieval.rerank = RerankStrategy::None;
        out.push(ExpandedRoute {
            route: r,
            actions: vec![DegradationAction::DisableRerank],
        });
    }

    let k = base.retrieval.effective_k();
    if k > 1 {
        let mut r = base.clone();
        r.retrieval.top_k = (k / 2).max(1);
        r.retrieval.rerank = RerankStrategy::None;
        let mut actions = vec![DegradationAction::ReduceRetrieval];
        if base.retrieval.rerank != RerankStrategy::None {
            actions.insert(0, DegradationAction::DisableRerank);
        }
        out.push(ExpandedRoute { route: r, actions });
    }

    if base.context_budget.get() > 128.0 && base.retrieval.strategy != RetrievalStrategy::None {
        let mut r = base.clone();
        r.context_budget = Tokens::new((base.context_budget.get() / 2.0).max(128.0));
        r.retrieval.context_budget = r.context_budget;
        r.retrieval.rerank = RerankStrategy::None;
        if k > 1 {
            r.retrieval.top_k = (k / 2).max(1);
        }
        out.push(ExpandedRoute {
            route: r,
            actions: vec![
                DegradationAction::DisableRerank,
                DegradationAction::ReduceRetrieval,
                DegradationAction::ReduceContext,
            ],
        });
    }

    match base.batching {
        BatchingPolicy::None if workload.traffic.design_rps() >= 30.0 => {
            let mut r = base.clone();
            r.batching = BatchingPolicy::Window {
                max_batch: 8,
                max_wait_ms: Milliseconds::new(15.0),
            };
            out.push(ExpandedRoute {
                route: r,
                actions: vec![DegradationAction::AlterBatching],
            });
        }
        BatchingPolicy::Window { .. } => {
            let mut r = base.clone();
            r.batching = BatchingPolicy::None;
            out.push(ExpandedRoute {
                route: r,
                actions: vec![DegradationAction::AlterBatching],
            });
        }
        _ => {}
    }

    if let Some(fb) = &cand.fallback_to {
        if let Some(other) = fleet.iter().find(|c| &c.id == fb) {
            let mut r = RouteConfig::baseline(other.clone(), workload);
            r.retrieval.rerank = RerankStrategy::None;
            if r.retrieval.effective_k() > 1 {
                r.retrieval.top_k = (r.retrieval.effective_k() / 2).max(1);
            }
            out.push(ExpandedRoute {
                route: r,
                actions: vec![DegradationAction::FallbackModel],
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use micp_core::scenarios::{interactive_assistant, quality_rag};
    use std::collections::HashSet;

    #[test]
    fn expansion_is_deterministic_and_unique() {
        let s = quality_rag();
        let a = expand_routes(&s.fleet, &s.workload);
        let b = expand_routes(&s.fleet, &s.workload);
        let keys_a: Vec<_> = a.iter().map(|e| e.route.key()).collect();
        let keys_b: Vec<_> = b.iter().map(|e| e.route.key()).collect();
        assert_eq!(keys_a, keys_b);
        let set: HashSet<_> = keys_a.iter().cloned().collect();
        assert_eq!(set.len(), keys_a.len());
        assert!(a.len() > s.fleet.len());
    }

    #[test]
    fn rag_expansion_includes_disable_rerank_and_reduce_k() {
        let s = quality_rag();
        let exp = expand_routes(&s.fleet, &s.workload);
        assert!(exp
            .iter()
            .any(|e| e.actions.contains(&DegradationAction::DisableRerank)));
        assert!(exp
            .iter()
            .any(|e| e.actions.contains(&DegradationAction::ReduceRetrieval)));
        assert!(exp
            .iter()
            .any(|e| e.actions.contains(&DegradationAction::ReduceContext)));
        assert!(exp
            .iter()
            .any(|e| e.actions.contains(&DegradationAction::FallbackModel)));
    }

    #[test]
    fn closed_book_has_no_retrieval_degrades() {
        let s = interactive_assistant();
        let exp = expand_routes(&s.fleet, &s.workload);
        assert!(!exp
            .iter()
            .any(|e| e.actions.contains(&DegradationAction::ReduceRetrieval)));
    }
}
