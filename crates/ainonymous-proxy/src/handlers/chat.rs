use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    Json,
};
use axum::response::sse::Event;
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use ainonymous_types::api::*;
use ainonymous_types::inference::*;
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
        handle_streaming(state, req, request_id, start).await.into_response()
    } else {
        handle_blocking(state, req, request_id, start).await.into_response()
    }
}

async fn handle_blocking(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
    request_id: String,
    start: Instant,
) -> impl IntoResponse {
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

async fn handle_streaming(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
    request_id: String,
    start: Instant,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let llama_url = state.config.llama_server_url.clone();
    let model = req.model.clone();
    let llama_req = build_llama_request(&req, true);
    let id = request_id.clone();

    let stream = async_stream::stream! {
        // Événement initial : rôle assistant
        let first = ChatCompletionChunk::first(&id, &model);
        yield Ok(Event::default().data(serde_json::to_string(&first).unwrap()));

        // Appel llama-server en streaming
        let client = reqwest::Client::new();
        match client
            .post(format!("{}/v1/chat/completions", llama_url))
            .json(&llama_req)
            .send()
            .await
        {
            Ok(resp) => {
                use tokio_stream::StreamExt as _;
                let mut byte_stream = resp.bytes_stream();

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            // Parser les SSE de llama-server et les re-émettre
                            let text = String::from_utf8_lossy(&bytes);
                            for line in text.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data == "[DONE]" {
                                        break;
                                    }
                                    if let Ok(v) = serde_json::from_str::<Value>(data) {
                                        if let Some(content) = v["choices"][0]["delta"]["content"].as_str() {
                                            let chunk = ChatCompletionChunk::token(&id, &model, content);
                                            yield Ok(Event::default()
                                                .data(serde_json::to_string(&chunk).unwrap()));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Erreur stream: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Connexion llama-server stream: {}", e);
            }
        }

        // Chunk final
        let done = ChatCompletionChunk::done(&id, &model);
        yield Ok(Event::default().data(serde_json::to_string(&done).unwrap()));
        yield Ok(Event::default().data("[DONE]"));
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
    )
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

fn build_llama_request(req: &ChatCompletionRequest, stream: bool) -> Value {
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
