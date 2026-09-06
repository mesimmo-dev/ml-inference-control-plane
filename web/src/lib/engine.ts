/** JSON shapes for the Rust engine and Python artifacts. Field names match serde. */

export type EstimateOrigin = "modeled" | "simulated" | "empirical";

export type TrafficClass = "interactive" | "batch" | "offline";

export type RetrievalStrategy = "none" | "sparse" | "dense" | "hybrid";

export type RerankStrategy = "none" | "cross_encoder" | "llm";

export type ConstraintKind = "latency" | "quality" | "cost" | "reliability" | "capacity";

export type DegradationAction =
  | "disable_rerank"
  | "reduce_retrieval"
  | "reduce_context"
  | "alter_batching"
  | "fallback_model"
  | "queue"
  | "reject";

export type BatchingPolicy =
  | { type: "none" }
  | { type: "window"; max_batch: number; max_wait_ms: number };

export type ExhaustionPolicy = { type: "reject" } | { type: "queue"; max_wait_ms: number };

export interface ObjectiveWeights {
  latency: number;
  quality: number;
  cost: number;
  throughput: number;
  reliability: number;
}

export interface BurstSpec {
  peak_multiplier: number;
  period_s: number;
  duty_cycle: number;
}

export interface TrafficProfile {
  class: TrafficClass;
  mean_rps: number;
  concurrency: number;
  burst: BurstSpec | null;
}

export interface TokenDistribution {
  mean: number;
  p95: number;
  p99: number;
}

export interface RetrievalConfig {
  strategy: RetrievalStrategy;
  top_k: number;
  rerank: RerankStrategy;
  context_budget: number;
}

export interface SloConstraints {
  latency_slo: number;
  quality_floor: number;
  cost_ceiling_per_request: number;
  reliability_target: number;
  min_capacity: number;
}

export interface WorkloadProfile {
  id: string;
  traffic: TrafficProfile;
  prompt_tokens: TokenDistribution;
  context_tokens: TokenDistribution;
  expected_output_tokens: TokenDistribution;
  retrieval: RetrievalConfig;
  batching: BatchingPolicy;
  constraints: SloConstraints;
  exhaustion: ExhaustionPolicy;
}

export interface LatencyCurve {
  intercept_ms: number;
  ms_per_input_token: number;
  ms_per_output_token: number;
  retrieval_ms_per_k: number;
  rerank_ms: number;
  sigma_ms: number;
}

export interface CostModel {
  usd_per_1k_input: number;
  usd_per_1k_output: number;
  usd_per_retrieval: number;
  usd_per_rerank: number;
}

export interface QualityModel {
  base: number;
  retrieval_gain_per_k: number;
  rerank_gain: number;
  context_saturation: number;
}

export interface ReliabilityModel {
  base_error_rate: number;
  timeout_as_failure: number;
  saturation_error_slope: number;
}

export interface CapacityModel {
  max_concurrency: number;
  max_tokens_per_sec: number;
  degraded_factor: number;
}

export interface InferenceCandidate {
  id: string;
  latency: LatencyCurve;
  quality: QualityModel;
  cost: CostModel;
  reliability: ReliabilityModel;
  capacity: CapacityModel;
  retrieval_capable: boolean;
  fallback_to: string | null;
}

export interface Scenario {
  id: string;
  name: string;
  summary: string;
  assumptions: string[];
  workload: WorkloadProfile;
  fleet: InferenceCandidate[];
  weights: ObjectiveWeights;
}

export interface ScenarioCard {
  id: string;
  name: string;
  summary: string;
  assumptions: string[];
}

export interface RouteEstimate {
  origin: EstimateOrigin;
  route_key: string;
  model_id: string;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  throughput_rps: number;
  utilization: number;
  saturated: boolean;
  quality: number;
  cost_per_request: number;
  cost_per_1k: number;
  slo_violation_prob: number;
  failure_prob: number;
  fallback_activation_prob: number;
}

export interface ConstraintViolation {
  kind: ConstraintKind;
  message: string;
  observed: number;
  limit: number;
}

export interface RouteConfig {
  candidate: InferenceCandidate;
  retrieval: RetrievalConfig;
  batching: BatchingPolicy;
  context_budget: number;
}

export interface EvaluatedRoute {
  route: RouteConfig;
  estimate: RouteEstimate;
  violations: ConstraintViolation[];
  degradations: DegradationAction[];
}

export interface RoutePlan {
  evaluated: EvaluatedRoute[];
  feasible_keys: string[];
  pareto_keys: string[];
  recommended: EvaluatedRoute | null;
}

export interface SimReport {
  origin: EstimateOrigin;
  n_arrivals: number;
  n_served: number;
  n_rejected: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  mean_sojourn_ms: number;
  mean_wait_ms: number;
  utilization: number;
  notes: string[];
}

export interface HealthBody {
  status: string;
  service: string;
}

export interface RecommendBody {
  origin: string;
  plan: RoutePlan;
}

export interface WorkbenchControls {
  meanRps: number;
  concurrency: number;
  burstMultiplier: number;
  burstPeriodS: number;
  burstDuty: number;
  latencySlo: number;
  qualityFloor: number;
  costCeiling: number;
  reliabilityTarget: number;
  contextBudget: number;
  topK: number;
  retrieval: RetrievalStrategy;
  rerank: RerankStrategy;
  batching: "none" | "window";
  batchMax: number;
  batchWaitMs: number;
  degradedFactor: number;
}

export const ORIGIN_COPY: Record<
  "modeled" | "simulated" | "local_synthetic" | "empirical",
  { label: string; note: string }
> = {
  modeled: {
    label: "MODELED",
    note: "Closed-form M/M/n sojourn with a lognormal tail. Not a measurement.",
  },
  simulated: {
    label: "SIMULATED",
    note: "Seeded G/G/n discrete-event run. Not a production trace.",
  },
  local_synthetic: {
    label: "LOCAL SYNTHETIC BENCHMARK",
    note: "Inventory score helper (WASM C ABI or TypeScript port of ModelProfile::score). Not a route plan.",
  },
  empirical: {
    label: "EMPIRICAL",
    note: "Reserved for telemetry-backed estimates. Unused in this repository.",
  },
};
