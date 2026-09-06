import { SAMPLE_FLEET, SAMPLE_WEIGHTS } from "./lib/types";
import { scoreCandidate } from "./lib/score";

const LAYERS = [
  { name: "micp-api", role: "Axum control plane — /health, /v1/route, /v1/allocate" },
  { name: "micp-policy", role: "Ordered constraint tightening; never relaxes bounds" },
  { name: "micp-slo", role: "Error budget and burn rate for admission" },
  { name: "micp-router", role: "Single-request feasible-set routing" },
  { name: "micp-optimizer", role: "Capacity-capped traffic-share allocation" },
  { name: "micp-sim", role: "Deterministic workload generation" },
  { name: "micp-wasm", role: "C-ABI scoring/SLO exports for this workbench" },
];

export function App() {
  const ranked = SAMPLE_FLEET.map((model) => ({
    model,
    score: scoreCandidate(model, SAMPLE_WEIGHTS, 10),
  })).sort((a, b) => b.score - a.score);

  return (
    <main className="page">
      <header className="mast">
        <p className="kicker">ML inference control plane</p>
        <h1>Systems workbench</h1>
        <p className="lede">
          Architecture shell for adaptive routing under latency, quality, cost,
          throughput, and reliability constraints. The table below is scored
          with the same formula as <code>micp-core</code>; WASM will replace
          the TypeScript port once the module is loaded.
        </p>
      </header>

      <section>
        <h2>Decision pipeline</h2>
        <ol className="layers">
          {LAYERS.map((layer) => (
            <li key={layer.name}>
              <code>{layer.name}</code>
              <span>{layer.role}</span>
            </li>
          ))}
        </ol>
      </section>

      <section>
        <h2>Sample fleet scores</h2>
        <p className="note">
          Weights: latency {SAMPLE_WEIGHTS.latency}, quality {SAMPLE_WEIGHTS.quality},
          cost {SAMPLE_WEIGHTS.cost}, throughput {SAMPLE_WEIGHTS.throughput},
          reliability {SAMPLE_WEIGHTS.reliability}. Demand 10 rps.
        </p>
        <table>
          <thead>
            <tr>
              <th>Model</th>
              <th>p99 (ms)</th>
              <th>Quality</th>
              <th>Cost / 1k</th>
              <th>Cap rps</th>
              <th>Error</th>
              <th>Score</th>
            </tr>
          </thead>
          <tbody>
            {ranked.map(({ model, score }) => (
              <tr key={model.id}>
                <td>
                  <code>{model.id}</code>
                </td>
                <td>{model.latency_p99_ms}</td>
                <td>{model.quality.toFixed(2)}</td>
                <td>{model.cost_per_1k_tokens.toFixed(2)}</td>
                <td>{model.capacity_rps}</td>
                <td>{model.error_rate.toFixed(3)}</td>
                <td>{score.toFixed(4)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </main>
  );
}
