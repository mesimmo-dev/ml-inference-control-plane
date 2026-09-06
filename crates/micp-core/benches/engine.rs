use criterion::{black_box, criterion_group, criterion_main, Criterion};
use micp_core::{all_scenarios, estimate_route, RouteConfig};

fn estimate_all_presets(c: &mut Criterion) {
    let scenarios = all_scenarios();
    c.bench_function("estimate_all_scenario_routes", |b| {
        b.iter(|| {
            for s in &scenarios {
                for cand in &s.fleet {
                    let route = RouteConfig::baseline(cand.clone(), &s.workload);
                    let _ = black_box(estimate_route(&route, &s.workload));
                }
            }
        })
    });
}

criterion_group!(benches, estimate_all_presets);
criterion_main!(benches);
