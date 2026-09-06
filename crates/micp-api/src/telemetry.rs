//! Tracing initialization.
//!
//! This is the attach point for OpenTelemetry. The current revision
//! installs a `tracing-subscriber` fmt layer so request logs exist
//! from day one. OTLP exporters, W3C trace-context extractors, and
//! metrics (decision latency, feasible-set size, SLO burn) belong
//! here once the request path is stable — adding them now would pull
//! a large crate graph for no recorded spans.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}
