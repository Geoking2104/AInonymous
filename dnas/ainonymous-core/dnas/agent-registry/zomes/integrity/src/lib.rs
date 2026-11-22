pub use ainonymous_revocation::{Revocation, TransportBinding};
use hdi::prelude::*;

#[hdk_entry_helper]
#[derive(Clone)]
pub struct RevocationEntry {
    pub revocation: Revocation,
}

#[derive(Serialize, Deserialize, Debug, SerializedBytes)]
pub struct RegistryProperties {
    pub network_id: String,
    pub network_admin_pubkey: Vec<u8>,
}

pub fn validate_revocation(
    r: &Revocation,
    author: &AgentPubKey,
) -> ExternResult<ValidateCallbackResult> {
    let props: RegistryProperties = dna_info()?.modifiers.properties.try_into().map_err(|e| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "CH6 properties missing: {e:?}"
        )))
    })?;
    let admin: [u8; 32] = props.network_admin_pubkey.try_into().map_err(|_| {
        wasm_error!(WasmErrorInner::Guest(
            "CH6 admin key must be 32 bytes".into()
        ))
    })?;
    let result = if author.get_raw_32() != admin || r.agent_id == author.to_string() {
        Err("only admin may revoke; admin self-revocation is forbidden".to_string())
    } else {
        // Reject alternate textual encodings of the target identity.
        match AgentPubKey::try_from(r.agent_id.as_str()) {
            Ok(agent) if agent.to_string() == r.agent_id => r.verify_author(
                &props.network_id,
                &admin,
                author.get_raw_32(),
                &author.to_string(),
            ),
            _ => Err("invalid canonical agent id".into()),
        }
    };
    Ok(match result {
        Ok(()) => ValidateCallbackResult::Valid,
        Err(e) => ValidateCallbackResult::Invalid(e),
    })
}

#[hdk_entry_helper]
pub struct IndexAnchor {
    pub name: String,
}

pub fn revocations_anchor() -> ExternResult<EntryHash> {
    hash_entry(IndexAnchor {
        name: "ch6/revocations/v1".into(),
    })
}
pub fn bindings_anchor() -> ExternResult<EntryHash> {
    hash_entry(IndexAnchor {
        name: "ch6/bindings/v1".into(),
    })
}

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
    Revocation(RevocationEntry),
}

#[hdk_link_types]
pub enum LinkTypes {
    AgentToCapabilities,
    AgentToHeartbeats,
    ModelToAgents,  // anchor "models/{id}" → agents capables
    RegionToAgents, // anchor "regions/{region}" → agents
    PathLinks,      // liens internes des anchors/paths (hdk anchor)
    Revocations,
    TransportBindings,
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::CreateEntry(OpEntry::CreateEntry { app_entry, action }) => match app_entry {
            EntryTypes::Revocation(entry) => {
                validate_revocation(&entry.revocation, action.author())
            }
            EntryTypes::NodeCapabilities(caps) => validate_capabilities(&caps),
            EntryTypes::NodeHeartbeat(hb) => validate_heartbeat(&hb),
        },
        FlatOp::CreateRecord(OpRecord::CreateEntry {
            app_entry: EntryTypes::Revocation(entry),
            action,
        }) => validate_revocation(&entry.revocation, action.author()),
        FlatOp::CreateEntry(OpEntry::UpdateEntry {
            app_entry: EntryTypes::Revocation(_),
            ..
        })
        | FlatOp::CreateRecord(OpRecord::UpdateEntry {
            app_entry: EntryTypes::Revocation(_),
            ..
        })
        | FlatOp::Update(OpUpdate::Entry {
            app_entry: EntryTypes::Revocation(_),
            ..
        }) => Ok(ValidateCallbackResult::Invalid(
            "revocations are immutable".into(),
        )),
        FlatOp::Delete(OpDelete { action })
        | FlatOp::CreateRecord(OpRecord::DeleteEntry { action, .. }) => {
            let original = must_get_valid_record(action.deletes_address.clone())?;
            if original
                .entry()
                .to_app_option::<RevocationEntry>()
                .ok()
                .flatten()
                .is_some()
                || original
                    .entry()
                    .to_app_option::<NodeCapabilities>()
                    .ok()
                    .flatten()
                    .is_some()
            {
                Ok(ValidateCallbackResult::Invalid(
                    "CH6 records are append-only".into(),
                ))
            } else {
                Ok(ValidateCallbackResult::Valid)
            }
        }
        FlatOp::Link(OpLink::CreateLink { link_type, action }) => {
            validate_create_link(link_type, &action)
        }
        FlatOp::Link(OpLink::DeleteLink {
            original_action,
            action,
            ..
        }) => {
            if original_action.base_address == revocations_anchor()?.into()
                || original_action.base_address == bindings_anchor()?.into()
            {
                return Ok(ValidateCallbackResult::Invalid(
                    "CH6 index links are append-only".into(),
                ));
            }
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
        LinkTypes::Revocations | LinkTypes::TransportBindings => {
            let is_revocation = matches!(link_type, LinkTypes::Revocations);
            let expected = if is_revocation {
                revocations_anchor()?
            } else {
                bindings_anchor()?
            };
            if action.base_address != expected.into() {
                return Ok(ValidateCallbackResult::Invalid("invalid CH6 anchor".into()));
            }
            let Some(hash) = action.target_address.clone().into_action_hash() else {
                return Ok(ValidateCallbackResult::Invalid(
                    "CH6 index must target action".into(),
                ));
            };
            let record = must_get_valid_record(hash)?;
            if record.action().action_type() != ActionType::Create
                || record.action().author() != action.author()
            {
                return Ok(ValidateCallbackResult::Invalid(
                    "CH6 index author/type mismatch".into(),
                ));
            }
            if is_revocation {
                let Some(entry) = record
                    .entry()
                    .to_app_option::<RevocationEntry>()
                    .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
                else {
                    return Ok(ValidateCallbackResult::Invalid(
                        "revocation entry missing".into(),
                    ));
                };
                validate_revocation(&entry.revocation, action.author())
            } else {
                let Some(caps) = record
                    .entry()
                    .to_app_option::<NodeCapabilities>()
                    .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
                else {
                    return Ok(ValidateCallbackResult::Invalid(
                        "capabilities missing".into(),
                    ));
                };
                validate_capabilities(&caps)
            }
        }

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
