import type { ReactNode } from "react";
import type { SensitivityRow } from "../lib/artifacts";
import type { RoutePlan, SimReport } from "../lib/engine";
import { degradationLabel, formatMs, formatPct, formatUsd, formatQuality } from "../lib/format";
import { extent, lognormalFromPercentiles, lognormalPdf, polyline, scale, ticks } from "../lib/plot";

const W = 520;
const H = 280;
const M = { t: 28, r: 18, b: 42, l: 52 };
const IW = W - M.l - M.r;
const IH = H - M.t - M.b;

export function Charts({
  plan,
  sim,
  sloMs,
  sensitivity,
}: {
  plan: RoutePlan | null;
  sim: SimReport | null;
  sloMs: number;
  sensitivity: SensitivityRow[];
}) {
  if (!plan) {
    return (
      <p className="note">
        Charts appear after a live evaluation. They are not filled with synthetic
        stand-ins while the engine is unreachable.
      </p>
    );
  }
  const latencyRows = sensitivity.filter((r) => r.axis === "latency_slo");
  return (
    <div className="chart-grid">
      <ParetoChart plan={plan} />
      <LatencyChart plan={plan} sim={sim} sloMs={sloMs} />
      <CostQualityChart plan={plan} />
      <ThroughputChart plan={plan} />
      <CompareChart plan={plan} />
      {latencyRows.length > 0 ? <SensitivityChart rows={latencyRows} /> : null}
      <FallbackChart plan={plan} />
    </div>
  );
}

function Frame({
  title,
  caption,
  children,
  yLabel,
  xLabel,
}: {
  title: string;
  caption: string;
  children: ReactNode;
  yLabel?: string;
  xLabel?: string;
}) {
  return (
    <figure className="figure chart">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={title}>
        <text x={M.l} y={16} className="svg-kicker">
          {title}
        </text>
        <line x1={M.l} y1={M.t} x2={M.l} y2={M.t + IH} className="svg-axis" />
        <line x1={M.l} y1={M.t + IH} x2={M.l + IW} y2={M.t + IH} className="svg-axis" />
        {yLabel ? (
          <text
            x={14}
            y={M.t + IH / 2}
            className="svg-axis-label"
            transform={`rotate(-90 14 ${M.t + IH / 2})`}
          >
            {yLabel}
          </text>
        ) : null}
        {xLabel ? (
          <text x={M.l + IW / 2} y={H - 8} className="svg-axis-label" textAnchor="middle">
            {xLabel}
          </text>
        ) : null}
        {children}
      </svg>
      <figcaption>{caption}</figcaption>
    </figure>
  );
}

function AxisTicks({
  xDomain,
  yDomain,
  xFmt,
  yFmt,
}: {
  xDomain: [number, number];
  yDomain: [number, number];
  xFmt?: (n: number) => string;
  yFmt?: (n: number) => string;
}) {
  const xf = xFmt ?? ((n: number) => n.toFixed(2));
  const yf = yFmt ?? ((n: number) => n.toFixed(1));
  return (
    <>
      {ticks(xDomain[0], xDomain[1], 4).map((t) => {
        const x = scale(t, xDomain[0], xDomain[1], M.l, M.l + IW);
        return (
          <g key={`x${t}`}>
            <line x1={x} y1={M.t + IH} x2={x} y2={M.t + IH + 4} className="svg-axis" />
            <text x={x} y={M.t + IH + 16} className="svg-tick" textAnchor="middle">
              {xf(t)}
            </text>
          </g>
        );
      })}
      {ticks(yDomain[0], yDomain[1], 4).map((t) => {
        const y = scale(t, yDomain[0], yDomain[1], M.t + IH, M.t);
        return (
          <g key={`y${t}`}>
            <line x1={M.l - 4} y1={y} x2={M.l} y2={y} className="svg-axis" />
            <text x={M.l - 8} y={y + 3} className="svg-tick" textAnchor="end">
              {yf(t)}
            </text>
          </g>
        );
      })}
    </>
  );
}

function ParetoChart({ plan }: { plan: RoutePlan }) {
  const pts = plan.evaluated.map((e) => e.estimate);
  const xD = extent(pts.map((p) => p.quality), 0.12, [0.5, 1]);
  const yD = extent(pts.map((p) => p.p99_ms), 0.12, [0, 200]);
  const pareto = new Set(plan.pareto_keys);
  const rec = plan.recommended?.estimate.route_key;
  const front = plan.evaluated
    .filter((e) => pareto.has(e.estimate.route_key))
    .sort((a, b) => a.estimate.quality - b.estimate.quality);
  const frontLine = polyline(
    front.map((e) => [
      scale(e.estimate.quality, xD[0], xD[1], M.l, M.l + IW),
      scale(e.estimate.p99_ms, yD[0], yD[1], M.t + IH, M.t),
    ]),
  );
  return (
    <Frame
      title="FIG. 2  ·  PARETO FRONT  ·  MODELED"
      caption="Quality (higher better) against modeled p99. Filled marks are Pareto-efficient. The recommended route is ringed. Infeasible candidates remain visible as open marks."
      xLabel="quality proxy"
      yLabel="p99 ms"
    >
      <AxisTicks xDomain={xD} yDomain={yD} xFmt={(n) => n.toFixed(2)} yFmt={(n) => n.toFixed(0)} />
      {front.length > 1 ? <path d={frontLine} className="svg-front" fill="none" /> : null}
      {plan.evaluated.map((e) => {
        const x = scale(e.estimate.quality, xD[0], xD[1], M.l, M.l + IW);
        const y = scale(e.estimate.p99_ms, yD[0], yD[1], M.t + IH, M.t);
        const on = pareto.has(e.estimate.route_key);
        const isRec = e.estimate.route_key === rec;
        const feasible = e.violations.length === 0;
        return (
          <g key={e.estimate.route_key}>
            <circle
              cx={x}
              cy={y}
              r={isRec ? 6 : 4}
              fill={on ? "var(--ink)" : "none"}
              stroke={feasible ? "var(--ink)" : "var(--oxide)"}
              strokeWidth={isRec ? 1.6 : 1}
              strokeDasharray={feasible ? undefined : "2 2"}
            />
          </g>
        );
      })}
    </Frame>
  );
}

function LatencyChart({
  plan,
  sim,
  sloMs,
}: {
  plan: RoutePlan;
  sim: SimReport | null;
  sloMs: number;
}) {
  const rec = plan.recommended?.estimate;
  const p50 = rec?.p50_ms ?? 30;
  const p99 = rec?.p99_ms ?? 80;
  const params = lognormalFromPercentiles(p50, Math.max(p99, p50 * 1.2));
  const xMax = Math.max(sloMs * 1.15, p99 * 1.4, sim?.p99_ms ?? 0, 40);
  const xD: [number, number] = [0, xMax];
  const xs: number[] = [];
  for (let i = 1; i <= 60; i += 1) xs.push((xMax * i) / 60);
  const ys = params ? xs.map((x) => lognormalPdf(x, params.mu, params.sigma)) : xs.map(() => 0);
  const yD = extent(ys, 0.05, [0, 1]);
  const line = polyline(
    xs.map((x, i) => [
      scale(x, xD[0], xD[1], M.l, M.l + IW),
      scale(ys[i] ?? 0, yD[0], yD[1], M.t + IH, M.t),
    ]),
  );
  const xAt = (v: number) => scale(v, xD[0], xD[1], M.l, M.l + IW);
  return (
    <Frame
      title="FIG. 3  ·  LATENCY  ·  MODELED SCHEMA"
      caption="Schematic lognormal fitted to modeled p50/p99 — not a histogram of traces. The dashed rule is the latency SLO. Simulated percentiles, when present, are ticks on the same axis."
      xLabel="sojourn ms"
      yLabel="density (schematic)"
    >
      <AxisTicks xDomain={xD} yDomain={yD} xFmt={(n) => n.toFixed(0)} yFmt={() => ""} />
      {params ? <path d={line} className="svg-front" fill="none" /> : null}
      <line x1={xAt(sloMs)} y1={M.t} x2={xAt(sloMs)} y2={M.t + IH} className="svg-slo" />
      <text x={xAt(sloMs) + 4} y={M.t + 12} className="svg-tick">
        SLO {sloMs.toFixed(0)}
      </text>
      {rec ? (
        <>
          <Marker x={xAt(rec.p50_ms)} label="p50" />
          <Marker x={xAt(rec.p95_ms)} label="p95" />
          <Marker x={xAt(rec.p99_ms)} label="p99" />
        </>
      ) : null}
      {sim && sim.origin === "simulated" ? (
        <>
          <Marker x={xAt(sim.p50_ms)} label="sim p50" alt />
          <Marker x={xAt(sim.p99_ms)} label="sim p99" alt />
        </>
      ) : null}
    </Frame>
  );
}

function Marker({ x, label, alt }: { x: number; label: string; alt?: boolean }) {
  return (
    <g>
      <line
        x1={x}
        y1={M.t + IH - 8}
        x2={x}
        y2={M.t + IH}
        stroke={alt ? "var(--pine)" : "var(--ink)"}
        strokeWidth={1}
      />
      <text x={x} y={M.t + 28} className="svg-tick" textAnchor="middle" fill={alt ? "var(--pine)" : undefined}>
        {label}
      </text>
    </g>
  );
}

function CostQualityChart({ plan }: { plan: RoutePlan }) {
  const pts = plan.evaluated.map((e) => e.estimate);
  const xD = extent(pts.map((p) => p.quality), 0.1, [0.5, 1]);
  const yD = extent(pts.map((p) => p.cost_per_request), 0.15, [0, 0.01]);
  const rec = plan.recommended?.estimate.route_key;
  const pareto = new Set(plan.pareto_keys);
  return (
    <Frame
      title="FIG. 4  ·  COST vs QUALITY  ·  MODELED"
      caption="USD per request against the quality proxy. Lower-right is cheaper and better. Ringed mark is the recommendation."
      xLabel="quality proxy"
      yLabel="USD / request"
    >
      <AxisTicks xDomain={xD} yDomain={yD} xFmt={(n) => n.toFixed(2)} yFmt={(n) => n.toFixed(4)} />
      {plan.evaluated.map((e) => {
        const x = scale(e.estimate.quality, xD[0], xD[1], M.l, M.l + IW);
        const y = scale(e.estimate.cost_per_request, yD[0], yD[1], M.t + IH, M.t);
        const isRec = e.estimate.route_key === rec;
        return (
          <circle
            key={e.estimate.route_key}
            cx={x}
            cy={y}
            r={isRec ? 6 : 4}
            fill={pareto.has(e.estimate.route_key) ? "var(--ink)" : "var(--paper)"}
            stroke="var(--ink)"
            strokeWidth={1}
          />
        );
      })}
    </Frame>
  );
}

function ThroughputChart({ plan }: { plan: RoutePlan }) {
  const pts = plan.evaluated;
  const xD = extent(pts.map((p) => p.estimate.utilization), 0.1, [0, 1]);
  const yD = extent(pts.map((p) => p.estimate.throughput_rps), 0.12, [0, 20]);
  const rec = plan.recommended?.estimate.route_key;
  return (
    <Frame
      title="FIG. 5  ·  THROUGHPUT vs UTILIZATION  ·  MODELED"
      caption="Offered throughput against occupancy ρ. Saturated routes (ρ → 1 under reject) sit at the right edge."
      xLabel="utilization ρ"
      yLabel="throughput rps"
    >
      <AxisTicks xDomain={xD} yDomain={yD} xFmt={(n) => n.toFixed(2)} yFmt={(n) => n.toFixed(0)} />
      {pts.map((e) => {
        const x = scale(e.estimate.utilization, xD[0], xD[1], M.l, M.l + IW);
        const y = scale(e.estimate.throughput_rps, yD[0], yD[1], M.t + IH, M.t);
        return (
          <rect
            key={e.estimate.route_key}
            x={x - 4}
            y={y - 4}
            width={8}
            height={8}
            fill={e.estimate.route_key === rec ? "var(--ink)" : "var(--paper)"}
            stroke={e.estimate.saturated ? "var(--oxide)" : "var(--ink)"}
            transform={`rotate(45 ${x} ${y})`}
          />
        );
      })}
    </Frame>
  );
}

function CompareChart({ plan }: { plan: RoutePlan }) {
  const rows = [...plan.evaluated].sort((a, b) => a.estimate.p99_ms - b.estimate.p99_ms);
  const maxP99 = Math.max(...rows.map((r) => r.estimate.p99_ms), 1);
  const rec = plan.recommended?.estimate.route_key;
  const rowH = Math.min(22, IH / Math.max(rows.length, 1));
  return (
    <Frame
      title="FIG. 6  ·  CANDIDATE p99  ·  MODELED"
      caption="Horizontal comparison of modeled p99. Solid bars are feasible; hatched bars violate at least one constraint."
      xLabel="p99 ms"
    >
      {rows.map((e, i) => {
        const y = M.t + 8 + i * rowH;
        const w = scale(e.estimate.p99_ms, 0, maxP99, 0, IW * 0.72);
        const feasible = e.violations.length === 0;
        return (
          <g key={e.estimate.route_key}>
            <rect
              x={M.l}
              y={y}
              width={Math.max(w, 1)}
              height={Math.max(rowH - 4, 4)}
              fill={e.estimate.route_key === rec ? "var(--ink)" : "var(--paper-2)"}
              stroke="var(--ink)"
              strokeDasharray={feasible ? undefined : "3 2"}
            />
            <text x={M.l + w + 6} y={y + rowH - 8} className="svg-tick">
              {e.estimate.model_id} {formatMs(e.estimate.p99_ms, 0)}
            </text>
          </g>
        );
      })}
    </Frame>
  );
}

function SensitivityChart({ rows }: { rows: SensitivityRow[] }) {
  const ordered = [...rows].sort((a, b) => a.value - b.value);
  const xD = extent(ordered.map((r) => r.value), 0.05, [0, 800]);
  const yVals = ordered.map((r) => r.p99_ms).filter((v): v is number => v != null);
  const yD = extent(yVals, 0.15, [0, 80]);
  const linePts = ordered
    .filter((r) => r.feasible && r.p99_ms != null)
    .map((r) => [
      scale(r.value, xD[0], xD[1], M.l, M.l + IW),
      scale(r.p99_ms ?? 0, yD[0], yD[1], M.t + IH, M.t),
    ] as [number, number]);
  return (
    <Frame
      title="FIG. 7  ·  SENSITIVITY  ·  PYTHON ARTIFACT  ·  MODELED"
      caption="One-at-a-time latency-SLO sweep from the curated Python evaluation artifact (interactive_assistant). Open circles are infeasible. This is a recorded modeled grid, not a live re-sweep."
      xLabel="latency SLO (ms)"
      yLabel="recommended p99"
    >
      <AxisTicks xDomain={xD} yDomain={yD} xFmt={(n) => n.toFixed(0)} yFmt={(n) => n.toFixed(0)} />
      {linePts.length > 1 ? <path d={polyline(linePts)} className="svg-front" fill="none" /> : null}
      {ordered.map((r) => {
        const x = scale(r.value, xD[0], xD[1], M.l, M.l + IW);
        const y = r.p99_ms != null ? scale(r.p99_ms, yD[0], yD[1], M.t + IH, M.t) : M.t + IH;
        return (
          <circle
            key={`${r.axis}-${r.index}`}
            cx={x}
            cy={y}
            r={4}
            fill={r.feasible ? "var(--ink)" : "none"}
            stroke="var(--ink)"
          />
        );
      })}
    </Frame>
  );
}

function FallbackChart({ plan }: { plan: RoutePlan }) {
  const rec = plan.recommended;
  const fb = rec?.route.candidate.fallback_to;
  const fallbackRoute = fb
    ? plan.evaluated.find((e) => e.estimate.model_id === fb && e.degradations.includes("fallback_model")) ??
      plan.evaluated.find((e) => e.estimate.model_id === fb)
    : undefined;
  const degraded = plan.evaluated.filter((e) => e.degradations.length > 0).slice(0, 6);
  return (
    <figure className="figure chart fallback-panel">
      <p className="svg-kicker" style={{ margin: "0 0 12px" }}>
        FIG. 8  ·  DEGRADATION / FALLBACK  ·  MODELED
      </p>
      {rec ? (
        <ol className="fallback-chain">
          <li>
            <strong>{rec.estimate.route_key}</strong>
            <span>
              recommended · p99 {formatMs(rec.estimate.p99_ms)} · quality {formatQuality(rec.estimate.quality)} ·{" "}
              {formatUsd(rec.estimate.cost_per_request)}/req
            </span>
          </li>
          {fb ? (
            <li>
              <strong>fallback_to {fb}</strong>
              <span>
                activation probability {formatPct(rec.estimate.fallback_activation_prob, 2)}
                {fallbackRoute ? ` · modeled p99 ${formatMs(fallbackRoute.estimate.p99_ms)}` : ""} · not an empirical
                failover rate
              </span>
            </li>
          ) : (
            <li>
              <strong>no model fallback configured</strong>
              <span>exhaustion policy remains on the workload</span>
            </li>
          )}
        </ol>
      ) : (
        <p className="note">No recommended route. Infeasible set — see violations in the table.</p>
      )}
      {degraded.length > 0 ? (
        <ul className="degrade-list">
          {degraded.map((e) => (
            <li key={e.estimate.route_key}>
              <code>{e.estimate.route_key}</code>
              <span>{e.degradations.map(degradationLabel).join(" · ") || "baseline"}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <figcaption>
        Degradation expands extra routes (disable rerank, reduce k/context, alter batching, fallback model). Constraints
        are never relaxed. Actions shown are from the live plan.
      </figcaption>
    </figure>
  );
}

