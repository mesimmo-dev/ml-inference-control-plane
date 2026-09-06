export type TrafficClass = "interactive" | "batch" | "offline";

export interface ObjectiveWeights {
  latency: number;
  quality: number;
  cost: number;
  throughput: number;
  reliability: number;
}

export interface ModelProfile {
  id: string;
  latency_p99_ms: number;
  quality: number;
  cost_per_1k_tokens: number;
  capacity_rps: number;
  error_rate: number;
}

export const SAMPLE_FLEET: ModelProfile[] = [
  {
    id: "llama-8b-fast",
    latency_p99_ms: 80,
    quality: 0.72,
    cost_per_1k_tokens: 0.04,
    capacity_rps: 120,
    error_rate: 0.004,
  },
  {
    id: "llama-70b-quality",
    latency_p99_ms: 220,
    quality: 0.91,
    cost_per_1k_tokens: 0.35,
    capacity_rps: 18,
    error_rate: 0.008,
  },
  {
    id: "mixtral-8x7b",
    latency_p99_ms: 140,
    quality: 0.84,
    cost_per_1k_tokens: 0.12,
    capacity_rps: 40,
    error_rate: 0.006,
  },
];

export const SAMPLE_WEIGHTS: ObjectiveWeights = {
  latency: 0.3,
  quality: 0.25,
  cost: 0.2,
  throughput: 0.1,
  reliability: 0.15,
};
