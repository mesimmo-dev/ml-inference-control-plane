//! Four-objective Pareto front and deterministic recommendation.
//!
//! Objectives (user-specified):
//! - latency: minimize modeled p99
//! - quality: maximize quality proxy
//! - cost: minimize USD/request
//! - reliability: maximize 1 - failure_prob
//!
//! Throughput/utilization is a hard constraint plus a fifth *weight*
//! used only when scoring the front, not when deciding domination.

use micp_core::{ObjectiveWeights, RouteEstimate};

const EPS: f64 = 1e-12;

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectivePoint {
    pub key: String,
    pub latency_ms: f64,
    pub quality: f64,
    pub cost: f64,
    pub reliability: f64,
}

impl ObjectivePoint {
    pub fn from_estimate(e: &RouteEstimate) -> Self {
        Self {
            key: e.route_key.clone(),
            latency_ms: e.p99_ms.get(),
            quality: e.quality.get(),
            cost: e.cost_per_request.get(),
            reliability: 1.0 - e.failure_prob.get(),
        }
    }
}

/// True when `b` dominates `a` (b is at least as good on all four and
/// strictly better on one).
pub fn dominates(b: &ObjectivePoint, a: &ObjectivePoint) -> bool {
    let le = b.latency_ms <= a.latency_ms + EPS
        && b.cost <= a.cost + EPS
        && b.quality + EPS >= a.quality
        && b.reliability + EPS >= a.reliability;
    let lt = b.latency_ms < a.latency_ms - EPS
        || b.cost < a.cost - EPS
        || b.quality > a.quality + EPS
        || b.reliability > a.reliability + EPS;
    le && lt
}

/// Indices of the non-dominated subset, sorted by key for stability.
pub fn pareto_front(points: &[ObjectivePoint]) -> Vec<usize> {
    let mut front: Vec<usize> = (0..points.len())
        .filter(|&i| {
            !points
                .iter()
                .enumerate()
                .any(|(j, b)| j != i && dominates(b, &points[i]))
        })
        .collect();
    front.sort_by_key(|&i| points[i].key.clone());
    front
}

/// Pick an operating point from `points` restricted to the Pareto front.
///
/// Score is [`RouteEstimate::weighted_score`] when an estimate is
/// supplied via [`recommend_from_estimates`]; this function uses the
/// same formula on the four objectives plus a zero throughput term.
/// Tie-break: higher score, then lexicographically smaller key.
pub fn recommend(points: &[ObjectivePoint], weights: &ObjectiveWeights) -> Option<usize> {
    let front = pareto_front(points);
    recommend_among(points, &front, weights)
}

pub fn recommend_among(
    points: &[ObjectivePoint],
    among: &[usize],
    weights: &ObjectiveWeights,
) -> Option<usize> {
    let w = weights.normalized();
    among.iter().copied().max_by(|&i, &j| {
        let si = score(&points[i], w);
        let sj = score(&points[j], w);
        si.partial_cmp(&sj)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| points[j].key.cmp(&points[i].key))
    })
}

pub fn recommend_from_estimates(
    estimates: &[RouteEstimate],
    weights: &ObjectiveWeights,
) -> Option<usize> {
    if estimates.is_empty() {
        return None;
    }
    let points: Vec<ObjectivePoint> = estimates
        .iter()
        .map(ObjectivePoint::from_estimate)
        .collect();
    let front = pareto_front(&points);
    front.iter().copied().max_by(|&i, &j| {
        let si = estimates[i].weighted_score(weights);
        let sj = estimates[j].weighted_score(weights);
        si.partial_cmp(&sj)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| estimates[j].route_key.cmp(&estimates[i].route_key))
    })
}

fn score(p: &ObjectivePoint, w: ObjectiveWeights) -> f64 {
    let latency = 1.0 / (1.0 + p.latency_ms / 100.0);
    let cost = 1.0 / (1.0 + p.cost * 1000.0);
    w.latency * latency
        + w.quality * p.quality.clamp(0.0, 1.0)
        + w.cost * cost
        + w.reliability * p.reliability.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(key: &str, lat: f64, q: f64, cost: f64, rel: f64) -> ObjectivePoint {
        ObjectivePoint {
            key: key.into(),
            latency_ms: lat,
            quality: q,
            cost,
            reliability: rel,
        }
    }

    #[test]
    fn dominated_point_is_not_on_the_front() {
        let pts = vec![
            p("a", 100.0, 0.9, 0.01, 0.99),
            p("b", 200.0, 0.8, 0.02, 0.98), // dominated by a
            p("c", 80.0, 0.7, 0.005, 0.99),
        ];
        let front = pareto_front(&pts);
        let keys: Vec<_> = front.iter().map(|&i| pts[i].key.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"c"));
        assert!(!keys.contains(&"b"));
    }

    #[test]
    fn incomparable_points_both_survive() {
        let pts = vec![
            p("fast", 50.0, 0.7, 0.01, 0.99),
            p("qual", 200.0, 0.95, 0.04, 0.99),
        ];
        let front = pareto_front(&pts);
        assert_eq!(front.len(), 2);
    }

    #[test]
    fn equal_points_are_both_non_dominated() {
        let pts = vec![
            p("aaa", 100.0, 0.8, 0.01, 0.99),
            p("bbb", 100.0, 0.8, 0.01, 0.99),
        ];
        let front = pareto_front(&pts);
        assert_eq!(front.len(), 2);
    }

    #[test]
    fn recommend_tie_breaks_by_key() {
        let pts = vec![
            p("bbb", 100.0, 0.8, 0.01, 0.99),
            p("aaa", 100.0, 0.8, 0.01, 0.99),
        ];
        let idx = recommend(&pts, &ObjectiveWeights::balanced()).unwrap();
        assert_eq!(pts[idx].key, "aaa");
    }

    #[test]
    fn recommend_follows_quality_weight() {
        let pts = vec![
            p("fast", 50.0, 0.70, 0.01, 0.99),
            p("qual", 180.0, 0.95, 0.02, 0.99),
        ];
        let quality = ObjectiveWeights {
            latency: 0.05,
            quality: 0.85,
            cost: 0.05,
            throughput: 0.0,
            reliability: 0.05,
        };
        let idx = recommend(&pts, &quality).unwrap();
        assert_eq!(pts[idx].key, "qual");
        let latency = ObjectiveWeights {
            latency: 0.85,
            quality: 0.05,
            cost: 0.05,
            throughput: 0.0,
            reliability: 0.05,
        };
        let idx = recommend(&pts, &latency).unwrap();
        assert_eq!(pts[idx].key, "fast");
    }

    #[test]
    fn empty_has_no_recommendation() {
        assert!(recommend(&[], &ObjectiveWeights::balanced()).is_none());
    }

    #[test]
    fn front_is_sorted_by_key() {
        let pts = vec![
            p("z", 10.0, 0.9, 0.01, 0.99),
            p("a", 11.0, 0.91, 0.012, 0.99),
        ];
        let front = pareto_front(&pts);
        let keys: Vec<_> = front.iter().map(|&i| pts[i].key.as_str()).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn no_front_member_is_dominated(
            lat in prop::collection::vec(1.0f64..500.0, 2..8),
            q in prop::collection::vec(0.0f64..1.0, 2..8),
            c in prop::collection::vec(0.0f64..0.1, 2..8),
            r in prop::collection::vec(0.5f64..1.0, 2..8),
        ) {
            let n = lat.len().min(q.len()).min(c.len()).min(r.len());
            let pts: Vec<ObjectivePoint> = (0..n)
                .map(|i| ObjectivePoint {
                    key: format!("k{i:02}"),
                    latency_ms: lat[i],
                    quality: q[i],
                    cost: c[i],
                    reliability: r[i],
                })
                .collect();
            let front = pareto_front(&pts);
            for &i in &front {
                for (j, b) in pts.iter().enumerate() {
                    if i != j {
                        prop_assert!(!dominates(b, &pts[i]));
                    }
                }
            }
            if let Some(idx) = recommend(&pts, &ObjectiveWeights::balanced()) {
                prop_assert!(front.contains(&idx));
            }
        }
    }
}
