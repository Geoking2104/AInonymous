use hdi::prelude::*;

#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeCapabilities {
    pub vram_gb: f32,
    pub ram_gb: f32,
    pub gpu_vendor: String,          // "apple_silicon"|"nvidia"|"amd"|"cpu"
    pub compute_backends: Vec<String>, // ["metal","cuda","hip","vulkan","cpu"]
    pub loaded_models: Vec<LoadedModelEntry>,
    pub max_concurrent_requests: u8,
    pub network_bandwidth_mbps: Option<u32>,
    pub region_hint: Option<String>,
    pub quic_endpoint: Option<String>, // "ip:port"
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoadedModelEntry {
    pub model_id: String,
    pub model_hash: String,           // SHA256 hex
    pub quantization: String,
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>,
    pub context_size: u32,
    pub ready: bool,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeHeartbeat {
    pub current_load: f32,            // 0.0 à 1.0
    pub available_slots: u8,
    pub queue_depth: u32,
    pub memory_pressure: f32,
    pub temperature_c: Option<f32>,
    pub timestamp_ms: i64,
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    NodeCapabilities(NodeCapabilities),
    NodeHeartbeat(NodeHeartbeat),
}

#[hdk_link_types]
pub enum LinkTypes {
    AgentToCapabilities,
    AgentToHeartbeats,
    ModelToAgents,       // anchor "models/{id}" → agents capables
    RegionToAgents,      // anchor "regions/{region}" → agents
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(OpEntry::CreateEntry { app_entry, .. }) => {
            match app_entry {
                EntryTypes::NodeCapabilities(caps) => validate_capabilities(&caps),
                EntryTypes::NodeHeartbeat(hb)      => validate_heartbeat(&hb),
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_capabilities(caps: &NodeCapabilities) -> ExternResult<ValidateCallbackResult> {
    if caps.vram_gb < 0.0 || caps.vram_gb > 2048.0 {
        return Ok(ValidateCallbackResult::Invalid("vram_gb hors plage [0, 2048]".into()));
    }
    if caps.ram_gb < 0.5 || caps.ram_gb > 4096.0 {
        return Ok(ValidateCallbackResult::Invalid("ram_gb hors plage [0.5, 4096]".into()));
    }
    if caps.max_concurrent_requests == 0 || caps.max_concurrent_requests > 64 {
        return Ok(ValidateCallbackResult::Invalid("max_concurrent_requests entre 1 et 64".into()));
    }
    if let Some(ref ep) = caps.quic_endpoint {
        if !ep.contains(':') {
            return Ok(ValidateCallbackResult::Invalid("quic_endpoint format invalide (ip:port)".into()));
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_heartbeat(hb: &NodeHeartbeat) -> ExternResult<ValidateCallbackResult> {
    if hb.current_load < 0.0 || hb.current_load > 1.0 {
        return Ok(ValidateCallbackResult::Invalid("current_load doit être entre 0.0 et 1.0".into()));
    }
    if hb.memory_pressure < 0.0 || hb.memory_pressure > 1.0 {
        return Ok(ValidateCallbackResult::Invalid("memory_pressure doit être entre 0.0 et 1.0".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
