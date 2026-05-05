use hdi::prelude::*;

// ─── Types d'entrées ─────────────────────────────────────────────────────────

#[hdk_entry_helper]
#[derive(Clone)]
pub struct InferenceRequest {
    pub request_id: String,
    pub model_id: String,
    pub prompt_hash: Vec<u8>,        // SHA256 du prompt
    pub max_tokens: u32,
    pub temperature: f32,
    pub requester: AgentPubKey,
    pub timestamp: Timestamp,
    pub execution_mode: String,      // "solo"|"pipeline"|"expert_shard"|"speculative"
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct LayerChunk {
    pub request_id: String,
    pub node: AgentPubKey,
    pub chunk_index: u32,
    pub activations_hash: Vec<u8>,   // SHA256 des activations (jamais les activations elles-mêmes)
    pub latency_ms: u32,
    pub timestamp: Timestamp,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct InferenceMetrics {
    pub request_id: String,
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

/// Offre de session QUIC négociée via Holochain
#[hdk_entry_helper]
#[derive(Clone)]
pub struct QuicSessionOffer {
    pub request_id: String,
    pub requestor: AgentPubKey,
    pub quic_endpoint: String,       // "ip:port"
    pub session_token_hash: Vec<u8>, // SHA256 du token (jamais le token lui-même en DHT)
    pub expires_at: Timestamp,
    pub layer_range: Option<(u32, u32)>,
}

// ─── Types de liens ───────────────────────────────────────────────────────────

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    InferenceRequest(InferenceRequest),
    LayerChunk(LayerChunk),
    InferenceMetrics(InferenceMetrics),
    QuicSessionOffer(QuicSessionOffer),
}

#[hdk_link_types]
pub enum LinkTypes {
    RequestToChunks,
    RequestToMetrics,
    AgentToRequests,
    ModelToRequests,    // anchor "models/{model_id}" → requêtes
    AgentToSessions,    // agent → sessions QUIC actives
}

// ─── Validation ───────────────────────────────────────────────────────────────

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(OpEntry::CreateEntry { app_entry, .. }) => {
            match app_entry {
                EntryTypes::InferenceRequest(req) => validate_inference_request(&req),
                EntryTypes::LayerChunk(chunk)     => validate_layer_chunk(&chunk),
                EntryTypes::InferenceMetrics(m)   => validate_metrics(&m),
                EntryTypes::QuicSessionOffer(o)   => validate_quic_offer(&o),
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_inference_request(req: &InferenceRequest) -> ExternResult<ValidateCallbackResult> {
    if req.request_id.len() != 36 {
        return Ok(ValidateCallbackResult::Invalid("request_id doit être UUID v4 (36 chars)".into()));
    }
    if req.max_tokens == 0 || req.max_tokens > 131_072 {
        return Ok(ValidateCallbackResult::Invalid("max_tokens doit être entre 1 et 131072".into()));
    }
    if req.temperature < 0.0 || req.temperature > 4.0 {
        return Ok(ValidateCallbackResult::Invalid("temperature doit être entre 0.0 et 4.0".into()));
    }
    if req.prompt_hash.len() != 32 {
        return Ok(ValidateCallbackResult::Invalid("prompt_hash doit être SHA256 (32 bytes)".into()));
    }
    let valid_modes = ["solo", "pipeline", "expert_shard", "speculative"];
    if !valid_modes.contains(&req.execution_mode.as_str()) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "execution_mode '{}' invalide, valeurs: {:?}", req.execution_mode, valid_modes
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_layer_chunk(chunk: &LayerChunk) -> ExternResult<ValidateCallbackResult> {
    if chunk.activations_hash.len() != 32 {
        return Ok(ValidateCallbackResult::Invalid(
            "activations_hash doit être SHA256 (32 bytes)".into()
        ));
    }
    // latence max raisonnable : 10 minutes
    if chunk.latency_ms > 600_000 {
        return Ok(ValidateCallbackResult::Invalid("latency_ms > 10 minutes, invalide".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_metrics(m: &InferenceMetrics) -> ExternResult<ValidateCallbackResult> {
    if m.tokens_per_second < 0.0 || m.tokens_per_second > 100_000.0 {
        return Ok(ValidateCallbackResult::Invalid(
            "tokens_per_second hors plage plausible [0, 100000]".into()
        ));
    }
    if m.nodes_used == 0 || m.nodes_used > 32 {
        return Ok(ValidateCallbackResult::Invalid("nodes_used doit être entre 1 et 32".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_quic_offer(offer: &QuicSessionOffer) -> ExternResult<ValidateCallbackResult> {
    // Vérifier format ip:port basique
    if !offer.quic_endpoint.contains(':') {
        return Ok(ValidateCallbackResult::Invalid(
            "quic_endpoint doit être au format ip:port".into()
        ));
    }
    if offer.session_token_hash.len() != 32 {
        return Ok(ValidateCallbackResult::Invalid(
            "session_token_hash doit être SHA256 (32 bytes)".into()
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// ─── Réseau public : pas de membrane proof ───────────────────────────────────

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    // Réseau public — tout agent accepté sans condition
    Ok(ValidateCallbackResult::Valid)
}
