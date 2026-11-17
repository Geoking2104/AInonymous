use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::warn;

use ainonymous_types::api::*;
use crate::AppState;

/// GET /v1/ainonymous/mesh/status
pub async fn mesh_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.mesh.get_mesh_status().await {
        Ok(status) => Json(status).into_response(),
        Err(e) => {
            warn!("Mesh status unavailable: {}", e);
            // Retourner un statut dégradé
            let metrics = state.metrics.read().unwrap();
            let uptime = metrics.start_time.elapsed().as_secs();
            Json(serde_json::json!({
                "local_node": {
                    "agent_id": "initializing",
                    "status": "starting",
                    "current_load": 0.0,
                    "requests_handled_24h": metrics.requests_total,
                },
                "mesh": {
                    "peers_connected": 0,
                    "uptime_seconds": uptime,
                    "status": "connecting"
                }
            })).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct NodesQuery {
    pub model_id: Option<String>,
}

/// GET /v1/ainonymous/mesh/nodes?model_id=...
pub async fn mesh_nodes(
    State(state): State<Arc<AppState>>,
    Query(q): Query<NodesQuery>,
) -> impl IntoResponse {
    // Seuls les modèles connus du proxy sont relayés au daemon.
    let model_id = match q.model_id.as_deref().filter(|m| !m.is_empty()) {
        None => "",
        Some(m) => match crate::state::known_model_id(m) {
            Some(known) => known,
            None => {
                return (StatusCode::NOT_FOUND,
                        Json(ApiError::invalid_model(m))).into_response();
            }
        },
    };
    match state.mesh.get_available_nodes(model_id).await {
        Ok(nodes) => Json(serde_json::json!({ "nodes": nodes })).into_response(),
        Err(e) => {
            warn!("Récupération des nœuds échouée: {}", e);
            (StatusCode::SERVICE_UNAVAILABLE,
             Json(ApiError::internal("Mesh Holochain non disponible"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct BlackboardPostBody {
    pub prefix: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub ttl_hours: Option<u32>,
}

/// POST /v1/ainonymous/blackboard/post
pub async fn blackboard_post(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BlackboardPostBody>,
) -> impl IntoResponse {
    let payload = serde_json::json!({
        "prefix": body.prefix,
        "content": body.content,
        "tags": body.tags.unwrap_or_default(),
        "ttl_hours": body.ttl_hours.unwrap_or(48),
    });

    match state.mesh.http()
        .post(crate::mesh_client::daemon_endpoint("mesh/blackboard/post"))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.json::<serde_json::Value>().await.unwrap_or_default();
            Json(body).into_response()
        }
        _ => {
            (StatusCode::SERVICE_UNAVAILABLE,
             Json(ApiError::internal("Blackboard non disponible"))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct BlackboardSearchQuery {
    pub q: String,
    pub prefix: Option<String>,
    pub limit: Option<u32>,
}

/// GET /v1/ainonymous/blackboard/search?q=...
pub async fn blackboard_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BlackboardSearchQuery>,
) -> impl IntoResponse {
    let terms: Vec<&str> = params.q.split_whitespace().collect();

    match state.mesh.http()
        .post(crate::mesh_client::daemon_endpoint("mesh/blackboard/search"))
        .json(&serde_json::json!({
            "terms": terms,
            "prefix_filter": params.prefix,
            "limit": params.limit.unwrap_or(20),
        }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.json::<serde_json::Value>().await.unwrap_or_default();
            Json(body).into_response()
        }
        _ => {
            Json(serde_json::json!({ "posts": [], "total": 0 })).into_response()
        }
    }
}
