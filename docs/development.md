# Development and reproducibility

## Toolchain pins

| Tool | Pin | Where |
| --- | --- | --- |
| Rust | `stable`, rust-version `1.80` | `rust-toolchain.toml`, `Cargo.toml` |
| Edition | 2021 | workspace package |
| Python | 3.10+ | `python/pyproject.toml` |
| Node | 20+ | `web/package.json` `engines` |
| WASM target | `wasm32-unknown-unknown` | optional; documented below |

`Cargo.lock` is committed so API and crate builds are reproducible
across machines. The Python package pins runtime deps in
`pyproject.toml`; install into a venv. The workbench commits
`package-lock.json`.

## Rust

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p micp-api --release
cargo bench -p micp-core --bench engine -- --quick
```

The API reads `configs/default.toml` by default. Override with:

```bash
MICP_CONFIG=/path/to/config.toml MICP_BIND=127.0.0.1:8080 cargo run -p micp-api
```

## Python

```bash
python3 -m venv python/.venv
python/.venv/bin/pip install -U pip
python/.venv/bin/pip install -e "python[dev]"
python/.venv/bin/pytest python/tests -q
python/.venv/bin/ruff check python/src python/tests
python/.venv/bin/ruff format --check python/src python/tests
python/.venv/bin/mypy --config-file python/pyproject.toml -p micp_eval
```

The package is importable as `micp_eval`. Runtime deps are NumPy and
pandas (tabular artifacts). Ruff, mypy, pytest, and Hypothesis are
dev extras. Integration tests spawn `target/release/micp-api` when
that binary exists and skip otherwise.

CLI (requires a running API):

```bash
python/.venv/bin/micp-eval evaluate --all --out python/artifacts
python/.venv/bin/micp-eval --help
```

See [docs/evaluation.md](evaluation.md).


## Workbench

```bash
cd web
npm ci        # or npm install on a fresh checkout without lock
npm test
npm run typecheck
npm run build
npm run dev   # Vite, http://127.0.0.1:5173
```

## WebAssembly

Requires the rustup target (not a separate SDK):

```bash
rustup target add wasm32-unknown-unknown
cargo test -p micp-wasm
cargo build -p micp-wasm --target wasm32-unknown-unknown --release
# artifact: target/wasm32-unknown-unknown/release/micp_wasm.wasm
```

The module exports `micp_score_candidate`, `micp_slo_burn_rate`,
`micp_modeled_p99_ms`, and `micp_slo_violation_prob`.
Both the original inventory score and the closed-form p99 path are
`f64` C ABI. `wasm-bindgen` / `wasm-pack` are **not** part of the
reproducible path.

If the target cannot be installed (air-gapped CI, missing rustup),
skip the WASM job; native `cargo test -p micp-wasm` still exercises
the same functions.

## Docker

The image builds only the API binary:

```bash
docker build -f docker/Dockerfile -t micp-api:local .
docker run --rm -p 8080:8080 micp-api:local
```

No compose file: a single process does not need one.

## CI

`.github/workflows/ci.yml` runs three jobs:

1. **rust** — fmt, clippy, `cargo test --workspace`, release build of `micp-api`
2. **python** — install `python[dev]`, ruff, mypy, pytest
3. **web** — `npm ci`, typecheck, unit tests, production build
4. **wasm** — install `wasm32-unknown-unknown`, build `micp-wasm`

Jobs are independent. A WASM toolchain failure must not hide a Rust
or Python regression.

## What not to add yet

- OpenTelemetry exporters and collector sidecars
- Kubernetes manifests / Helm
- A second HTTP framework in Python
- `wasm-pack` just to produce JS shims
- Database or auth layers (the control plane is currently stateless
  aside from in-memory SLO windows)
