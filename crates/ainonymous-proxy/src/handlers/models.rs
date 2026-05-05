use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use serde::Deserialize;
use tracing::info;

use ainonymous_types::api::*;
use crate::state::ModelArchitecture;
use crate::AppState;

/// GET /v1/models
pub async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let models = state.local_models.read().unwrap();

    let data: Vec<ModelInfo> = models.values().map(|m| {
        let (nodes_available, avg_latency_ms) = (1u32, None::<u32>); // TODO: requête DHT

        ModelInfo {
            id: m.model_id.clone(),
            object: "model",
            created: 1735000000,
            owned_by: if m.ready { "ainonymous-local" } else { "ainonymous-mesh" }.into(),
            meta: ModelMeta {
                vram_required_gb: m.vram_gb,
                context_length: m.context_size,
                multimodal: m.multimodal,
                architecture: match &m.architecture {
                    ModelArchitecture::Dense => "dense".into(),
                    ModelArchitecture::MoE { .. } => "moe".into(),
                    ModelArchitecture::DenseEdge => "dense-edge".into(),
                },
                nodes_available,
                avg_latency_ms,
                active_params_b: match &m.architecture {
                    ModelArchitecture::MoE { active_params_b } => Some(*active_params_b),
                    _ => None,
                },
                speculative_draft: if m.model_id.contains("e4b") || m.model_id.contains("e2b") {
                    Some(true)
                } else { None },
            },
        }
    }).collect();

    Json(ModelsResponse { object: "list", data })
}

#[derive(Deserialize)]
pub struct PullModelRequest {
    pub model_id: String,
    pub quantization: Option<String>,
    pub source: Option<String>,
}

/// POST /v1/ainonymous/models/pull
pub async fn pull_model(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PullModelRequest>,
) -> impl IntoResponse {
    info!("Téléchargement modèle: {} (quant: {:?})", req.model_id, req.quantization);

    // TODO: déclencher le téléchargement via le daemon
    // Pour l'instant : retourner un job ID
    let job_id = uuid::Uuid::new_v4().to_string();

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "job_id": job_id,
        "model_id": req.model_id,
        "status": "downloading",
        "message": "Téléchargement démarré en arrière-plan"
    })))
}
