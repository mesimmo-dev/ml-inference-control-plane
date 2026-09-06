export interface SensitivityRow {
  axis: string;
  path: string;
  index: number;
  value: number;
  status: string;
  feasible: boolean;
  recommended_key: string;
  model_id: string;
  origin: string;
  p50_ms: number | null;
  p95_ms: number | null;
  p99_ms: number | null;
  throughput_rps: number | null;
  utilization: number | null;
  saturated: boolean | null;
  quality: number | null;
  cost_per_request: number | null;
  n_evaluated: number;
  n_feasible: number;
  n_pareto: number;
}

export interface SensitivityTransition {
  axis: string;
  fromIndex: number;
  toIndex: number;
  fromKey: string;
  toKey: string;
  atValue: number;
  kind: "route" | "feasibility";
}

function parseCell(raw: string): string {
  return raw.replace(/^'|'$/g, "").trim();
}

function num(raw: string): number | null {
  if (!raw) return null;
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
}

function boolish(raw: string): boolean | null {
  const v = raw.toLowerCase();
  if (v === "true") return true;
  if (v === "false") return false;
  return null;
}

/** Minimal CSV parser for the curated micp-eval sensitivity artifact. */
export function parseSensitivityCsv(text: string): SensitivityRow[] {
  const lines = text.trim().split(/\r?\n/).filter((l) => l.length > 0);
  if (lines.length < 2) return [];
  const header = splitCsv(lines[0] ?? "");
  const idx = (name: string) => header.indexOf(name);
  const rows: SensitivityRow[] = [];
  for (const line of lines.slice(1)) {
    const cells = splitCsv(line).map(parseCell);
    const get = (name: string) => cells[idx(name)] ?? "";
    rows.push({
      axis: get("axis"),
      path: get("path"),
      index: Number(get("index")) || 0,
      value: Number(get("value")),
      status: get("status"),
      feasible: boolish(get("feasible")) === true,
      recommended_key: get("recommended_key"),
      model_id: get("model_id"),
      origin: get("origin") || "modeled",
      p50_ms: num(get("p50_ms")),
      p95_ms: num(get("p95_ms")),
      p99_ms: num(get("p99_ms")),
      throughput_rps: num(get("throughput_rps")),
      utilization: num(get("utilization")),
      saturated: boolish(get("saturated")),
      quality: num(get("quality")),
      cost_per_request: num(get("cost_per_request")),
      n_evaluated: Number(get("n_evaluated")) || 0,
      n_feasible: Number(get("n_feasible")) || 0,
      n_pareto: Number(get("n_pareto")) || 0,
    });
  }
  return rows;
}

function splitCsv(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  let q = false;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (q) {
      if (ch === '"') {
        if (line[i + 1] === '"') {
          cur += '"';
          i += 1;
        } else {
          q = false;
        }
      } else {
        cur += ch;
      }
    } else if (ch === '"') {
      q = true;
    } else if (ch === ",") {
      out.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  out.push(cur);
  return out;
}

export function transitions(rows: SensitivityRow[]): SensitivityTransition[] {
  const byAxis = new Map<string, SensitivityRow[]>();
  for (const row of rows) {
    const list = byAxis.get(row.axis) ?? [];
    list.push(row);
    byAxis.set(row.axis, list);
  }
  const found: SensitivityTransition[] = [];
  for (const [axis, list] of byAxis) {
    const ordered = [...list].sort((a, b) => a.index - b.index);
    for (let i = 1; i < ordered.length; i += 1) {
      const prev = ordered[i - 1]!;
      const cur = ordered[i]!;
      if (prev.feasible !== cur.feasible) {
        found.push({
          axis,
          fromIndex: prev.index,
          toIndex: cur.index,
          fromKey: prev.recommended_key || "(infeasible)",
          toKey: cur.recommended_key || "(infeasible)",
          atValue: cur.value,
          kind: "feasibility",
        });
      } else if (prev.recommended_key !== cur.recommended_key) {
        found.push({
          axis,
          fromIndex: prev.index,
          toIndex: cur.index,
          fromKey: prev.recommended_key || "(none)",
          toKey: cur.recommended_key || "(none)",
          atValue: cur.value,
          kind: "route",
        });
      }
    }
  }
  return found;
}

export function axes(rows: SensitivityRow[]): string[] {
  return [...new Set(rows.map((r) => r.axis))];
}
