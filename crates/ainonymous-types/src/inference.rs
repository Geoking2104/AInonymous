use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Requête d'inférence interne (entre composants AInonymous)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: Uuid,
    pub model_id: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub stream: bool,
    pub stop: Option<Vec<String>>,
    pub ainonymous_opts: AInonymousOptions,
}

impl InferenceRequest {
    pub fn new(model_id: String, messages: Vec<ChatMessage>) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            model_id,
            messages,
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
            stop: None,
            ainonymous_opts: AInonymousOptions::default(),
        }
    }
}

/// Options spécifiques AInonymous dans une requête
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AInonymousOptions {
    pub execution_mode: Option<ExecutionModeHint>,
    pub min_nodes: Option<u8>,
    pub prefer_region: Option<String>,
    pub speculative_draft_model: Option<String>,
    pub blackboard_context: bool,
    pub queue: bool,
    pub queue_timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModeHint {
    Auto,
    Solo,
    Pipeline,
    ExpertShard,
    Speculative,
}

/// Message dans une conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Contenu d'un message (texte ou multimodal)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    pub detail: Option<String>,
}

/// Token généré (pour streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedToken {
    pub token_id: u32,
    pub text: String,
    pub logprob: Option<f32>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
}

/// Métriques d'une requête complète
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetrics {
    pub request_id: Uuid,
    pub model_id: String,
    pub total_latency_ms: u32,
    pub tokens_per_second: f32,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub nodes_used: u8,
    pub execution_mode: String,
    pub success: bool,
    pub error_reason: Option<String>,
}

/// Header binaire pour le transfert d'activations via QUIC
#[derive(Debug, Clone)]
pub struct ActivationHeader {
    pub request_id: [u8; 36],  // UUID string
    pub layer_start: u32,
    pub layer_end: u32,
    pub seq_len: u32,
    pub hidden_size: u32,
    pub dtype: DType,
    pub compressed: bool,
}

impl ActivationHeader {
    pub const SIZE: usize = 64;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..36].copy_from_slice(&self.request_id);
        buf[36..40].copy_from_slice(&self.layer_start.to_le_bytes());
        buf[40..44].copy_from_slice(&self.layer_end.to_le_bytes());
        buf[44..48].copy_from_slice(&self.seq_len.to_le_bytes());
        buf[48..52].copy_from_slice(&self.hidden_size.to_le_bytes());
        buf[52] = self.dtype as u8;
        buf[53] = self.compressed as u8;
        buf
    }

    pub fn from_bytes(buf: &[u8; Self::SIZE]) -> Self {
        let mut request_id = [0u8; 36];
        request_id.copy_from_slice(&buf[0..36]);
        Self {
            request_id,
            layer_start: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            layer_end: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            seq_len: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            hidden_size: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            dtype: DType::from(buf[52]),
            compressed: buf[53] != 0,
        }
    }

    pub fn tensor_size_bytes(&self) -> usize {
        self.seq_len as usize * self.hidden_size as usize * self.dtype.bytes_per_element()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum DType {
    F32 = 0,
    F16 = 1,
    Bf16 = 2,
}

impl DType {
    pub fn bytes_per_element(&self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F16 | Self::Bf16 => 2,
        }
    }
}

impl From<u8> for DType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::F32,
            1 => Self::F16,
            2 => Self::Bf16,
            _ => Self::Bf16,
        }
    }
}
