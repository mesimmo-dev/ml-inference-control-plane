//! Control-plane HTTP service.

mod config;
mod telemetry;

pub use config::{FileConfig, DEFAULT_CONFIG};
pub use telemetry::init as init_telemetry;

use std::sync::{Arc, Mutex};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use micp_core::{MicpError, ModelProfile, ObjectiveWeights, RequestConstraints, RoutingDecision};
use micp_optimizer::{allocate, Allocation};
use micp_policy::PolicySet;
use micp_router::route_with_policy;
use micp_slo::{SloSpec, SloState};
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub struct AppState {
    pub fleet: Vec<ModelProfile>,
    pub weights: ObjectiveWeights,
    pub policies: PolicySet,
    pub slo: Arc<Mutex<SloState>>,
    pub min_error_budget: f64,
    pub bind: String,
}

impl AppState {
    pub fn from_config(cfg: FileConfig) -> Self {
        let slo = SloState::new(SloSpec {
            name: cfg.slo.name,
            success_target: cfg.slo.success_target,
            latency_ms: cfg.slo.latency_ms,
        });
        Self {
            fleet: cfg.models,
            weights: cfg.weights,
            policies: PolicySet {
                policies: cfg.policies,
            },
            slo: Arc::new(Mutex::new(slo)),
            min_error_budget: cfg.slo.min_error_budget,
            bind: cfg.server.bind,
        }
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/config", get(show_config))
        .route("/v1/route", post(do_route))
        .route("/v1/allocate", post(do_allocate))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthBody> {
    Json(HealthBody {
        status: "ok",
        service: "micp-api",
    })
}

#[derive(Serialize)]
struct ConfigBody {
    bind: String,
    models: usize,
    policies: usize,
    weights: ObjectiveWeights,
    slo_name: String,
    error_budget_remaining: f64,
    admitting: bool,
}

async fn show_config(State(state): State<AppState>) -> Json<ConfigBody> {
    let slo = state.slo.lock().expect("slo lock");
    Json(ConfigBody {
        bind: state.bind.clone(),
        models: state.fleet.len(),
        policies: state.policies.policies.len(),
        weights: state.weights,
        slo_name: slo.spec.name.clone(),
        error_budget_remaining: slo.error_budget_remaining(),
        admitting: slo.admitting(state.min_error_budget),
    })
}

#[derive(Deserialize)]
struct RouteIn {
    #[serde(default = "RequestConstraints::interactive_default")]
    constraints: RequestConstraints,
    #[serde(default = "default_demand")]
    demand_rps: f64,
}

fn default_demand() -> f64 {
    10.0
}

async fn do_route(
    State(state): State<AppState>,
    Json(body): Json<RouteIn>,
) -> std::result::Result<Json<RoutingDecision>, ApiError> {
    let decision = route_with_policy(
        &state.fleet,
        &body.constraints,
        &state.weights,
        body.demand_rps,
        Some(&state.policies),
    )?;
    Ok(Json(decision))
}

#[derive(Deserialize)]
struct AllocateIn {
    #[serde(default = "RequestConstraints::interactive_default")]
    constraints: RequestConstraints,
    demand_rps: f64,
}

async fn do_allocate(
    State(state): State<AppState>,
    Json(body): Json<AllocateIn>,
) -> std::result::Result<Json<Allocation>, ApiError> {
    let allocation = allocate(
        &state.fleet,
        &body.constraints,
        &state.weights,
        body.demand_rps,
    )?;
    Ok(Json(allocation))
}

struct ApiError(MicpError);

impl From<MicpError> for ApiError {
    fn from(value: MicpError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            MicpError::NoFeasibleModel => StatusCode::CONFLICT,
            MicpError::InvalidConfig(_) => StatusCode::BAD_REQUEST,
        };
        let body = serde_json::json!({ "error": self.0.to_string() });
        (status, Json(body)).into_response()
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_telemetry();
    let (cfg, path) = FileConfig::load()?;
    tracing::info!(config = %path.display(), "loaded configuration");
    let mut state = AppState::from_config(cfg);
    if let Ok(bind) = std::env::var("MICP_BIND") {
        state.bind = bind;
    }
    let bind = state.bind.clone();
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "micp-api listening");
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    fn state() -> AppState {
        AppState::from_config(FileConfig::parse(DEFAULT_CONFIG).unwrap())
    }

    async fn json_get(uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app(state())
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn health_ok() {
        let (status, body) = json_get("/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn config_reports_bundled_fleet() {
        let (status, body) = json_get("/v1/config").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["models"], 3);
        assert_eq!(body["admitting"], true);
    }

    #[tokio::test]
    async fn route_returns_a_model() {
        let response = app(state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/route")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"demand_rps": 12}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["feasible"], true);
        assert!(body["model_id"].as_str().is_some());
    }
}
