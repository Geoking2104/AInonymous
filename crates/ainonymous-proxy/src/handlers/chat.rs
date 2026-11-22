use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};
use uuid::Uuid;

use ainonymous_types::api::*;
use ainonymous_types::{ExecutionPlan, InferenceMetrics};
use crate::AppState;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    let start = Instant::now();
    let request_id = format!("chatcmpl-ainon-{}", Uuid::new_v4().simple());
    let model_id = req.model.clone();

    debug!("Requête chat/{} stream={}", model_id, req.stream);

    if req.stream {
        super::streaming::response(state, req, request_id).into_response()
    } else {
        handle_blocking(state, req, request_id, start).await.into_response()
    }
}

async fn handle_blocking(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
    request_id: String,
    start: Instant,
) -> Response {
    // Routage Phase 2 (chaîne) : si le plan d'exécution est multi-nœuds,
    // passer par le mesh distribué. Sinon (ou si le plan est indisponible),
    // exécution solo via llama-server local.
    if let Ok(plan) = state.mesh.get_execution_plan(&req.model).await {
        if matches!(plan, ExecutionPlan::PipelineSplit { .. }) {
            return mesh_pipeline_response(&state, &req, request_id, start).await;
        }
    }

    // Construire la requête pour llama-server
    let llama_req = build_llama_request(&req, false);

    match state.mesh.llama_chat(&llama_req).await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Value>().await {
                Ok(llama_resp) => {
                    let latency = start.elapsed().as_millis() as u32;
                    let prompt_tokens = llama_resp["usage"]["prompt_tokens"]
                        .as_u64().unwrap_or(0) as u32;
                    let completion_tokens = llama_resp["usage"]["completion_tokens"]
                        .as_u64().unwrap_or(0) as u32;
                    let content = llama_resp["choices"][0]["message"]["content"]
                        .as_str().unwrap_or("").to_string();

                    state.record_request(true, latency, completion_tokens);

                    let tps = if latency > 0 {
                        completion_tokens as f32 / (latency as f32 / 1000.0)
                    } else { 0.0 };

                    spawn_publish_metrics(state.clone(), InferenceMetrics {
                        request_id: Uuid::new_v4(),
                        model_id: req.model.clone(),
                        total_latency_ms: latency,
                        tokens_per_second: tps,
                        prompt_tokens,
                        completion_tokens,
                        nodes_used: 1,
                        execution_mode: "solo".into(),
                        success: true,
                        error_reason: None,
                    });

                    info!("✓ {} — {}ms — {:.1} tok/s", req.model, latency, tps);

                    let response = ChatCompletionResponse {
                        id: request_id,
                        object: "chat.completion",
                        created: chrono::Utc::now().timestamp(),
                        model: req.model.clone(),
                        choices: vec![ChatChoice {
                            index: 0,
                            message: AssistantMessage { role: "assistant", content },
                            finish_reason: Some("stop".into()),
                        }],
                        usage: UsageStats { prompt_tokens, completion_tokens,
                            total_tokens: prompt_tokens + completion_tokens },
                        ainonymous: Some(AInonymousMeta {
                            execution_mode: "solo".into(),
                            nodes_used: 1,
                            node_ids: vec!["local".into()],
                            total_latency_ms: latency,
                            tokens_per_second: tps,
                            speculative_acceptance_rate: None,
                        }),
                    };
                    (StatusCode::OK, Json(response)).into_response()
                }
                Err(e) => {
                    error!("Parse réponse llama-server: {}", e);
                    let err = ApiError::internal(&e.to_string());
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!("llama-server erreur {}: {}", status, body);
            let err = ApiError::internal(&format!("llama-server: {}", body));
            (StatusCode::BAD_GATEWAY, Json(err)).into_response()
        }
        Err(e) => {
            error!("Connexion llama-server: {}", e);
            let err = ApiError::internal("llama-server inaccessible");
            (StatusCode::SERVICE_UNAVAILABLE, Json(err)).into_response()
        }
    }
}

/// Réponse d'inférence distribuée (pipeline-split) via le daemon mesh.
async fn mesh_pipeline_response(
    state: &Arc<AppState>,
    req: &ChatCompletionRequest,
    request_id: String,
    start: Instant,
) -> Response {
    let messages = serde_json::to_value(&req.messages)
        .unwrap_or_else(|_| serde_json::json!([]));

    match state.mesh.run_mesh_inference(&req.model, &messages, req.max_tokens).await {
        Ok(v) => {
            let content = v["content"].as_str().unwrap_or("").to_string();
            let completion_tokens = v["token_count"].as_u64().unwrap_or(0) as u32;
            let node_ids: Vec<String> = v["node_ids"].as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let nodes_used = node_ids.len().max(1) as u8;
            let latency = start.elapsed().as_millis() as u32;
            state.record_request(true, latency, completion_tokens);
            let tps = if latency > 0 {
                completion_tokens as f32 / (latency as f32 / 1000.0)
            } else { 0.0 };

            info!("✓ {} (pipeline_split, {} nœuds) — {}ms", req.model, nodes_used, latency);

            // prompt_tokens : le daemon (/mesh/infer) ne le renvoie pas ; 0 comme
            // dans `usage` ci-dessous, en attendant qu'il soit exposé.
            spawn_publish_metrics(state.clone(), InferenceMetrics {
                request_id: Uuid::new_v4(),
                model_id: req.model.clone(),
                total_latency_ms: latency,
                tokens_per_second: tps,
                prompt_tokens: 0,
                completion_tokens,
                nodes_used,
                execution_mode: "pipeline_split".into(),
                success: true,
                error_reason: None,
            });

            let response = ChatCompletionResponse {
                id: request_id,
                object: "chat.completion",
                created: chrono::Utc::now().timestamp(),
                model: req.model.clone(),
                choices: vec![ChatChoice {
                    index: 0,
                    message: AssistantMessage { role: "assistant", content },
                    finish_reason: Some("stop".into()),
                }],
                usage: UsageStats {
                    prompt_tokens: 0,
                    completion_tokens,
                    total_tokens: completion_tokens,
                },
                ainonymous: Some(AInonymousMeta {
                    execution_mode: "pipeline_split".into(),
                    nodes_used,
                    node_ids,
                    total_latency_ms: latency,
                    tokens_per_second: tps,
                    speculative_acceptance_rate: None,
                }),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Inférence mesh échouée: {}", e);
            let err = ApiError::internal(&e.to_string());
            (StatusCode::BAD_GATEWAY, Json(err)).into_response()
        }
    }
}

/// POST /v1/completions (text completion legacy)
pub async fn completions(
    State(_state): State<Arc<AppState>>,
    Json(_req): Json<Value>,
) -> impl IntoResponse {
    // Déléguer directement à llama-server
    (StatusCode::NOT_IMPLEMENTED,
     Json(serde_json::json!({"error": "completions non implémenté, utiliser chat/completions"})))
}

/// POST /v1/embeddings
pub async fn embeddings(
    State(_state): State<Arc<AppState>>,
    Json(_req): Json<Value>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(serde_json::json!({"error": "embeddings non implémenté dans cette version"})))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

pub(super) fn build_llama_request(req: &ChatCompletionRequest, stream: bool) -> Value {
    serde_json::json!({
        "model": req.model,
        "messages": req.messages,
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "top_p": req.top_p,
        "stream": stream,
        "stop": req.stop,
    })
}

/// Publie en arrière-plan les métriques mesurées d'une requête vers le daemon
/// (`POST /mesh/metrics` → zome `publish_metrics`), sans retarder la réponse.
fn spawn_publish_metrics(state: Arc<AppState>, metrics: InferenceMetrics) {
    tokio::spawn(async move {
        state.mesh.publish_metrics(&metrics).await;
    });
}
