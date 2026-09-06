import type { Scenario, WorkbenchControls } from "./engine";

export function controlsFromScenario(scenario: Scenario): WorkbenchControls {
  const w = scenario.workload;
  const burst = w.traffic.burst;
  const batch = w.batching;
  return {
    meanRps: w.traffic.mean_rps,
    concurrency: w.traffic.concurrency,
    burstMultiplier: burst?.peak_multiplier ?? 1,
    burstPeriodS: burst?.period_s ?? 30,
    burstDuty: burst?.duty_cycle ?? 0.2,
    latencySlo: w.constraints.latency_slo,
    qualityFloor: w.constraints.quality_floor,
    costCeiling: w.constraints.cost_ceiling_per_request,
    reliabilityTarget: w.constraints.reliability_target,
    contextBudget: w.retrieval.context_budget,
    topK: w.retrieval.top_k,
    retrieval: w.retrieval.strategy,
    rerank: w.retrieval.rerank,
    batching: batch.type,
    batchMax: batch.type === "window" ? batch.max_batch : 8,
    batchWaitMs: batch.type === "window" ? batch.max_wait_ms : 15,
    degradedFactor: scenario.fleet[0]?.capacity.degraded_factor ?? 1,
  };
}

export function applyControls(scenario: Scenario, c: WorkbenchControls): Scenario {
  const next: Scenario = structuredClone(scenario);
  next.workload.traffic.mean_rps = c.meanRps;
  next.workload.traffic.concurrency = Math.max(1, Math.round(c.concurrency));
  if (c.burstMultiplier > 1) {
    next.workload.traffic.burst = {
      peak_multiplier: c.burstMultiplier,
      period_s: c.burstPeriodS,
      duty_cycle: c.burstDuty,
    };
  } else {
    next.workload.traffic.burst = null;
  }
  next.workload.constraints.latency_slo = c.latencySlo;
  next.workload.constraints.quality_floor = c.qualityFloor;
  next.workload.constraints.cost_ceiling_per_request = c.costCeiling;
  next.workload.constraints.reliability_target = c.reliabilityTarget;
  next.workload.retrieval.strategy = c.retrieval;
  next.workload.retrieval.top_k = Math.max(0, Math.round(c.topK));
  next.workload.retrieval.rerank = c.rerank;
  next.workload.retrieval.context_budget = Math.max(0, c.contextBudget);
  if (c.retrieval === "none") {
    next.workload.retrieval.top_k = 0;
    next.workload.retrieval.rerank = "none";
  }
  next.workload.batching =
    c.batching === "window"
      ? { type: "window", max_batch: Math.max(1, Math.round(c.batchMax)), max_wait_ms: c.batchWaitMs }
      : { type: "none" };
  const factor = Math.min(Math.max(c.degradedFactor, 0), 1);
  for (const model of next.fleet) {
    model.capacity.degraded_factor = factor;
  }
  return next;
}

export function designRps(c: WorkbenchControls): number {
  const base = Math.max(c.meanRps, 0);
  return c.burstMultiplier > 1 ? base * c.burstMultiplier : base;
}
