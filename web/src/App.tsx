import { Architecture } from "./components/Architecture";
import { Workbench } from "./components/Workbench";
import { ORIGIN_COPY } from "./lib/engine";

export function App() {
  return (
    <div className="page">
      <header className="mast">
        <p className="running-head">ML systems · routing under constraint · executable environment</p>
        <p className="kicker">ML inference control plane</p>
        <h1>ML Inference Control Plane</h1>
        <p className="subtitle">Adaptive Routing Under Production Constraints</p>
        <p className="lede standfirst">
          An executable ML systems environment for model selection under competing latency,
          quality, cost, throughput, and reliability objectives.
        </p>
      </header>

      <section className="abstract" id="abstract">
        <h2>Abstract</h2>
        <p>
          Serving stacks expose a fleet of candidates whose latency, quality, unit cost, and
          residual capacity cannot be optimized independently. A control plane in front of those
          runtimes must admit a request (or a share of traffic) onto a feasible route, record why
          the others failed, and keep the decision reproducible. This environment implements that
          loop: a Rust engine estimates closed-form M/M/n sojourns, expands degradation variants
          without relaxing SLOs, computes a four-objective Pareto set, and optionally replays the
          recommended route through a seeded G/G/n simulator. Python drives off-path sweeps. This
          workbench is the operator surface — it does not contain a second engine.
        </p>
        <p>
          Every quantity is tagged. <strong>Modeled</strong> numbers are the closed-form path.{" "}
          <strong>Simulated</strong> numbers are seeded discrete-event runs. The inventory score
          helper is a <strong>local synthetic benchmark</strong>. <strong>Empirical</strong> is
          reserved and unused. None of these are production traces.
        </p>
      </section>

      <Architecture />

      <section className="origin-legend" aria-label="Estimate origins">
        <h2>Estimate origins</h2>
        <ul>
          {(Object.keys(ORIGIN_COPY) as Array<keyof typeof ORIGIN_COPY>).map((key) => (
            <li key={key}>
              <span className={`origin-badge origin-${key}`}>{ORIGIN_COPY[key].label}</span>
              <span>{ORIGIN_COPY[key].note}</span>
            </li>
          ))}
        </ul>
      </section>

      <Workbench />

      <section className="limitations">
        <h2>Modeling limitations</h2>
        <ul>
          <li>Queueing is M/M/n (modeled) or seeded G/G/n (simulated), not a GPU runtime.</li>
          <li>Quality is a calibrated proxy, not a judged groundedness metric.</li>
          <li>Cost is list-price-like USD, not a negotiated bill.</li>
          <li>Burst estimates size to peak. Degraded capacity scales concurrency, not token speed.</li>
          <li>The TypeScript layer never independently ranks engine routes.</li>
        </ul>
        <p className="colophon">
          Polyglot repository: Rust (Tokio/Axum) · Python (micp_eval) · TypeScript/React · WASM C ABI.
          Apache-2.0.
        </p>
      </section>
    </div>
  );
}
