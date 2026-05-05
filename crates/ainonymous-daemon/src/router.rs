use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tower_http::trace::TraceLayer;

use crate::{conductor::Conductor, holochain::HolochainClient};

#[derive(Clone)]
struct DaemonState {
    conductor: Arc<Conductor>,
    holochain: HolochainClient,
}

pub fn build(conductor: Arc<Conductor>, holochain: HolochainClient) -> Router {
    let state = DaemonState { conductor, holochain };

    Router::new()
        // Endpoints pour le proxy ainonymous-proxy
        .route("/mesh/status", get(mesh_status))
        .route("/mesh/nodes", get(mesh_nodes))
        .route("/mesh/plan", post(mesh_plan))
        .route("/mesh/metrics", post(mesh_metrics))
        .route("/mesh/blackboard/post", post(blackboard_post))
        .route("/mesh/blackboard/search", post(blackboard_search))

        // Endpoints internes (zome calls via daemon)
        .route("/zome/:dna/:zome/:function", post(zome_call))

        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn mesh_status(State(s): State<DaemonState>) -> impl IntoResponse {
    match s.holochain.get_execution_plan("").await {
        Ok(_) => Json(serde_json::json!({
            "local_node": { "status": "active" },
            "mesh": { "status": "connected" }
        })).into_response(),
        Err(_) => Json(serde_json::json!({
            "local_node": { "status": "degraded" },
            "mesh": { "status": "connecting" }
        })).into_response(),
    }
}

#[derive(Deserialize)]
struct ModelQuery { model_id: Option<String> }

async fn mesh_nodes(
    State(s): State<DaemonState>,
    Query(q): Query<ModelQuery>,
) -> impl IntoResponse {
    let model_id = q.model_id.as_deref().unwrap_or("");
    match s.holochain.get_available_nodes(model_id).await {
        Ok(nodes) => Json(nodes).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn mesh_plan(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let model_id = body["model_id"].as_str().unwrap_or("");
    match s.holochain.get_execution_plan(model_id).await {
        Ok(plan) => Json(plan).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn mesh_metrics(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // TODO: publier les métriques via zome call Holochain
    StatusCode::OK
}

async fn blackboard_post(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let prefix = body["prefix"].as_str().unwrap_or("STATUS");
    let content = body["content"].as_str().unwrap_or("");
    let tags = body["tags"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    match s.holochain.blackboard_post(prefix, content, tags).await {
        Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({"status": "posted"}))).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn blackboard_search(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let terms = body["terms"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let prefix_filter = body["prefix_filter"].as_str().map(String::from);

    match s.holochain.blackboard_search(terms, prefix_filter).await {
        Ok(results) => Json(results).into_response(),
        Err(_) => Json(serde_json::json!({"posts": [], "total": 0})).into_response(),
    }
}

/// Route générique pour les zome calls (délégation vers Holochain)
async fn zome_call(
    State(s): State<DaemonState>,
    Path((dna, zome, function)): Path<(String, String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match s.holochain.zome_call(&dna, &zome, &function, payload).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}
