import { scoreCandidate } from "./score";
import type { ModelProfile, ObjectiveWeights } from "./types";

type WasmExports = {
  micp_score_candidate: (
    latency: number,
    quality: number,
    cost: number,
    capacity: number,
    error: number,
    demand: number,
    wLat: number,
    wQual: number,
    wCost: number,
    wThru: number,
    wRel: number,
  ) => number;
  micp_slo_burn_rate: (target: number, requests: number, successes: number) => number;
  micp_modeled_p99_ms: (
    intercept: number,
    msIn: number,
    msOut: number,
    inTok: number,
    outTok: number,
    extra: number,
    sigma: number,
    lambda: number,
    n: number,
  ) => number;
  micp_slo_violation_prob: (mean: number, sigma: number, slo: number) => number;
};

export type InventoryScorer = {
  source: "wasm" | "typescript_port";
  score: (model: ModelProfile, weights: ObjectiveWeights, demandRps: number) => number;
  modeledP99Ms?: WasmExports["micp_modeled_p99_ms"];
  sloViolationProb?: WasmExports["micp_slo_violation_prob"];
  burnRate?: WasmExports["micp_slo_burn_rate"];
};

function tsScorer(): InventoryScorer {
  return {
    source: "typescript_port",
    score: scoreCandidate,
  };
}

export async function loadInventoryScorer(url = "/micp_wasm.wasm"): Promise<InventoryScorer> {
  try {
    const response = await fetch(url);
    if (!response.ok) return tsScorer();
    const bytes = await response.arrayBuffer();
    const result = await WebAssembly.instantiate(bytes);
    const exp = result.instance.exports as unknown as WasmExports;
    if (typeof exp.micp_score_candidate !== "function") return tsScorer();
    return {
      source: "wasm",
      score(model, weights, demandRps) {
        return exp.micp_score_candidate(
          model.latency_p99_ms,
          model.quality,
          model.cost_per_1k_tokens,
          model.capacity_rps,
          model.error_rate,
          demandRps,
          weights.latency,
          weights.quality,
          weights.cost,
          weights.throughput,
          weights.reliability,
        );
      },
      modeledP99Ms: exp.micp_modeled_p99_ms,
      sloViolationProb: exp.micp_slo_violation_prob,
      burnRate: exp.micp_slo_burn_rate,
    };
  } catch {
    return tsScorer();
  }
}
