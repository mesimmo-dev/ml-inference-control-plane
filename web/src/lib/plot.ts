export function extent(values: number[], pad = 0.08, fallback: [number, number] = [0, 1]): [number, number] {
  const xs = values.filter((v) => Number.isFinite(v));
  if (xs.length === 0) return fallback;
  let lo = Math.min(...xs);
  let hi = Math.max(...xs);
  if (lo === hi) {
    const span = Math.abs(lo) * 0.1 || 1;
    lo -= span;
    hi += span;
  }
  const m = (hi - lo) * pad;
  return [lo - m, hi + m];
}

export function scale(v: number, d0: number, d1: number, r0: number, r1: number): number {
  if (d1 === d0) return (r0 + r1) / 2;
  return r0 + ((v - d0) / (d1 - d0)) * (r1 - r0);
}

export function ticks(d0: number, d1: number, count = 4): number[] {
  if (!Number.isFinite(d0) || !Number.isFinite(d1) || count < 2) return [];
  const out: number[] = [];
  for (let i = 0; i < count; i += 1) {
    out.push(d0 + ((d1 - d0) * i) / (count - 1));
  }
  return out;
}

/** Invert p50/p99 markers into lognormal (μ, σ) for a schematic density. */
export function lognormalFromPercentiles(p50: number, p99: number): { mu: number; sigma: number } | null {
  if (!(p50 > 0) || !(p99 > p50)) return null;
  const mu = Math.log(p50);
  const sigma = Math.log(p99 / p50) / 2.32635;
  if (!(sigma > 0) || !Number.isFinite(mu)) return null;
  return { mu, sigma };
}

export function lognormalPdf(x: number, mu: number, sigma: number): number {
  if (x <= 0) return 0;
  const z = (Math.log(x) - mu) / sigma;
  return Math.exp(-0.5 * z * z) / (x * sigma * Math.sqrt(2 * Math.PI));
}

export function polyline(points: Array<[number, number]>): string {
  return points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(2)} ${y.toFixed(2)}`).join(" ");
}
