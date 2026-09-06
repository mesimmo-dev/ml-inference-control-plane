import { ORIGIN_COPY, type ConstraintKind, type DegradationAction, type EstimateOrigin } from "./engine";

export function formatMs(v: number | null | undefined, digits = 1): string {
  if (v == null || !Number.isFinite(v)) return "—";
  if (Math.abs(v) >= 1000) return `${(v / 1000).toFixed(2)} s`;
  return `${v.toFixed(digits)} ms`;
}

export function formatRps(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${v.toFixed(v >= 100 ? 0 : 1)} rps`;
}

export function formatUsd(v: number | null | undefined, digits = 4): string {
  if (v == null || !Number.isFinite(v)) return "—";
  if (Math.abs(v) >= 1) return `$${v.toFixed(2)}`;
  return `$${v.toFixed(digits)}`;
}

export function formatProb(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  if (v === 0) return "0";
  if (v < 1e-6) return "<1e-6";
  if (v < 0.001) return v.toExponential(2);
  return v.toFixed(4);
}

export function formatPct(v: number | null | undefined, digits = 1): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${(v * 100).toFixed(digits)}%`;
}

export function formatQuality(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return v.toFixed(3);
}

export function originLabel(origin: EstimateOrigin | "local_synthetic"): string {
  if (origin === "local_synthetic") return ORIGIN_COPY.local_synthetic.label;
  return ORIGIN_COPY[origin].label;
}

export function originNote(origin: EstimateOrigin | "local_synthetic"): string {
  if (origin === "local_synthetic") return ORIGIN_COPY.local_synthetic.note;
  return ORIGIN_COPY[origin].note;
}

export function kindLabel(kind: ConstraintKind): string {
  return kind.replace(/_/g, " ");
}

export function degradationLabel(action: DegradationAction): string {
  return action.replace(/_/g, " ");
}

export function routeModel(key: string): string {
  return key.split(":")[0] ?? key;
}

export function neverEmpirical(origin: string): boolean {
  return origin !== "empirical";
}
