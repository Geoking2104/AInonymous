use hdk::prelude::*;
use agent_registry_integrity::*;

/// Annoncer les capacités de ce nœud dans le DHT
#[hdk_extern]
pub fn announce_capabilities(caps: NodeCapabilities) -> ExternResult<ActionHash> {
    let hash = create_entry(EntryTypes::NodeCapabilities(caps.clone()))?;
    let agent = agent_info()?.agent_latest_pubkey;

    // Lier l'agent à ses capacités
    create_link(agent.clone(), hash.clone(), LinkTypes::AgentToCapabilities, ())?;

    // Lier chaque modèle chargé à cet agent (pour recherche par modèle)
    for model in &caps.loaded_models {
        if model.ready {
            let model_anchor = anchor("models", &model.model_id)?;
            create_link(model_anchor, agent.clone(), LinkTypes::ModelToAgents, ())?;
        }
    }

    // Lier la région à cet agent
    if let Some(ref region) = caps.region_hint {
        let region_anchor = anchor("regions", region)?;
        create_link(region_anchor, agent, LinkTypes::RegionToAgents, ())?;
    }

    Ok(hash)
}

/// Heartbeat périodique (toutes les 30s)
#[hdk_extern]
pub fn heartbeat(hb: NodeHeartbeat) -> ExternResult<ActionHash> {
    let hash = create_entry(EntryTypes::NodeHeartbeat(hb))?;
    let agent = agent_info()?.agent_latest_pubkey;
    create_link(agent, hash.clone(), LinkTypes::AgentToHeartbeats, ())?;
    Ok(hash)
}

/// Mettre à jour l'endpoint QUIC
#[hdk_extern]
pub fn update_quic_endpoint(input: UpdateQuicInput) -> ExternResult<ActionHash> {
    // Récupérer les capacités actuelles et les mettre à jour
    let agent = agent_info()?.agent_latest_pubkey;
    let links = get_links(
        GetLinksInputBuilder::try_new(agent, LinkTypes::AgentToCapabilities)?.build()
    )?;

    if let Some(last_link) = links.last() {
        if let Some(hash) = last_link.target.clone().into_action_hash() {
            if let Some(record) = get(hash.clone(), GetOptions::default())? {
                if let Ok(Some(mut caps)) = record.entry().to_app_option::<NodeCapabilities>() {
                    caps.quic_endpoint = Some(input.endpoint);
                    return update_entry(hash, EntryTypes::NodeCapabilities(caps));
                }
            }
        }
    }

    Err(wasm_error!(WasmErrorInner::Guest("Capacités non trouvées".into())))
}

/// Récupérer les nœuds disponibles pour un modèle donné
/// Filtre par heartbeat récent (< 60 secondes)
#[hdk_extern]
pub fn get_available_nodes(model_id: String) -> ExternResult<Vec<NodeSummary>> {
    let anchor = anchor("models", &model_id)?;
    let links = get_links(
        GetLinksInputBuilder::try_new(anchor, LinkTypes::ModelToAgents)?.build()
    )?;

    let now_ms = sys_time()?.as_millis();
    let mut summaries = Vec::new();

    for link in links {
        if let Some(agent_hash) = link.target.clone().into_entry_hash() {
            let agent: AgentPubKey = agent_hash.into();

            // Vérifier le heartbeat récent
            let hb_links = get_links(
                GetLinksInputBuilder::try_new(agent.clone(), LinkTypes::AgentToHeartbeats)?.build()
            )?;

            let recent_hb = hb_links.iter().rev().find_map(|l| {
                let age_ms = now_ms as i64 - l.timestamp.as_millis() as i64;
                if age_ms < 60_000 { // heartbeat < 60 secondes
                    l.target.clone().into_action_hash()
                } else { None }
            });

            if let Some(hb_hash) = recent_hb {
                if let Some(hb_record) = get(hb_hash, GetOptions::default())? {
                    if let Ok(Some(hb)) = hb_record.entry().to_app_option::<NodeHeartbeat>() {
                        // Récupérer les capacités
                        let caps = get_node_capabilities_inner(&agent)?;
                        if let Some(caps) = caps {
                            summaries.push(NodeSummary {
                                agent_id: agent.to_string(),
                                vram_gb: caps.vram_gb,
                                current_load: hb.current_load,
                                available_slots: hb.available_slots,
                                quic_endpoint: caps.quic_endpoint.clone(),
                                region_hint: caps.region_hint.clone(),
                                score: compute_score(&caps, &hb),
                            });
                        }
                    }
                }
            }
        }
    }

    // Trier par score décroissant
    summaries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(summaries)
}

/// Récupérer les capacités d'un agent spécifique
#[hdk_extern]
pub fn get_node_capabilities(agent: AgentPubKey) -> ExternResult<Option<NodeCapabilities>> {
    get_node_capabilities_inner(&agent)
}

fn get_node_capabilities_inner(agent: &AgentPubKey) -> ExternResult<Option<NodeCapabilities>> {
    let links = get_links(
        GetLinksInputBuilder::try_new(agent.clone(), LinkTypes::AgentToCapabilities)?.build()
    )?;

    if let Some(last_link) = links.last() {
        if let Some(hash) = last_link.target.clone().into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                return Ok(record.entry().to_app_option::<NodeCapabilities>()?);
            }
        }
    }
    Ok(None)
}

fn compute_score(caps: &NodeCapabilities, hb: &NodeHeartbeat) -> f32 {
    let vram_score  = (caps.vram_gb / 80.0).min(1.0) * 30.0;
    let load_score  = (1.0 - hb.current_load) * 40.0;
    let slots_score = (hb.available_slots as f32 / 8.0).min(1.0) * 20.0;
    let mem_score   = (1.0 - hb.memory_pressure) * 10.0;
    vram_score + load_score + slots_score + mem_score
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
pub struct NodeSummary {
    pub agent_id: String,
    pub vram_gb: f32,
    pub current_load: f32,
    pub available_slots: u8,
    pub quic_endpoint: Option<String>,
    pub region_hint: Option<String>,
    pub score: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateQuicInput {
    pub endpoint: String,
}
