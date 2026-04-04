pub mod chat;
pub mod models;
pub mod mesh;

use axum::{extract::State, response::IntoResponse, Json};
use std::sync::Arc;
use crate::AppState;

/// GET /health
pub async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let llama_ok = state.mesh.check_llama_health().await;
    Json(serde_json::json!({
        "status": if llama_ok { "ok" } else { "degraded" },
        "llama_server": if llama_ok { "up" } else { "down" },
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
