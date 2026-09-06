import { useCallback, useEffect, useMemo, useState } from "react";
import { EngineHttpError, EngineUnreachableError, evaluate, getHealth, getScenario, listScenarios, simulate } from "../lib/api";
import { parseSensitivityCsv, transitions, type SensitivityRow } from "../lib/artifacts";
import { applyControls, controlsFromScenario, designRps } from "../lib/controls";
import type { RoutePlan, Scenario, ScenarioCard, SimReport, WorkbenchControls } from "../lib/engine";
import { ORIGIN_COPY } from "../lib/engine";
import {
  degradationLabel,
  formatMs,
  formatPct,
  formatProb,
  formatQuality,
  formatRps,
  formatUsd,
  kindLabel,
} from "../lib/format";
import { SAMPLE_FLEET, SAMPLE_WEIGHTS } from "../lib/types";
import { loadInventoryScorer, type InventoryScorer } from "../lib/wasm";
import { Charts } from "./Charts";

type EngineState = "checking" | "connected" | "unreachable";

export function Workbench() {
  const [engine, setEngine] = useState<EngineState>("checking");
  const [cards, setCards] = useState<ScenarioCard[]>([]);
  const [scenarioId, setScenarioId] = useState("interactive_assistant");
  const [baseline, setBaseline] = useState<Scenario | null>(null);
  const [controls, setControls] = useState<WorkbenchControls | null>(null);
  const [plan, setPlan] = useState<RoutePlan | null>(null);
  const [sim, setSim] = useState<SimReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scorer, setScorer] = useState<InventoryScorer | null>(null);
  const [sensitivity, setSensitivity] = useState<SensitivityRow[]>([]);
  const [artifactNote, setArtifactNote] = useState<string | null>(null);

  useEffect(() => {
    void loadInventoryScorer().then(setScorer);
    void fetch("/artifacts/sample_sensitivity.csv")
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error("missing artifact"))))
      .then((text) => setSensitivity(parseSensitivityCsv(text)))
      .catch(() => setSensitivity([]));
    void fetch("/artifacts/sample_evaluate.json")
      .then((r) => (r.ok ? r.json() : null))
      .then((j: { note?: string; result?: { recommended_key?: string } } | null) => {
        if (j?.note) setArtifactNote(`${j.note}${j.result?.recommended_key ? ` · recorded key ${j.result.recommended_key}` : ""}`);
      })
      .catch(() => setArtifactNote(null));
  }, []);

  const boot = useCallback(async () => {
    setEngine("checking");
    const ok = await getHealth();
    if (!ok) {
      setEngine("unreachable");
      setCards([]);
      setBaseline(null);
      setPlan(null);
      setSim(null);
      setError("micp-api is unreachable. The workbench will not invent a route plan.");
      return;
    }
    try {
      const listed = await listScenarios();
      setCards(listed);
      const id = listed.some((c) => c.id === scenarioId) ? scenarioId : listed[0]?.id;
      if (!id) {
        setEngine("connected");
        return;
      }
      const full = await getScenario(id);
      setScenarioId(id);
      setBaseline(full);
      setControls(controlsFromScenario(full));
      setEngine("connected");
      setError(null);
    } catch (err) {
      setEngine("unreachable");
      setError(err instanceof Error ? err.message : "failed to load scenarios");
    }
  }, [scenarioId]);

  useEffect(() => {
    void boot();
    // Initial boot only; scenario changes are handled by onSelect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onSelect = async (id: string) => {
    setScenarioId(id);
    if (engine !== "connected") return;
    try {
      const full = await getScenario(id);
      setBaseline(full);
      setControls(controlsFromScenario(full));
      setPlan(null);
      setSim(null);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed to load scenario");
    }
  };

  const patched = useMemo(() => {
    if (!baseline || !controls) return null;
    return applyControls(baseline, controls);
  }, [baseline, controls]);

  const run = useCallback(async () => {
    if (!patched || engine !== "connected") return;
    setBusy(true);
    setError(null);
    try {
      const next = await evaluate(patched, true);
      setPlan(next);
      const rec = next.recommended;
      if (rec) {
        try {
          const report = await simulate(patched, rec.estimate.model_id, { seed: 1, durationS: 2 });
          setSim(report);
        } catch {
          setSim(null);
        }
      } else {
        setSim(null);
      }
    } catch (err) {
      setPlan(null);
      setSim(null);
      if (err instanceof EngineUnreachableError) {
        setEngine("unreachable");
        setError("micp-api became unreachable during evaluate.");
      } else if (err instanceof EngineHttpError) {
        setError(err.message);
      } else {
        setError(err instanceof Error ? err.message : "evaluate failed");
      }
    } finally {
      setBusy(false);
    }
  }, [patched, engine]);

  useEffect(() => {
    if (!patched || engine !== "connected") return;
    const handle = window.setTimeout(() => {
      void run();
    }, 280);
    return () => window.clearTimeout(handle);
  }, [patched, engine, run]);

  const set = <K extends keyof WorkbenchControls>(key: K, value: WorkbenchControls[K]) => {
    setControls((prev) => (prev ? { ...prev, [key]: value } : prev));
  };

  const inventoryScores = scorer
    ? SAMPLE_FLEET.map((model) => ({
        model,
        score: scorer.score(model, SAMPLE_WEIGHTS, 10),
      })).sort((a, b) => b.score - a.score)
    : [];

  const rec = plan?.recommended;
  const feasible = new Set(plan?.feasible_keys ?? []);
  const pareto = new Set(plan?.pareto_keys ?? []);
  const trans = transitions(sensitivity);

  return (
    <section className="workbench" id="control-plane">
      <header className="section-head">
        <p className="kicker">Control plane</p>
        <h2>Interactive evaluation</h2>
        <p className="lede">
          Controls patch a scenario body and send it to <code>POST /v1/evaluate</code> with
          <code> allow_infeasible</code>. Recommended-route simulation uses{" "}
          <code>POST /v1/simulate</code>. Nothing in this panel is a production measurement.
        </p>
      </header>

      <EngineBanner state={engine} busy={busy} onRetry={() => void boot()} />

      <div className="workbench-grid">
        <aside className="panel controls">
          <h3>Scenario</h3>
          <label>
            Production scenario
            <select value={scenarioId} onChange={(e) => void onSelect(e.target.value)} disabled={engine !== "connected"}>
              {(cards.length ? cards : [{ id: scenarioId, name: scenarioId, summary: "", assumptions: [] }]).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
          {baseline ? <p className="hint">{baseline.summary}</p> : null}
          {baseline ? (
            <ul className="assumptions">
              {baseline.assumptions.map((a) => (
                <li key={a}>{a}</li>
              ))}
            </ul>
          ) : null}

          {controls ? (
            <>
              <h3>Traffic</h3>
              <Slider label="Request rate" unit="rps" min={1} max={200} step={1} value={controls.meanRps} onChange={(v) => set("meanRps", v)} />
              <Slider label="Concurrency" unit="clients" min={1} max={128} step={1} value={controls.concurrency} onChange={(v) => set("concurrency", v)} />
              <Slider label="Burst multiplier" unit="× mean" min={1} max={12} step={0.5} value={controls.burstMultiplier} onChange={(v) => set("burstMultiplier", v)} />
              <p className="hint">Design rate {formatRps(designRps(controls))} (mean, or mean × peak when bursty).</p>

              <h3>SLOs</h3>
              <Slider label="Latency SLO" unit="ms p99" min={20} max={2000} step={10} value={controls.latencySlo} onChange={(v) => set("latencySlo", v)} />
              <Slider label="Quality floor" unit="proxy" min={0.4} max={0.99} step={0.01} value={controls.qualityFloor} onChange={(v) => set("qualityFloor", v)} />
              <Slider label="Cost ceiling" unit="USD/req" min={0.0005} max={0.05} step={0.0005} value={controls.costCeiling} onChange={(v) => set("costCeiling", v)} />
              <Slider label="Reliability target" unit="1 − fail" min={0.9} max={0.999} step={0.001} value={controls.reliabilityTarget} onChange={(v) => set("reliabilityTarget", v)} />

              <h3>Context & retrieval</h3>
              <label>
                Retrieval strategy
                <select value={controls.retrieval} onChange={(e) => set("retrieval", e.target.value as WorkbenchControls["retrieval"])}>
                  <option value="none">none (closed book)</option>
                  <option value="sparse">sparse</option>
                  <option value="dense">dense</option>
                  <option value="hybrid">hybrid</option>
                </select>
              </label>
              <Slider label="top_k" unit="docs" min={0} max={32} step={1} value={controls.topK} onChange={(v) => set("topK", v)} />
              <label>
                Rerank
                <select value={controls.rerank} onChange={(e) => set("rerank", e.target.value as WorkbenchControls["rerank"])}>
                  <option value="none">none</option>
                  <option value="cross_encoder">cross encoder</option>
                  <option value="llm">llm</option>
                </select>
              </label>
              <Slider label="Context budget" unit="tokens" min={0} max={4096} step={64} value={controls.contextBudget} onChange={(v) => set("contextBudget", v)} />

              <h3>Batching & capacity</h3>
              <label>
                Batching policy
                <select value={controls.batching} onChange={(e) => set("batching", e.target.value as WorkbenchControls["batching"])}>
                  <option value="none">none</option>
                  <option value="window">window</option>
                </select>
              </label>
              {controls.batching === "window" ? (
                <>
                  <Slider label="Max batch" unit="req" min={1} max={32} step={1} value={controls.batchMax} onChange={(v) => set("batchMax", v)} />
                  <Slider label="Max wait" unit="ms" min={1} max={80} step={1} value={controls.batchWaitMs} onChange={(v) => set("batchWaitMs", v)} />
                </>
              ) : null}
              <Slider label="Degraded-capacity factor" unit="replicas" min={0} max={1} step={0.05} value={controls.degradedFactor} onChange={(v) => set("degradedFactor", v)} />
              <p className="hint">Applied to every candidate’s usable concurrency. 1.0 is healthy.</p>
            </>
          ) : (
            <p className="note">Load a scenario from the engine to edit constraints.</p>
          )}
        </aside>

        <div className="panel results">
          {error ? <p className="error">{error}</p> : null}

          {engine === "unreachable" ? (
            <div className="empty">
              <p>
                Engine unreachable. Modeled recommendations are not computed in the browser. Bundled Python artifacts
                and the inventory-score helper remain inspectable below.
              </p>
            </div>
          ) : null}

          {rec ? (
            <div className="reco">
              <p className="kicker">Recommended inference route</p>
              <h3>
                <code>{rec.estimate.route_key}</code>
              </h3>
              <p className="origin-row">
                <OriginBadge origin="modeled" />
                {sim ? <OriginBadge origin="simulated" /> : null}
              </p>
              <dl className="metrics">
                <Metric label="p50" value={formatMs(rec.estimate.p50_ms)} origin="MODELED" />
                <Metric label="p95" value={formatMs(rec.estimate.p95_ms)} origin="MODELED" />
                <Metric label="p99" value={formatMs(rec.estimate.p99_ms)} origin="MODELED" />
                <Metric label="throughput" value={formatRps(rec.estimate.throughput_rps)} origin="MODELED" />
                <Metric label="utilization" value={formatPct(rec.estimate.utilization)} origin="MODELED" />
                <Metric label="saturated" value={rec.estimate.saturated ? "yes" : "no"} origin="MODELED" />
                <Metric label="quality proxy" value={formatQuality(rec.estimate.quality)} origin="MODELED" />
                <Metric label="USD / request" value={formatUsd(rec.estimate.cost_per_request)} origin="MODELED" />
                <Metric label="USD / 1k req" value={formatUsd(rec.estimate.cost_per_request * 1000, 2)} origin="MODELED" />
                <Metric label="SLO-miss P" value={formatProb(rec.estimate.slo_violation_prob)} origin="MODELED" />
                <Metric label="failure P" value={formatProb(rec.estimate.failure_prob)} origin="MODELED" />
                <Metric label="fallback P" value={formatProb(rec.estimate.fallback_activation_prob)} origin="MODELED" />
              </dl>
              {sim ? (
                <div className="sim-strip">
                  <OriginBadge origin="simulated" />
                  <span>
                    n={sim.n_arrivals} served {sim.n_served} rejected {sim.n_rejected} · p50 {formatMs(sim.p50_ms)} · p95{" "}
                    {formatMs(sim.p95_ms)} · p99 {formatMs(sim.p99_ms)} · util {formatPct(sim.utilization)}
                  </span>
                </div>
              ) : null}
              {rec.degradations.length > 0 ? (
                <p className="hint">Degradations on this route: {rec.degradations.map(degradationLabel).join(", ")}.</p>
              ) : (
                <p className="hint">Baseline route — no degradation actions applied.</p>
              )}
            </div>
          ) : plan ? (
            <div className="reco infeasible">
              <p className="kicker">No feasible route</p>
              <h3>Empty feasible set</h3>
              <p className="note">
                Every expanded candidate violates at least one constraint. Reasons are listed per row. This is a modeled
                infeasibility, not an outage.
              </p>
            </div>
          ) : null}

          {plan ? (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Route</th>
                    <th>Set</th>
                    <th>p50 / p95 / p99</th>
                    <th>Thru</th>
                    <th>ρ</th>
                    <th>Quality</th>
                    <th>USD/req</th>
                    <th>SLO-miss</th>
                    <th>Fail P</th>
                    <th>Violations</th>
                  </tr>
                </thead>
                <tbody>
                  {plan.evaluated.map((e) => {
                    const key = e.estimate.route_key;
                    const tag = key === rec?.estimate.route_key ? "rec" : pareto.has(key) ? "pareto" : feasible.has(key) ? "feas" : "infeas";
                    return (
                      <tr key={key} data-tag={tag}>
                        <td>
                          <code>{key}</code>
                          <div className="tiny">{e.degradations.map(degradationLabel).join(" · ") || "baseline"}</div>
                        </td>
                        <td className="set-cell">
                          {tag === "rec" ? "recommended" : tag === "pareto" ? "Pareto" : tag === "feas" ? "feasible" : "infeasible"}
                        </td>
                        <td>
                          {formatMs(e.estimate.p50_ms, 0)} / {formatMs(e.estimate.p95_ms, 0)} / {formatMs(e.estimate.p99_ms, 0)}
                        </td>
                        <td>{formatRps(e.estimate.throughput_rps)}</td>
                        <td>
                          {formatPct(e.estimate.utilization)}
                          {e.estimate.saturated ? " sat" : ""}
                        </td>
                        <td>{formatQuality(e.estimate.quality)}</td>
                        <td>{formatUsd(e.estimate.cost_per_request)}</td>
                        <td>{formatProb(e.estimate.slo_violation_prob)}</td>
                        <td>{formatProb(e.estimate.failure_prob)}</td>
                        <td>
                          {e.violations.length === 0
                            ? "—"
                            : e.violations.map((v) => `${kindLabel(v.kind)} (obs ${v.observed.toPrecision(3)} / lim ${v.limit.toPrecision(3)})`).join("; ")}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
              <p className="hint">
                Origin of every row above: <strong>MODELED</strong>. Simulated numbers appear only in the strip under the
                recommendation.
              </p>
            </div>
          ) : null}

          <Charts plan={plan} sim={sim} sloMs={controls?.latencySlo ?? 200} sensitivity={sensitivity} />

          <section className="artifact-block">
            <h3>Python evaluation artifacts</h3>
            <p className="note">
              Curated <code>micp-eval.v1</code> fixtures bundled from{" "}
              <code>python/examples/artifacts</code>. Origin is <strong>MODELED</strong>. Not a live sweep and not
              empirical.
            </p>
            {artifactNote ? <p className="hint">{artifactNote}</p> : null}
            {trans.length > 0 ? (
              <ul className="transitions">
                {trans.map((t) => (
                  <li key={`${t.axis}-${t.toIndex}-${t.kind}`}>
                    <strong>{t.axis}</strong> {t.kind} at {t.atValue}: {t.fromKey} → {t.toKey}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="hint">No recorded transitions in the bundled CSV.</p>
            )}
          </section>

          <section className="inventory-block">
            <h3>Inventory score helper</h3>
            <p className="origin-row">
              <OriginBadge origin="local_synthetic" />
              <span className="hint">
                Source: {scorer?.source === "wasm" ? "WASM C ABI (micp_score_candidate)" : "TypeScript port of ModelProfile::score"}. Compact
                inventory rows, not the engine plan.
              </span>
            </p>
            <table>
              <thead>
                <tr>
                  <th>Inventory id</th>
                  <th>p99</th>
                  <th>Quality</th>
                  <th>Cost/1k tok</th>
                  <th>Cap rps</th>
                  <th>Score</th>
                </tr>
              </thead>
              <tbody>
                {inventoryScores.map(({ model, score }) => (
                  <tr key={model.id}>
                    <td>
                      <code>{model.id}</code>
                    </td>
                    <td>{model.latency_p99_ms}</td>
                    <td>{model.quality.toFixed(2)}</td>
                    <td>{model.cost_per_1k_tokens.toFixed(2)}</td>
                    <td>{model.capacity_rps}</td>
                    <td>{Number.isFinite(score) ? score.toFixed(4) : "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        </div>
      </div>
    </section>
  );
}

function EngineBanner({
  state,
  busy,
  onRetry,
}: {
  state: EngineState;
  busy: boolean;
  onRetry: () => void;
}) {
  const label =
    state === "connected" ? "micp-api connected · live modeled/simulated path" : state === "checking" ? "probing micp-api" : "engine unreachable";
  return (
    <div className="engine-banner" data-state={state}>
      <span>{label}{busy ? " · evaluating" : ""}</span>
      {state !== "connected" ? (
        <button type="button" onClick={onRetry}>
          Retry
        </button>
      ) : null}
    </div>
  );
}

function OriginBadge({ origin }: { origin: "modeled" | "simulated" | "local_synthetic" | "empirical" }) {
  const copy = ORIGIN_COPY[origin];
  return (
    <span className={`origin-badge origin-${origin}`} title={copy.note}>
      {copy.label}
    </span>
  );
}

function Metric({ label, value, origin }: { label: string; value: string; origin: string }) {
  return (
    <div>
      <dt>
        {label} <em>{origin}</em>
      </dt>
      <dd>{value}</dd>
    </div>
  );
}

function Slider({
  label,
  unit,
  min,
  max,
  step,
  value,
  onChange,
}: {
  label: string;
  unit: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="slider">
      <span>
        {label}
        <em>
          {value} {unit}
        </em>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}
