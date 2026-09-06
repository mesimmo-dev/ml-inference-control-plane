import { describe, expect, it } from "vitest";
import { applyControls, controlsFromScenario, designRps } from "./controls";
import type { Scenario } from "./engine";

const SCENARIO: Scenario = {
  id: "interactive_assistant",
  name: "Low-latency interactive assistant",
  summary: "Closed-book chat.",
  assumptions: ["Open-loop arrivals at 15 rps."],
  weights: { latency: 0.45, quality: 0.2, cost: 0.15, throughput: 0.05, reliability: 0.15 },
  fleet: [
    {
      id: "fast-8b",
      latency: {
        intercept_ms: 20,
        ms_per_input_token: 0.02,
        ms_per_output_token: 0.08,
        retrieval_ms_per_k: 2,
        rerank_ms: 25,
        sigma_ms: 8,
      },
      quality: { base: 0.7, retrieval_gain_per_k: 0.02, rerank_gain: 0.03, context_saturation: 1024 },
      cost: { usd_per_1k_input: 0.004, usd_per_1k_output: 0.008, usd_per_retrieval: 0, usd_per_rerank: 0 },
      reliability: { base_error_rate: 0.003, timeout_as_failure: 0.5, saturation_error_slope: 0.2 },
      capacity: { max_concurrency: 12, max_tokens_per_sec: 8000, degraded_factor: 1 },
      retrieval_capable: false,
      fallback_to: "fast-8b",
    },
  ],
  workload: {
    id: "interactive_assistant",
    traffic: { class: "interactive", mean_rps: 15, concurrency: 12, burst: null },
    prompt_tokens: { mean: 256, p95: 256, p99: 256 },
    context_tokens: { mean: 0, p95: 0, p99: 0 },
    expected_output_tokens: { mean: 128, p95: 128, p99: 128 },
    retrieval: { strategy: "none", top_k: 0, rerank: "none", context_budget: 0 },
    batching: { type: "none" },
    constraints: {
      latency_slo: 200,
      quality_floor: 0.65,
      cost_ceiling_per_request: 0.01,
      reliability_target: 0.99,
      min_capacity: 15,
    },
    exhaustion: { type: "reject" },
  },
};

describe("applyControls", () => {
  it("patches traffic, SLOs, retrieval, batching, and degraded capacity", () => {
    const c = controlsFromScenario(SCENARIO);
    const patched = applyControls(SCENARIO, {
      ...c,
      meanRps: 40,
      concurrency: 20,
      burstMultiplier: 3,
      latencySlo: 120,
      qualityFloor: 0.8,
      costCeiling: 0.005,
      reliabilityTarget: 0.995,
      retrieval: "hybrid",
      topK: 8,
      rerank: "cross_encoder",
      contextBudget: 1024,
      batching: "window",
      batchMax: 4,
      batchWaitMs: 12,
      degradedFactor: 0.4,
    });
    expect(patched.workload.traffic.mean_rps).toBe(40);
    expect(patched.workload.traffic.concurrency).toBe(20);
    expect(patched.workload.traffic.burst?.peak_multiplier).toBe(3);
    expect(patched.workload.constraints.latency_slo).toBe(120);
    expect(patched.workload.retrieval.strategy).toBe("hybrid");
    expect(patched.workload.retrieval.top_k).toBe(8);
    expect(patched.workload.batching).toEqual({ type: "window", max_batch: 4, max_wait_ms: 12 });
    expect(patched.fleet[0]?.capacity.degraded_factor).toBe(0.4);
    expect(SCENARIO.workload.traffic.mean_rps).toBe(15);
  });

  it("sizes design rate to peak when bursty", () => {
    const c = controlsFromScenario(SCENARIO);
    expect(designRps({ ...c, meanRps: 10, burstMultiplier: 8 })).toBe(80);
    expect(designRps({ ...c, meanRps: 10, burstMultiplier: 1 })).toBe(10);
  });
});
