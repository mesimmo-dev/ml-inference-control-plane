import type { ModelProfile, ObjectiveWeights } from "./types";

export function normalize(w: ObjectiveWeights): ObjectiveWeights {
  const s = w.latency + w.quality + w.cost + w.throughput + w.reliability;
  if (s <= 0) {
    return {
      latency: 0.2,
      quality: 0.2,
      cost: 0.2,
      throughput: 0.2,
      reliability: 0.2,
    };
  }
  return {
    latency: w.latency / s,
    quality: w.quality / s,
    cost: w.cost / s,
    throughput: w.throughput / s,
    reliability: w.reliability / s,
  };
}

/** Mirrors `micp_core::ModelProfile::score`. Replaced by WASM when loaded. */
export function scoreCandidate(
  model: ModelProfile,
  weights: ObjectiveWeights,
  demandRps: number,
): number {
  const w = normalize(weights);
  const demand = Math.max(demandRps, 0);
  const latency = 1 / (1 + model.latency_p99_ms / 100);
  const cost = 1 / (1 + model.cost_per_1k_tokens);
  const throughput = model.capacity_rps / (model.capacity_rps + demand);
  const reliability = 1 - Math.min(Math.max(model.error_rate, 0), 1);
  const quality = Math.min(Math.max(model.quality, 0), 1);
  return (
    w.latency * latency +
    w.quality * quality +
    w.cost * cost +
    w.throughput * throughput +
    w.reliability * reliability
  );
}
