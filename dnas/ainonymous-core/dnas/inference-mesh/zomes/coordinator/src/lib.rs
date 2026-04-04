use hdk::prelude::*;
use inference_mesh_integrity::*;

// ─── API publique ─────────────────────────────────────────────────────────────

/// Soumettre une requête d'inférence dans le DHT
#[hdk_extern]
pub fn submit_inference_request(input: SubmitRequestInput) -> ExternResult<Record> {
    use sha2::{Sha256, Digest};

    let mut hasher = Sha256::new();
    hasher.update(&input.prompt_bytes);
    let prompt_hash = hasher.finalize().to_vec();

    let request = InferenceRequest {
        request_id: input.request_id,
        model_id: input.model_id.clone(),
        prompt_hash,
        max_tokens: input.max_tokens.unwrap_or(2048),
        temperature: input.temperature.unwrap_or(0.7),
        requester: agent_info()?.agent_latest_pubkey,
        timestamp: sys_time()?,
        execution_mode: "solo".into(),
    };

    let action_hash = create_entry(EntryTypes::InferenceRequest(request))?;

    // Lier l'agent à ses requêtes
    create_link(
        agent_info()?.agent_latest_pubkey,
        action_hash.clone(),
        LinkTypes::AgentToRequests,
        (),
    )?;

    // Lier le modèle à ses requêtes (pour stats)
    let model_anchor = anchor("models", &input.model_id)?;
    create_link(model_anchor, action_hash.clone(), LinkTypes::ModelToRequests, ())?;

    get(action_hash, GetOptions::default())?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Record non trouvé".into())))
}

/// Négocier une session QUIC avec un nœud distant (plan de contrôle)
/// Le nœud distant ouvrira un listener QUIC et retournera son endpoint + token
#[hdk_extern]
pub fn negotiate_quic_session(input: QuicNegotiateInput) -> ExternResult<QuicSessionResult> {
    use sha2::{Sha256, Digest};
    use rand::RngCore;

    // Générer token éphémère (32 bytes) — jamais stocké en DHT
    let mut token = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut token);

    // Stocker uniquement le hash du token en DHT
    let mut hasher = Sha256::new();
    hasher.update(&token);
    let token_hash = hasher.finalize().to_vec();

    // Émettre un signal vers le daemon local pour ouvrir le listener QUIC
    emit_signal(QuicListenerSignal {
        session_token: token.clone(),
        requestor: call_info()?.provenance,
        layer_range: input.layer_range,
        expires_in_seconds: 30,
    })?;

    // Publier l'offre de session (avec hash seulement) dans le DHT pour traçabilité
    let offer = QuicSessionOffer {
        request_id: input.request_id,
        requestor: call_info()?.provenance,
        quic_endpoint: input.local_quic_endpoint.clone(),
        session_token_hash: token_hash,
        expires_at: Timestamp::from_micros(
            sys_time()?.as_micros() + 30_000_000 // +30 secondes
        ),
        layer_range: input.layer_range,
    };
    create_entry(EntryTypes::QuicSessionOffer(offer))?;

    // Retourner le token en clair au demandeur (via remote call, pas le DHT)
    Ok(QuicSessionResult {
        quic_endpoint: input.local_quic_endpoint,
        session_token: token,
        expires_in_seconds: 30,
        layer_range: input.layer_range,
    })
}

/// Publier les métriques d'inférence
#[hdk_extern]
pub fn publish_metrics(input: InferenceMetrics) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::InferenceMetrics(input))
}

/// Calculer un plan d'exécution optimal (appel les données de l'agent-registry)
#[hdk_extern]
pub fn compute_execution_plan(input: PlanInput) -> ExternResult<ExecutionPlanResult> {
    // Appel cross-DNA vers agent-registry pour obtenir les nœuds disponibles
    let available_nodes = call(
        CallTargetCell::OtherRole("agent-registry".into()),
        "coordinator",
        "get_available_nodes",
        None,
        input.model_id.clone(),
    )?;

    let nodes: Vec<NodeInfo> = match available_nodes {
        ZomeCallResponse::Ok(result) => result.decode()
            .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?,
        ZomeCallResponse::Unauthorized(..) | ZomeCallResponse::CountersigningSession(..) =>
            return Err(wasm_error!(WasmErrorInner::Guest("Appel non autorisé".into()))),
        ZomeCallResponse::NetworkError(e) =>
            return Err(wasm_error!(WasmErrorInner::Guest(format!("Erreur réseau: {}", e)))),
    };

    // Sélectionner le mode d'exécution
    let plan = if nodes.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            format!("Aucun nœud disponible pour {}", input.model_id)
        )));
    } else if nodes.len() == 1 || input.force_solo {
        ExecutionPlanResult::Solo {
            node_agent: nodes[0].agent_id.clone(),
            quic_endpoint: nodes[0].quic_endpoint.clone().unwrap_or_default(),
        }
    } else if input.model_id.contains("moe") {
        // MoE : distribuer par experts
        build_expert_shard_plan(&nodes, &input.model_id)
    } else {
        // Dense : pipeline split par couches
        build_pipeline_split_plan(&nodes, &input.model_id)
    };

    Ok(plan)
}

/// Récupérer les métriques de performance d'un nœud
#[hdk_extern]
pub fn get_node_metrics(agent: AgentPubKey) -> ExternResult<Vec<InferenceMetrics>> {
    let links = get_links(
        GetLinksInputBuilder::try_new(agent, LinkTypes::AgentToRequests)?.build()
    )?;

    let mut metrics = Vec::new();
    for link in links.iter().take(100) {
        if let Some(hash) = link.target.clone().into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                if let Ok(Some(metric)) = record.entry().to_app_option::<InferenceMetrics>() {
                    metrics.push(metric);
                }
            }
        }
    }
    Ok(metrics)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn build_pipeline_split_plan(nodes: &[NodeInfo], model_id: &str) -> ExecutionPlanResult {
    let total_layers = model_total_layers(model_id);
    let n = nodes.len().min(4) as u32; // max 4 nœuds pour le pipeline
    let layers_per_node = total_layers / n;

    let stages: Vec<PipelineStageResult> = nodes.iter().take(n as usize)
        .enumerate()
        .map(|(i, node)| {
            let start = i as u32 * layers_per_node;
            let end = if i == n as usize - 1 { total_layers - 1 } else { start + layers_per_node - 1 };
            PipelineStageResult {
                node_agent: node.agent_id.clone(),
                quic_endpoint: node.quic_endpoint.clone().unwrap_or_default(),
                layer_start: start,
                layer_end: end,
                is_last: i == n as usize - 1,
            }
        })
        .collect();

    ExecutionPlanResult::PipelineSplit { stages }
}

fn build_expert_shard_plan(nodes: &[NodeInfo], model_id: &str) -> ExecutionPlanResult {
    // Pour Gemma 4-26B MoE : distribuer les experts entre les nœuds
    // Tous les nœuds portent le tronc dense ; les experts sparse sont distribués
    let trunk_node = nodes[0].agent_id.clone();

    // Nœud 0 : trunk + experts 0..N/2
    // Nœud 1 : trunk + experts N/2..N
    // etc.
    let stages: Vec<ExpertStageResult> = nodes.iter()
        .enumerate()
        .map(|(i, node)| ExpertStageResult {
            node_agent: node.agent_id.clone(),
            quic_endpoint: node.quic_endpoint.clone().unwrap_or_default(),
            expert_ids: (0..64).filter(|e| e % nodes.len() == i).collect(),
            has_trunk: true,
        })
        .collect();

    ExecutionPlanResult::ExpertShard { stages, trunk_node }
}

fn model_total_layers(model_id: &str) -> u32 {
    match model_id {
        id if id.contains("31b")  => 48,
        id if id.contains("26b")  => 30,
        id if id.contains("e4b")  => 32,
        id if id.contains("e2b")  => 18,
        id if id.contains("70b")  => 80,
        id if id.contains("72b")  => 80,
        _ => 32,
    }
}

// ─── Types d'entrée/sortie ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
pub struct SubmitRequestInput {
    pub request_id: String,
    pub model_id: String,
    pub prompt_bytes: Vec<u8>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuicNegotiateInput {
    pub request_id: String,
    pub local_quic_endpoint: String,
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuicSessionResult {
    pub quic_endpoint: String,
    pub session_token: Vec<u8>,
    pub expires_in_seconds: u32,
    pub layer_range: Option<(u32, u32)>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlanInput {
    pub model_id: String,
    pub force_solo: bool,
    pub prefer_region: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ExecutionPlanResult {
    Solo {
        node_agent: String,
        quic_endpoint: String,
    },
    PipelineSplit {
        stages: Vec<PipelineStageResult>,
    },
    ExpertShard {
        stages: Vec<ExpertStageResult>,
        trunk_node: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PipelineStageResult {
    pub node_agent: String,
    pub quic_endpoint: String,
    pub layer_start: u32,
    pub layer_end: u32,
    pub is_last: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExpertStageResult {
    pub node_agent: String,
    pub quic_endpoint: String,
    pub expert_ids: Vec<u32>,
    pub has_trunk: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NodeInfo {
    pub agent_id: String,
    pub quic_endpoint: Option<String>,
    pub vram_gb: f32,
    pub current_load: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuicListenerSignal {
    pub session_token: Vec<u8>,
    pub requestor: AgentPubKey,
    pub layer_range: Option<(u32, u32)>,
    pub expires_in_seconds: u32,
}
