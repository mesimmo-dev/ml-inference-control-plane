import { describe, expect, it } from "vitest";
import { parseSensitivityCsv, transitions } from "./artifacts";

const CSV = `axis,path,index,value,status,feasible,recommended_key,model_id,origin,p50_ms,p95_ms,p99_ms,throughput_rps,utilization,saturated,quality,cost_per_request,cost_per_1k,slo_violation_prob,failure_prob,fallback_activation_prob,n_evaluated,n_feasible,n_pareto,degradations
latency_slo,workload.constraints.latency_slo,0,20.0,no_feasible,False,,,,,,,,,,,,,,,,3,0,0,[]
latency_slo,workload.constraints.latency_slo,1,50.0,no_feasible,False,,,,,,,,,,,,,,,,3,0,0,[]
latency_slo,workload.constraints.latency_slo,2,100.0,ok,True,fast-8b:k0:none:c0:none,fast-8b,modeled,32.6,48.7,57.5,15.0,0.04,False,0.7,0.002048,2.048,2e-6,0.003,0.0,3,1,1,['fallback_model']
latency_slo,workload.constraints.latency_slo,3,200.0,ok,True,fast-8b:k0:none:c0:none,fast-8b,modeled,32.6,48.7,57.5,15.0,0.04,False,0.7,0.002048,2.048,0.0,0.003,0.0,3,1,1,['fallback_model']
`;

describe("sensitivity artifact", () => {
  it("parses curated CSV and finds the feasibility transition", () => {
    const rows = parseSensitivityCsv(CSV);
    expect(rows).toHaveLength(4);
    expect(rows[0]?.feasible).toBe(false);
    expect(rows[2]?.origin).toBe("modeled");
    expect(rows[2]?.recommended_key).toBe("fast-8b:k0:none:c0:none");
    const t = transitions(rows);
    expect(t.some((x) => x.kind === "feasibility" && x.atValue === 100)).toBe(true);
  });

  it("does not relabel modeled rows as empirical", () => {
    for (const row of parseSensitivityCsv(CSV)) {
      if (row.origin) expect(row.origin).not.toBe("empirical");
    }
  });
});
