/// Types compatibles OpenAI Chat Completions API
use serde::{Deserialize, Serialize};
use crate::inference::{ChatMessage, FinishReason, MessageRole, MessageContent};

// ─── Requêtes ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default)]
    pub stream: bool,
    pub stop: Option<serde_json::Value>,
    pub user: Option<String>,
    /// Extensions AInonymous
    pub ainonymous: Option<crate::inference::AInonymousOptions>,
}

fn default_max_tokens() -> u32 { 2048 }
fn default_temperature() -> f32 { 0.7 }
fn default_top_p() -> f32 { 0.9 }

#[derive(Debug, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

// ─── Réponses ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: UsageStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ainonymous: Option<AInonymousMeta>,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: AssistantMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssistantMessage {
    pub role: &'static str,
    pub content: String,
}

#[derive(Debug, Serialize, Default)]
pub struct UsageStats {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Métadonnées AInonymous dans la réponse
#[derive(Debug, Serialize)]
pub struct AInonymousMeta {
    pub execution_mode: String,
    pub nodes_used: u8,
    pub node_ids: Vec<String>,
    pub total_latency_ms: u32,
    pub tokens_per_second: f32,
    pub speculative_acceptance_rate: Option<f32>,
}

// ─── Streaming ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: DeltaContent,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct DeltaContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl ChatCompletionChunk {
    pub fn first(id: &str, model: &str) -> Self {
        Self {
            id: id.to_string(),
            object: "chat.completion.chunk",
            created: chrono::Utc::now().timestamp(),
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: DeltaContent { role: Some("assistant"), content: None },
                finish_reason: None,
            }],
        }
    }

    pub fn token(id: &str, model: &str, text: &str) -> Self {
        Self {
            id: id.to_string(),
            object: "chat.completion.chunk",
            created: chrono::Utc::now().timestamp(),
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: DeltaContent { role: None, content: Some(text.to_string()) },
                finish_reason: None,
            }],
        }
    }

    pub fn done(id: &str, model: &str) -> Self {
        Self {
            id: id.to_string(),
            object: "chat.completion.chunk",
            created: chrono::Utc::now().timestamp(),
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: DeltaContent::default(),
                finish_reason: Some("stop".to_string()),
            }],
        }
    }
}

// ─── Models endpoint ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: &'static str,
    pub data: Vec<ModelInfo>,
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub owned_by: String,
    pub meta: ModelMeta,
}

#[derive(Debug, Serialize)]
pub struct ModelMeta {
    pub vram_required_gb: f32,
    pub context_length: u32,
    pub multimodal: bool,
    pub architecture: String,
    pub nodes_available: u32,
    pub avg_latency_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_params_b: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speculative_draft: Option<bool>,
}

// ─── Mesh status endpoints ────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MeshStatus {
    pub local_node: LocalNodeStatus,
    pub mesh: MeshStats,
    pub blackboard: BlackboardStats,
}

#[derive(Debug, Serialize)]
pub struct LocalNodeStatus {
    pub agent_id: String,
    pub status: &'static str,
    pub vram_available_gb: f32,
    pub loaded_models: Vec<String>,
    pub current_load: f32,
    pub requests_handled_24h: u64,
}

#[derive(Debug, Serialize)]
pub struct MeshStats {
    pub peers_connected: u32,
    pub peers_active: u32,
    pub total_vram_gb: f32,
    pub requests_in_flight: u32,
    pub avg_latency_ms: u32,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct BlackboardStats {
    pub posts_last_24h: u64,
    pub agents_active: u32,
}

// ─── Erreurs API ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorDetail {
    pub message: String,
    pub r#type: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ainonymous: Option<serde_json::Value>,
}

impl ApiError {
    pub fn no_node(model: &str, available_vram: f32, suggested: Option<&str>) -> Self {
        let detail = serde_json::json!({
            "max_available_vram_gb": available_vram,
            "suggested_model": suggested,
        });
        Self {
            error: ApiErrorDetail {
                message: format!(
                    "Aucun nœud disponible pour {} (disponible: {:.1}GB max)",
                    model, available_vram
                ),
                r#type: "mesh_unavailable".into(),
                code: "NO_CAPABLE_NODE".into(),
                ainonymous: Some(detail),
            },
        }
    }

    pub fn invalid_model(model: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                message: format!("Modèle '{}' inconnu ou non disponible", model),
                r#type: "invalid_request_error".into(),
                code: "MODEL_NOT_FOUND".into(),
                ainonymous: None,
            },
        }
    }

    pub fn internal(msg: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                message: msg.to_string(),
                r#type: "internal_error".into(),
                code: "INTERNAL_ERROR".into(),
                ainonymous: None,
            },
        }
    }
}
