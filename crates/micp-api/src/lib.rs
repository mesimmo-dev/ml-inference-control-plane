//! Control-plane HTTP service.

mod config;
mod telemetry;

pub use config::{FileConfig, DEFAULT_CONFIG};
pub use telemetry::init as init_telemetry;

use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use micp_core::{
    all_scenarios, scenario_by_id, InferenceCandidate, MicpError, ModelProfile, ObjectiveWeights,
    RequestConstraints, RouteConfig, RoutingDecision, Scenario,
};
use micp_optimizer::{allocate, Allocation};
use micp_policy::PolicySet;
use micp_router::{plan, route_with_policy, RoutePlan};
use micp_sim::{simulate, SimReport, SimSpec};
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

    fn inventory_candidates(&self) -> Vec<InferenceCandidate> {
        self.fleet
            .iter()
            .map(InferenceCandidate::from_inventory)
            .collect()
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/config", get(show_config))
        .route("/v1/route", post(do_route))
        .route("/v1/allocate", post(do_allocate))
        .route("/v1/scenarios", get(list_scenarios))
        .route("/v1/scenarios/{id}", get(show_scenario))
        .route("/v1/evaluate", post(do_evaluate))
        .route("/v1/recommend", post(do_recommend))
        .route("/v1/simulate", post(do_simulate))
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

#[derive(Serialize)]
struct ScenarioCard {
    id: String,
    name: String,
    summary: String,
    assumptions: Vec<String>,
}

async fn list_scenarios() -> Json<Vec<ScenarioCard>> {
    Json(
        all_scenarios()
            .into_iter()
            .map(|s| ScenarioCard {
                id: s.id,
                name: s.name,
                summary: s.summary,
                assumptions: s.assumptions,
            })
            .collect(),
    )
}

async fn show_scenario(Path(id): Path<String>) -> std::result::Result<Json<Scenario>, ApiError> {
    scenario_by_id(&id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("unknown scenario {id}")))
}

#[derive(Deserialize)]
struct EvaluateIn {
    scenario_id: Option<String>,
}

fn resolve_scenario(state: &AppState, id: Option<&str>) -> std::result::Result<Scenario, ApiError> {
    match id {
        Some(id) => {
            scenario_by_id(id).ok_or_else(|| ApiError::not_found(format!("unknown scenario {id}")))
        }
        None => {
            let base = scenario_by_id("interactive_assistant").expect("builtin");
            Ok(Scenario {
                fleet: state.inventory_candidates(),
                ..base
            })
        }
    }
}

async fn do_evaluate(
    State(state): State<AppState>,
    Json(body): Json<EvaluateIn>,
) -> std::result::Result<Json<RoutePlan>, ApiError> {
    let s = resolve_scenario(&state, body.scenario_id.as_deref())?;
    Ok(Json(plan(&s.workload, &s.fleet, &s.weights)?))
}

#[derive(Serialize)]
struct RecommendBody {
    origin: &'static str,
    plan: RoutePlan,
}

async fn do_recommend(
    State(state): State<AppState>,
    Json(body): Json<EvaluateIn>,
) -> std::result::Result<Json<RecommendBody>, ApiError> {
    let s = resolve_scenario(&state, body.scenario_id.as_deref())?;
    Ok(Json(RecommendBody {
        origin: "modeled",
        plan: plan(&s.workload, &s.fleet, &s.weights)?,
    }))
}

#[derive(Deserialize)]
struct SimulateIn {
    scenario_id: String,
    model_id: Option<String>,
    seed: Option<u64>,
    duration_s: Option<f64>,
    long_tail: Option<bool>,
}

async fn do_simulate(
    Json(body): Json<SimulateIn>,
) -> std::result::Result<Json<SimReport>, ApiError> {
    let s = scenario_by_id(&body.scenario_id)
        .ok_or_else(|| ApiError::not_found(format!("unknown scenario {}", body.scenario_id)))?;
    let cand = match &body.model_id {
        Some(id) => s
            .fleet
            .iter()
            .find(|c| c.id.as_str() == id)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("unknown model {id}")))?,
        None => s.fleet[0].clone(),
    };
    let route = RouteConfig::baseline(cand, &s.workload);
    let spec = SimSpec {
        duration_s: body.duration_s.unwrap_or(2.0).min(10.0),
        seed: body.seed.unwrap_or(1),
        long_tail: body.long_tail.unwrap_or(false),
        max_events: 8_000,
        ..SimSpec::default()
    };
    Ok(Json(simulate(&route, &s.workload, &spec)?))
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message,
        }
    }
}

impl From<MicpError> for ApiError {
    fn from(value: MicpError) -> Self {
        let status = match value {
            MicpError::NoFeasibleModel => StatusCode::CONFLICT,
            MicpError::InvalidConfig(_) => StatusCode::BAD_REQUEST,
        };
        Self {
            status,
            message: value.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": self.message });
        (self.status, Json(body)).into_response()
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

    async fn json_post(uri: &str, body: &str) -> (StatusCode, serde_json::Value) {
        let response = app(state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, v)
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
        let (status, body) = json_post("/v1/route", r#"{"demand_rps": 12}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["feasible"], true);
        assert!(body["model_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn scenarios_list_five() {
        let (status, body) = json_get("/v1/scenarios").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn scenario_detail_includes_assumptions() {
        let (status, body) = json_get("/v1/scenarios/quality_rag").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "quality_rag");
        assert!(body["assumptions"].as_array().unwrap().len() >= 3);
        assert_eq!(body["fleet"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn unknown_scenario_is_404() {
        let (status, _) = json_get("/v1/scenarios/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn evaluate_interactive_returns_recommendation() {
        let (status, body) =
            json_post("/v1/evaluate", r#"{"scenario_id":"interactive_assistant"}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["recommended"]["estimate"]["route_key"]
            .as_str()
            .is_some());
        assert_eq!(body["recommended"]["estimate"]["origin"], "modeled");
    }

    #[tokio::test]
    async fn simulate_marks_origin_simulated() {
        let (status, body) = json_post(
            "/v1/simulate",
            r#"{"scenario_id":"interactive_assistant","model_id":"fast-8b","duration_s":2.0,"seed":3}"#,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["origin"], "simulated");
        assert!(body["n_arrivals"].as_u64().unwrap() > 0);
    }
}
