# Workbench

The TypeScript/React workbench is the operator surface for the control
plane. It is not a second routing engine.

## What it does

- Loads the five built-in scenarios from `GET /v1/scenarios`
- Patches traffic, SLO, retrieval, batching, and degraded-capacity fields
  on the scenario JSON (the same body Python uses)
- Evaluates via `POST /v1/evaluate` with `allow_infeasible: true`
- Replays the recommended model through `POST /v1/simulate` (seeded, 2s)
- Charts Pareto, latency schema, cost/quality, throughput/utilization,
  candidate comparison, sensitivity, and degradation/fallback
- Renders curated Python artifacts from `web/public/artifacts/`
  (copies of `python/examples/artifacts/`)
- Loads `micp_wasm.wasm` for the inventory score helper

## What it does not do

- Rank engine routes in JavaScript
- Treat modeled or simulated numbers as empirical
- Invent a plan when `micp-api` is unreachable

If the API is down, the UI shows **engine unreachable**. Architecture,
origin legend, bundled artifacts, and the inventory-score helper remain
visible. They are labeled.

## Origins in the UI

| Badge | Source |
| --- | --- |
| MODELED | `/v1/evaluate` plan rows |
| SIMULATED | `/v1/simulate` strip and latency ticks |
| LOCAL SYNTHETIC BENCHMARK | WASM `micp_score_candidate` or TS `scoreCandidate` |
| EMPIRICAL | Legend only; unused |

## Development

Vite (`web/`) listens on `127.0.0.1:5173` and proxies `/health` and
`/v1` to `MICP_API_ORIGIN` (default `http://127.0.0.1:8081`).

Production-style single process: build `web/dist`, run `micp-api` with
that directory present (or `MICP_WEB_DIST`). Axum serves the SPA as a
fallback; API routes keep precedence.

## Visual language

Ivory / graphite editorial surface. Newsreader + Source Sans 3 + IBM
Plex Mono. Thin rules, figure captions, no dashboard chrome.
