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
        requester: agent_info()?.agent_initial_pubkey,
        timestamp: sys_time()?,
        execution_mode: "solo".into(),
    };

    let action_hash = create_entry(EntryTypes::InferenceRequest(request))?;

    // Lier l'agent à ses requêtes
    create_link(
        agent_info()?.agent_initial_pubkey,
        action_hash.clone(),
        LinkTypes::AgentToRequests,
        (),
    )?;

    // Lier le modèle à ses requêtes (pour stats)
    let model_anchor = anchor(LinkTypes::PathLinks, "models".to_string(), input.model_id.clone())?;
    create_link(model_anchor, action_hash.clone(), LinkTypes::ModelToRequests, ())?;

    get(action_hash, GetOptions::default())?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Record non trouvé".into())))
}

/// Négocier une session QUIC avec un nœud distant (plan de contrôle)
/// Le nœud distant ouvrira un listener QUIC et retournera son endpoint + token
#[hdk_extern]
pub fn negotiate_quic_session(input: QuicNegotiateInput) -> ExternResult<QuicSessionResult> {
    use sha2::{Sha256, Digest};

    // Générer token éphémère (32 bytes) via le host Holochain — jamais stocké en DHT
    let token = random_bytes(32)?.to_vec();

    // Stocker uniquement le hash du token en DHT
    let mut hasher = Sha256::new();
    hasher.update(&token);
    let token_hash = hasher.finalize().to_vec();

    // Endpoint QUIC et clé publique ed25519 de CE nœud, résolus via agent-registry.
    // La pubkey est incluse dans la réponse pour permettre le pinning mTLS côté demandeur.
    let my_info = resolve_my_agent_info();
    let my_endpoint = my_info
        .as_ref()
        .and_then(|i| i.quic_endpoint.clone())
        .unwrap_or(input.local_quic_endpoint.clone());
    let my_node_pubkey = my_info.and_then(|i| i.node_pubkey);

    // Émettre un signal vers le daemon local pour ouvrir le listener QUIC.
    // Le next-hop est propagé pour que le worker sache vers qui relayer.
    emit_signal(QuicListenerSignal {
        session_token: token.clone(),
        requestor: call_info()?.provenance,
        layer_range: input.layer_range,
        expires_in_seconds: 30,
        next_agent_id: input.next_agent_id.clone(),
        next_layer_range: input.next_layer_range,
    })?;

    // Publier l'offre de session (avec hash seulement) dans le DHT pour traçabilité
    let offer = QuicSessionOffer {
        request_id: input.request_id,
        requestor: call_info()?.provenance,
        quic_endpoint: my_endpoint.clone(),
        session_token_hash: token_hash,
        expires_at: Timestamp::from_micros(
            sys_time()?.as_micros() + 30_000_000 // +30 secondes
        ),
        layer_range: input.layer_range,
    };
    create_entry(EntryTypes::QuicSessionOffer(offer))?;

    // Retourner le token en clair au demandeur (via remote call, pas le DHT).
    // Palier D : inclure la clé publique ed25519 pour le pinning mTLS côté demandeur.
    Ok(QuicSessionResult {
        quic_endpoint: my_endpoint,
        session_token: token,
        expires_in_seconds: 30,
        layer_range: input.layer_range,
        next_agent_id: input.next_agent_id,
        next_layer_range: input.next_layer_range,
        node_pubkey: my_node_pubkey,
    })
}

/// Callback d'init : autorise tout agent du réseau public à appeler
/// `negotiate_quic_session` à distance (nécessaire pour la négociation DHT).
#[hdk_extern]
fn init(_: ()) -> ExternResult<InitCallbackResult> {
    let mut fns: std::collections::HashSet<GrantedFunction> = std::collections::HashSet::new();
    fns.insert((zome_info()?.name, "negotiate_quic_session".into()));
    create_cap_grant(CapGrantEntry {
        tag: "mesh-remote-negotiate".into(),
        access: CapAccess::Unrestricted,
        functions: GrantedFunctions::Listed(fns),
    })?;
    Ok(InitCallbackResult::Pass)
}

/// Négociation SORTANTE via le DHT : appelle `negotiate_quic_session` sur
/// l'agent `target` (call_remote) et relaie son offre (endpoint + token).
/// Pendant DHT du POST REST `/mesh/session/negotiate` du bootstrap statique.
#[hdk_extern]
pub fn request_remote_session(input: RemoteSessionInput) -> ExternResult<QuicSessionResult> {
    let target = AgentPubKey::try_from(input.target.clone())
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("agent cible invalide: {e:?}"))))?;

    let payload = QuicNegotiateInput {
        request_id: String::new(),
        local_quic_endpoint: String::new(),
        layer_range: input.layer_range,
        expert_ids: None,
        next_agent_id: input.next_agent_id,
        next_layer_range: input.next_layer_range,
    };

    let resp = call_remote(
        target,
        "inference-mesh-coordinator",
        "negotiate_quic_session".into(),
        None,
        payload,
    )?;

    match resp {
        ZomeCallResponse::Ok(data) => data
            .decode()
            .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e))),
        ZomeCallResponse::Unauthorized(..)
        | ZomeCallResponse::CountersigningSession(..)
        | ZomeCallResponse::AuthenticationFailed(..) => {
            Err(wasm_error!(WasmErrorInner::Guest("call_remote non autorisé".into())))
        }
        ZomeCallResponse::NetworkError(e) => {
            Err(wasm_error!(WasmErrorInner::Guest(format!("erreur réseau call_remote: {e}"))))
        }
    }
}

/// Informations réseau de CET agent, résolues via agent-registry (cross-DNA
/// intra-agent) : endpoint QUIC + clé publique ed25519 pour le pinning mTLS.
fn resolve_my_agent_info() -> Option<AgentQuicEndpoint> {
    let me = agent_info().ok()?.agent_initial_pubkey;
    let resp = call(
        CallTargetCell::OtherRole("agent-registry".into()),
        "agent-registry-coordinator",
        "get_node_capabilities".into(),
        None,
        me,
    )
    .ok()?;
    match resp {
        ZomeCallResponse::Ok(data) => data.decode().ok()?,
        _ => None,
    }
}

/// Endpoint QUIC de CET agent (wrapper de compatibilité).
fn resolve_my_quic_endpoint() -> Option<String> {
    resolve_my_agent_info()?.quic_endpoint
}

/// Publier les métriques d'inférence
#[hdk_extern]
pub fn publish_metrics(input: InferenceMetrics) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::InferenceMetrics(input))
}

/// Calculer un plan d'exécution optimal (appel les données de l'agent-registry)
#[hdk_extern]
pub fn compute_execution_plan(input: PlanInput) -> ExternResult<ExecutionPlanOutput> {
    // Appel cross-DNA vers agent-registry pour obtenir les nœuds disponibles
    let available_nodes = call(
        CallTargetCell::OtherRole("agent-registry".into()),
        "agent-registry-coordinator",
        "get_available_nodes".into(),
        None,
        input.model_id.clone(),
    )?;

    let nodes: Vec<NodeInfo> = match available_nodes {
        ZomeCallResponse::Ok(result) => result.decode()
            .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?,
        ZomeCallResponse::Unauthorized(..)
        | ZomeCallResponse::CountersigningSession(..)
        | ZomeCallResponse::AuthenticationFailed(..) =>
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
        ExecutionPlanOutput::Solo {
            node: nodes[0].agent_id.clone(),
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
        LinkQuery::try_new(agent, LinkTypes::AgentToRequests)?,
        GetStrategy::default(),
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

fn build_pipeline_split_plan(nodes: &[NodeInfo], model_id: &str) -> ExecutionPlanOutput {
    let total_layers = model_total_layers(model_id);
    let n = nodes.len().min(4) as u32; // max 4 nœuds pour le pipeline
    let layers_per_node = total_layers / n;

    let stages: Vec<PipelineStageOutput> = nodes.iter().take(n as usize)
        .enumerate()
        .map(|(i, node)| {
            let start = i as u32 * layers_per_node;
            let end = if i == n as usize - 1 { total_layers - 1 } else { start + layers_per_node - 1 };
            PipelineStageOutput {
                node: node.agent_id.clone(),
                quic_endpoint: node.quic_endpoint.clone().unwrap_or_default(),
                layer_start: start,
                layer_end: end,
                is_last: i == n as usize - 1,
            }
        })
        .collect();

    ExecutionPlanOutput::PipelineSplit { stages }
}

fn build_expert_shard_plan(nodes: &[NodeInfo], _model_id: &str) -> ExecutionPlanOutput {
    // Pour Gemma 4-26B MoE : distribuer les experts entre les nœuds
    // Tous les nœuds portent le tronc dense ; les experts sparse sont distribués
    let trunk_node = nodes[0].agent_id.clone();

    let stages: Vec<ExpertStageOutput> = nodes.iter()
        .enumerate()
        .map(|(i, node)| ExpertStageOutput {
            node: node.agent_id.clone(),
            quic_endpoint: node.quic_endpoint.clone().unwrap_or_default(),
            expert_ids: (0..64u32).filter(|e| (*e as usize) % nodes.len() == i).collect(),
            has_trunk: true,
        })
        .collect();

    ExecutionPlanOutput::ExpertShard { stages, trunk_node }
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
    #[serde(default)]
    pub next_agent_id: Option<String>,
    #[serde(default)]
    pub next_layer_range: Option<(u32, u32)>,
}

/// Entrée de la négociation SORTANTE (call_remote vers `target`).
#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteSessionInput {
    /// AgentPubKey cible (encodage holo_hash, ex: "uhCAk…").
    pub target: String,
    pub layer_range: Option<(u32, u32)>,
    #[serde(default)]
    pub next_agent_id: Option<String>,
    #[serde(default)]
    pub next_layer_range: Option<(u32, u32)>,
}

/// Vue minimale des capacités d'un agent (décodage partiel de NodeCapabilities
/// d'agent-registry) : endpoint QUIC + clé publique ed25519 pour le pinning mTLS.
#[derive(Serialize, Deserialize, Debug)]
pub struct AgentQuicEndpoint {
    #[serde(default)]
    pub quic_endpoint: Option<String>,
    /// Clé publique ed25519 hex (palier D) — absente si le nœud n'a pas encore
    /// migré vers `load_or_generate` ; ignorée sans paniquer (`serde(default)`).
    #[serde(default)]
    pub node_pubkey: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuicSessionResult {
    pub quic_endpoint: String,
    pub session_token: Vec<u8>,
    pub expires_in_seconds: u32,
    pub layer_range: Option<(u32, u32)>,
    #[serde(default)]
    pub next_agent_id: Option<String>,
    #[serde(default)]
    pub next_layer_range: Option<(u32, u32)>,
    /// Clé publique ed25519 hex du nœud répondant (palier D).
    /// Renseignée si le nœud a publié `node_pubkey` dans agent-registry.
    /// Le demandeur l'utilise pour pinner le certificat mTLS lors de la connexion QUIC.
    #[serde(default)]
    pub node_pubkey: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlanInput {
    pub model_id: String,
    /// Mode solo forcé (pas de pipeline). Défaut : false → routage automatique.
    #[serde(default)]
    pub force_solo: bool,
    /// Préférence de région (optionnel).
    #[serde(default)]
    pub prefer_region: Option<String>,
}

/// Plan d'exécution retourné par `compute_execution_plan`.
///
/// Les noms de champs (`node`, `quic_endpoint`) correspondent exactement à
/// `ainonymous_types::ExecutionPlan` côté daemon, afin que le daemon puisse
/// d