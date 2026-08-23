use hdi::prelude::*;

#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeCapabilities {
    pub vram_gb: f32,
    pub ram_gb: f32,
    pub gpu_vendor: String,            // "apple_silicon"|"nvidia"|"amd"|"cpu"
    pub compute_backends: Vec<String>, // ["metal","cuda","hip","vulkan","cpu"]
    pub loaded_models: Vec<String>,
    pub max_concurrent_requests: u8,
    pub network_bandwidth_mbps: Option<u32>,
    pub region_hint: Option<String>,
    pub quic_endpoint: Option<String>, // "ip:port"
    /// Separate Ed25519 transport public key, encoded as 64 lowercase hex
    /// characters and authored by the same Holochain agent.
    pub node_pubkey: String,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeHeartbeat {
    pub current_load: f32, // 0.0 à 1.0
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
    ModelToAgents,  // anchor "models/{id}" → agents capables
    RegionToAgents, // anchor "regions/{region}" → agents
    PathLinks,      // liens internes des anchors/paths (hdk anchor)
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::CreateEntry(OpEntry::CreateEntry { app_entry, .. }) => match app_entry {
            EntryTypes::NodeCapabilities(caps) => validate_capabilities(&caps),
            EntryTypes::NodeHeartbeat(hb) => validate_heartbeat(&hb),
        },
        FlatOp::Link(OpLink::CreateLink { link_type, action }) => {
            validate_create_link(link_type, &action)
        }
        FlatOp::Link(OpLink::DeleteLink {
            original_action,
            action,
            ..
        }) => {
            if action.author() != original_action.author() {
                Ok(ValidateCallbackResult::Invalid(
                    "only the link author may delete an agent-registry link".into(),
                ))
            } else {
                Ok(ValidateCallbackResult::Valid)
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_create_link(
    link_type: LinkTypes,
    action: &TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let author_address: AnyLinkableHash = action.author().clone().into();
    match link_type {
        LinkTypes::AgentToCapabilities | LinkTypes::AgentToHeartbeats => {
            if action.base_address != author_address {
                return Ok(ValidateCallbackResult::Invalid(
                    "agent record links must use the link author as their base".into(),
                ));
            }
            let Some(target_hash) = action.target_address.clone().into_action_hash() else {
                return Ok(ValidateCallbackResult::Invalid(
                    "agent record links must target an action".into(),
                ));
            };
            let target = must_get_valid_record(target_hash)?;
            if target.action().author() != action.author() {
                return Ok(ValidateCallbackResult::Invalid(
                    "agent record links must target an entry authored by the link author".into(),
                ));
            }
            Ok(ValidateCallbackResult::Valid)
        }
        LinkTypes::ModelToAgents | LinkTypes::RegionToAgents => {
            if action.target_address != author_address {
                Ok(ValidateCallbackResult::Invalid(
                    "discovery links may advertise only the link author".into(),
                ))
            } else {
                Ok(ValidateCallbackResult::Valid)
            }
        }
        LinkTypes::PathLinks => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_capabilities(caps: &NodeCapabilities) -> ExternResult<ValidateCallbackResult> {
    if caps.vram_gb < 0.0 || caps.vram_gb > 2048.0 {
        return Ok(ValidateCallbackResult::Invalid(
            "vram_gb hors plage [0, 2048]".into(),
        ));
    }
    if caps.ram_gb < 0.5 || caps.ram_gb > 4096.0 {
        return Ok(ValidateCallbackResult::Invalid(
            "ram_gb hors plage [0.5, 4096]".into(),
        ));
    }
    if caps.max_concurrent_requests == 0 || caps.max_concurrent_requests > 64 {
        return Ok(ValidateCallbackResult::Invalid(
            "max_concurrent_requests entre 1 et 64".into(),
        ));
    }
    if let Some(ref ep) = caps.quic_endpoint {
        if !ep.contains(':') {
            return Ok(ValidateCallbackResult::Invalid(
                "quic_endpoint format invalide (ip:port)".into(),
            ));
        }
    }
    if caps.node_pubkey.len() != 64
        || !caps
            .node_pubkey
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(ValidateCallbackResult::Invalid(
            "node_pubkey must be a 32-byte Ed25519 public key encoded as hex".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_heartbeat(hb: &NodeHeartbeat) -> ExternResult<ValidateCallbackResult> {
    if hb.current_load < 0.0 || hb.current_load > 1.0 {
        return Ok(ValidateCallbackResult::Invalid(
            "current_load doit être entre 0.0 et 1.0".into(),
        ));
    }
    if hb.memory_pressure < 0.0 || hb.memory_pressure > 1.0 {
        return Ok(ValidateCallbackResult::Invalid(
            "memory_pressure doit être entre 0.0 et 1.0".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
