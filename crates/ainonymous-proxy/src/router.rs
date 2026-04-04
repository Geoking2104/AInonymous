use std::sync::Arc;
use axum::{
    routing::{get, post},
    Router,
};
use crate::{handlers, AppState};

pub fn build(state: Arc<AppState>) -> Router {
    Router::new()
        // ── OpenAI-compatible endpoints ───────────────────────────────────
        .route("/v1/models", get(handlers::models::list_models))
        .route("/v1/chat/completions", post(handlers::chat::chat_completions))
        .route("/v1/completions", post(handlers::chat::completions))
        .route("/v1/embeddings", post(handlers::chat::embeddings))

        // ── AInonymous native endpoints ───────────────────────────────────
        .route("/v1/ainonymous/mesh/status", get(handlers::mesh::mesh_status))
        .route("/v1/ainonymous/mesh/nodes", get(handlers::mesh::mesh_nodes))
        .route("/v1/ainonymous/blackboard/post", post(handlers::mesh::blackboard_post))
        .route("/v1/ainonymous/blackboard/search", get(handlers::mesh::blackboard_search))
        .route("/v1/ainonymous/models/pull", post(handlers::models::pull_model))

        // ── Healthcheck ───────────────────────────────────────────────────
        .route("/health", get(handlers::health))
        .route("/", get(handlers::health))

        .with_state(state)
}
