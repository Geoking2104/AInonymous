# Spécification Zomes Holochain — AInonymous

> Structure complète des DNAs, zomes, entrées, liens et règles de validation.

---

## Structure hApp `ainonymous-core`

```
ainonymous-core/
├── dnas/
│   ├── inference-mesh/          # Coordination inférence distribuée
│   │   ├── zomes/
│   │   │   ├── integrity/       # Types de données + validation
│   │   │   └── coordinator/     # Logique métier + API publique
│   │   └── workdir/
│   ├── agent-registry/          # Registre des nœuds et capacités
│   │   ├── zomes/
│   │   │   ├── integrity/
│   │   │   └── coordinator/
│   │   └── workdir/
│   └── blackboard/              # Collaboration agents décentralisée
│       ├── zomes/
│       │   ├── integrity/
│       │   └── coordinator/
│       └── workdir/
├── ui/                          # Interface web (React)
├── tests/                       # Tests d'intégration
└── happ.yaml                    # Manifest hApp
```

---

## DNA 1 : `inference-mesh`

### Integrity Zome — Types de Données

```rust
// src/lib.rs (integrity zome)

use hdi::prelude::*;

/// Plan d'exécution calculé pour une requête donnée
#[hdk_entry_helper]
#[derive(Clone)]
pub struct InferenceRequest {
    pub request_id: String,          // UUID v4
    pub model_id: String,            // "gemma4-31b", "gemma4-26b-moe"
    pub prompt_hash: Vec<u8>,        // SHA256 du prompt (pas le prompt lui-même)
    pub max_tokens: u32,
    pub temperature: f32,
    pub requester: AgentPubKey,
    pub timestamp: Timestamp,
    pub execution_mode: ExecutionMode,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ExecutionMode {
    Solo,                            // 1 nœud, modèle entier
    PipelineSplit {                  // N nœuds, partition par couches
        layer_assignments: Vec<LayerAssignment>,
    },
    ExpertShard {                    // N nœuds, partition par experts MoE
        expert_assignments: Vec<ExpertAssignment>,
    },
    Speculative {                    // Draft + Verify
        draft_node: AgentPubKey,
        verify_node: AgentPubKey,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayerAssignment {
    pub node: AgentPubKey,
    pub layer_start: u32,
    pub layer_end: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExpertAssignment {
    pub node: AgentPubKey,
    pub expert_ids: Vec<u32>,
    pub has_trunk: bool,             // true = ce nœud porte le tronc dense
}

/// Résultat d'inférence partiel (par couche ou par expert)
#[hdk_entry_helper]
#[derive(Clone)]
pub struct LayerChunk {
    pub request_id: String,
    pub node: AgentPubKey,
    pub chunk_index: u32,
    pub activations_hash: Vec<u8>,   // hash des activations (pas les activations elles-mêmes)
    pub tokens_produced: Option<Vec<u32>>, // si chunk final : tokens générés
    pub latency_ms: u32,
    pub timestamp: Timestamp,
}

/// Métriques agrégées d'une requête complète
#[hdk_entry_helper]
#[derive(Clone)]
pub struct InferenceMetrics {
    pub request_id: String,
    pub total_latency_ms: u32,
    pub tokens_per_second: f32,
    pub nodes_used: u8,
    pub model_id: String,
    pub success: bool,
    pub error_reason: Option<String>,
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    InferenceRequest(InferenceRequest),
    LayerChunk(LayerChunk),
    InferenceMetrics(InferenceMetrics),
}

#[hdk_link_types]
pub enum LinkTypes {
    RequestToChunks,      // InferenceRequest → LayerChunk[]
    RequestToMetrics,     // InferenceRequest → InferenceMetrics
    AgentToRequests,      // AgentPubKey → InferenceRequest[]
}
```

### Integrity Zome — Validation

```rust
// Règles de validation déterministes — exécutées par tous les pairs

pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(OpEntry::CreateEntry { app_entry, action }) => {
            match app_entry {
                EntryTypes::InferenceRequest(req) => validate_inference_request(&req),
                EntryTypes::LayerChunk(chunk) => validate_layer_chunk(&chunk),
                EntryTypes::InferenceMetrics(metrics) => validate_inference_metrics(&metrics),
            }
        },
        FlatOp::RegisterCreateLink { link_type, .. } => {
            match link_type {
                LinkTypes::RequestToChunks => Ok(ValidateCallbackResult::Valid),
                LinkTypes::RequestToMetrics => Ok(ValidateCallbackResult::Valid),
                LinkTypes::AgentToRequests => Ok(ValidateCallbackResult::Valid),
            }
        },
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_inference_request(req: &InferenceRequest) -> ExternResult<ValidateCallbackResult> {
    // Vérifier UUID format
    if req.request_id.len() != 36 {
        return Ok(ValidateCallbackResult::Invalid("request_id doit être UUID v4".into()));
    }
    // Vérifier model_id connu
    let valid_models = [
        "gemma4-e2b", "gemma4-e4b", "gemma4-26b-moe", "gemma4-31b",
        "qwen3-8b", "qwen3-14b", "qwen3-32b", "qwen3-72b",
        "llama-3.3-70b", "deepseek-r2",
    ];
    if !valid_models.contains(&req.model_id.as_str()) {
        // Accepter aussi les modèles avec hash HuggingFace
        if !req.model_id.contains('/') && !req.model_id.ends_with(".gguf") {
            return Ok(ValidateCallbackResult::Invalid(
                format!("model_id '{}' non reconnu", req.model_id)
            ));
        }
    }
    // max_tokens dans les limites du modèle
    if req.max_tokens == 0 || req.max_tokens > 128_000 {
        return Ok(ValidateCallbackResult::Invalid("max_tokens hors plage [1, 128000]".into()));
    }
    // temperature valide
    if req.temperature < 0.0 || req.temperature > 4.0 {
        return Ok(ValidateCallbackResult::Invalid("temperature hors plage [0.0, 4.0]".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_layer_chunk(chunk: &LayerChunk) -> ExternResult<ValidateCallbackResult> {
    if chunk.activations_hash.len() != 32 {
        return Ok(ValidateCallbackResult::Invalid("activations_hash doit être SHA256 (32 bytes)".into()));
    }
    if chunk.latency_ms > 300_000 {  // 5 minutes max par chunk
        return Ok(ValidateCallbackResult::Invalid("latency_ms impossiblement élevée".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_inference_metrics(metrics: &InferenceMetrics) -> ExternResult<ValidateCallbackResult> {
    if metrics.tokens_per_second < 0.0 || metrics.tokens_per_second > 10_000.0 {
        return Ok(ValidateCallbackResult::Invalid("tokens_per_second hors plage plausible".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}
```

### Coordinator Zome — API Publique

```rust
// src/lib.rs (coordinator zome)
use hdk::prelude::*;

/// Soumettre une nouvelle requête d'inférence
#[hdk_extern]
pub fn submit_inference_request(input: SubmitRequestInput) -> ExternResult<Record> {
    let request = InferenceRequest {
        request_id: generate_uuid()?,
        model_id: input.model_id,
        prompt_hash: sha256(&input.prompt_bytes),
        max_tokens: input.max_tokens.unwrap_or(2048),
        temperature: input.temperature.unwrap_or(0.7),
        requester: agent_info()?.agent_latest_pubkey,
        timestamp: sys_time()?,
        execution_mode: ExecutionMode::Solo, // sera recalculé par le router
    };
    let action_hash = create_entry(EntryTypes::InferenceRequest(request.clone()))?;
    let record = get(action_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Entrée non trouvée".into())))?;
    // Lien agent → requêtes
    create_link(
        agent_info()?.agent_latest_pubkey,
        action_hash,
        LinkTypes::AgentToRequests,
        (),
    )?;
    Ok(record)
}

/// Exécuter une inférence locale (appel reçu depuis un autre nœud)
#[hdk_extern]
pub fn run_local_inference(input: LocalInferenceInput) -> ExternResult<LocalInferenceOutput> {
    // Délégation vers llama-server local via HTTP (port 9337)
    // Note : call_info() permet de vérifier l'origine de l'appel
    let caller = call_info()?.provenance;
    // Vérifier que le caller est dans le DHT (pair valide)
    // ... validation de capacité ...

    // Appel HTTP interne vers llama.cpp server
    let response = call_llama_server(&input)?;
    Ok(LocalInferenceOutput {
        request_id: input.request_id,
        tokens: response.tokens,
        latency_ms: response.latency_ms,
    })
}

/// Calculer et retourner le plan d'exécution optimal
#[hdk_extern]
pub fn compute_execution_plan(input: PlanInput) -> ExternResult<ExecutionPlan> {
    // 1. Récupérer nœuds disponibles depuis agent-registry
    let available_nodes = get_available_nodes(&input.model_id)?;

    // 2. Calculer mode optimal
    let plan = if available_nodes.len() == 1 {
        ExecutionPlan::Solo { node: available_nodes[0].agent.clone() }
    } else if input.model_id.contains("moe") {
        compute_expert_shard_plan(&available_nodes, &input.model_id)?
    } else {
        compute_pipeline_split_plan(&available_nodes, &input.model_id)?
    };

    Ok(plan)
}

/// Publier métriques après completion
#[hdk_extern]
pub fn publish_metrics(input: InferenceMetrics) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::InferenceMetrics(input))
}

/// Récupérer l'historique de performance d'un nœud
#[hdk_extern]
pub fn get_node_performance(node: AgentPubKey) -> ExternResult<Vec<InferenceMetrics>> {
    // Récupère les métriques depuis le DHT pour ce nœud
    let links = get_links(
        GetLinksInputBuilder::try_new(node, LinkTypes::AgentToRequests)?.build()
    )?;
    // ... aggregate metrics ...
    Ok(vec![])
}
```

---

## DNA 2 : `agent-registry`

### Types de Données

```rust
#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeCapabilities {
    pub vram_gb: f32,
    pub ram_gb: f32,
    pub gpu_vendor: GpuVendor,
    pub compute_backends: Vec<ComputeBackend>,
    pub loaded_models: Vec<LoadedModel>,
    pub max_concurrent_requests: u8,
    pub network_bandwidth_mbps: Option<u32>,
    pub region_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GpuVendor {
    AppleSilicon,
    Nvidia { vram_gb: f32, compute_capability: String },
    Amd { vram_gb: f32 },
    Intel { vram_gb: f32 },
    CpuOnly,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ComputeBackend {
    Metal,       // macOS Apple Silicon
    Cuda,        // NVIDIA
    Hip,         // AMD ROCm
    Vulkan,      // Cross-platform GPU
    Cpu,         // Toujours disponible
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct LoadedModel {
    pub model_id: String,
    pub model_hash: Vec<u8>,          // SHA256 GGUF
    pub quantization: Quantization,
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>,
    pub context_size: u32,
    pub ready: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Quantization {
    F16,
    Q8_0,
    Q6_K,
    Q5_K_M,
    Q4_K_M,
    Q4_0,
    Q3_K_M,
    IQ2_XXS,
    Dynamic(String),  // quantization dynamique (ex: EXL2)
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeHeartbeat {
    pub current_load: f32,           // 0.0 (idle) à 1.0 (saturé)
    pub available_slots: u8,         // requêtes simultanées disponibles
    pub queue_depth: u32,            // requêtes en attente
    pub memory_pressure: f32,        // 0.0 à 1.0
    pub temperature_c: Option<f32>,  // température GPU si disponible
}
```

### Coordinator Zome — Fonctions clés

```rust
/// Enregistrer/mettre à jour les capacités de ce nœud
#[hdk_extern]
pub fn announce_capabilities(caps: NodeCapabilities) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::NodeCapabilities(caps))
}

/// Heartbeat périodique (appelé toutes les 30s par le daemon local)
#[hdk_extern]
pub fn heartbeat(hb: NodeHeartbeat) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::NodeHeartbeat(hb))
}

/// Récupérer les nœuds disponibles pour un modèle donné
#[hdk_extern]
pub fn get_available_nodes(model_id: String) -> ExternResult<Vec<NodeInfo>> {
    // DHT anchor pour ce model_id
    let anchor = anchor("models", &model_id)?;
    let links = get_links(
        GetLinksInputBuilder::try_new(anchor, LinkTypes::ModelToNodes)?.build()
    )?;

    let mut nodes = Vec::new();
    for link in links {
        if let Some(record) = get(link.target.into_action_hash().unwrap(), GetOptions::default())? {
            // Vérifier heartbeat récent (< 60s)
            if let Some(caps) = extract_node_capabilities(&record) {
                nodes.push(NodeInfo {
                    agent: caps.agent,
                    caps,
                    last_heartbeat: get_last_heartbeat(&caps.agent)?,
                });
            }
        }
    }

    // Filtrer nœuds avec heartbeat trop ancien
    nodes.retain(|n| n.last_heartbeat_age_seconds() < 60);

    // Trier par score (VRAM libre, load, région)
    nodes.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap());

    Ok(nodes)
}
```

---

## DNA 3 : `blackboard`

### Types de Données

```rust
#[hdk_entry_helper]
#[derive(Clone)]
pub struct BlackboardPost {
    pub prefix: PostPrefix,
    pub content: String,         // max 4096 chars
    pub tags: Vec<String>,       // pour recherche
    pub ttl_hours: u32,          // 48 par défaut
    pub reply_to: Option<ActionHash>, // threading
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PostPrefix {
    Status,
    Finding,
    Question,
    Tip,
    Done,
    Custom(String),
}

impl PostPrefix {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "STATUS" => Self::Status,
            "FINDING" => Self::Finding,
            "QUESTION" => Self::Question,
            "TIP" => Self::Tip,
            "DONE" => Self::Done,
            other => Self::Custom(other.to_string()),
        }
    }
    pub fn prefix_str(&self) -> &str {
        match self {
            Self::Status => "STATUS",
            Self::Finding => "FINDING",
            Self::Question => "QUESTION",
            Self::Tip => "TIP",
            Self::Done => "DONE",
            Self::Custom(s) => s.as_str(),
        }
    }
}
```

### Validation Blackboard (Integrity Zome)

```rust
fn validate_blackboard_post(post: &BlackboardPost) -> ExternResult<ValidateCallbackResult> {
    // Longueur du contenu
    if post.content.is_empty() || post.content.len() > 4096 {
        return Ok(ValidateCallbackResult::Invalid(
            "Contenu entre 1 et 4096 caractères".into()
        ));
    }

    // Détection PII basique (chemins absolus, patterns clés)
    let pii_patterns = [
        r"/home/", r"C:\\Users\\", r"api_key", r"secret_key",
        r"password", r"token=", r"Bearer ", r"ssh-rsa",
    ];
    for pattern in &pii_patterns {
        if post.content.to_lowercase().contains(pattern) {
            return Ok(ValidateCallbackResult::Invalid(
                format!("Contenu contient données sensibles potentielles ({})", pattern)
            ));
        }
    }

    // TTL dans les limites
    if post.ttl_hours == 0 || post.ttl_hours > 168 {  // max 7 jours
        return Ok(ValidateCallbackResult::Invalid("TTL entre 1 et 168 heures".into()));
    }

    // Tags : max 10, chacun max 64 chars
    if post.tags.len() > 10 {
        return Ok(ValidateCallbackResult::Invalid("Maximum 10 tags".into()));
    }

    Ok(ValidateCallbackResult::Valid)
}
```

### Coordinator Zome — API Blackboard

```rust
/// Poster un message sur le blackboard
#[hdk_extern]
pub fn post(input: PostInput) -> ExternResult<ActionHash> {
    let post = BlackboardPost {
        prefix: PostPrefix::from_str(&input.prefix),
        content: strip_pii(&input.content),  // nettoyage côté auteur
        tags: input.tags.unwrap_or_default(),
        ttl_hours: input.ttl_hours.unwrap_or(48),
        reply_to: input.reply_to,
    };
    let hash = create_entry(EntryTypes::BlackboardPost(post.clone()))?;

    // Lier au tag anchor pour la recherche
    let timeline_anchor = anchor("timeline", "all")?;
    create_link(timeline_anchor, hash.clone(), LinkTypes::TimelineToPost, ())?;

    for tag in &post.tags {
        let tag_anchor = anchor("tags", tag)?;
        create_link(tag_anchor, hash.clone(), LinkTypes::TagToPost, ())?;
    }

    Ok(hash)
}

/// Recherche textuelle dans le blackboard
#[hdk_extern]
pub fn search(input: SearchInput) -> ExternResult<Vec<BlackboardPost>> {
    let all_posts = get_recent_posts(200)?;

    let results: Vec<BlackboardPost> = all_posts
        .into_iter()
        .filter(|post| {
            // Filtrer par prefix si spécifié
            if let Some(ref prefix) = input.prefix_filter {
                if post.prefix.prefix_str() != prefix.as_str() {
                    return false;
                }
            }
            // Recherche OR multi-termes dans contenu + tags
            input.terms.iter().any(|term| {
                post.content.to_lowercase().contains(&term.to_lowercase())
                    || post.tags.iter().any(|t| t.to_lowercase().contains(&term.to_lowercase()))
            })
        })
        .collect();

    Ok(results)
}

/// Récupérer les posts récents (filtrés par TTL)
#[hdk_extern]
pub fn get_recent_posts(limit: u32) -> ExternResult<Vec<BlackboardPost>> {
    let anchor = anchor("timeline", "all")?;
    let links = get_links(
        GetLinksInputBuilder::try_new(anchor, LinkTypes::TimelineToPost)?
            .build()
    )?;

    let now = sys_time()?;
    let mut posts = Vec::new();

    for link in links.into_iter().rev().take(limit as usize) {
        if let Some(record) = get(link.target.into_action_hash().unwrap(), GetOptions::default())? {
            if let Ok(Some(post)) = record.entry().to_app_option::<BlackboardPost>() {
                // Vérifier TTL
                let age_hours = (now.as_micros() - link.timestamp.as_micros()) / 3_600_000_000;
                if age_hours < post.ttl_hours as i64 {
                    posts.push(post);
                }
            }
        }
    }

    Ok(posts)
}
```

---

## Membrane Proofs — Réseau Public

AInonymous est un **réseau public ouvert** : aucune membrane proof requise pour rejoindre.
Tout nœud peut participer sans invitation ni staking.

```rust
// Dans la DNA inference-mesh
// Pas de membrane proof — accès libre

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    // Réseau public : tout agent est accepté sans condition
    Ok(ValidateCallbackResult::Valid)
}
```

La protection contre les comportements malveillants repose uniquement sur :
- La **validation déterministe** dans les integrity zomes (données invalides rejetées par tous les pairs)
- Les **warrants** : preuve cryptographique publiée en DHT si un pair détecte un comportement invalide, entraînant l'éjection automatique du nœud fautif
- Le **scoring de réputation** dans `agent-registry` (basé sur métriques historiques publiées en DHT) — les nœuds avec mauvaises métriques reçoivent moins de requêtes, sans être bloqués

Cette approche est cohérente avec Holochain : pas de gouvernance centralisée, mais des règles du jeu enforced cryptographiquement par les pairs eux-mêmes.

### Ajout d'un QUIC Session Token dans la Membrane Zome

Même en réseau public, la négociation QUIC nécessite un token de session éphémère pour authentifier la connexion de données entre deux nœuds spécifiques. Ce token n'est pas une membrane proof — c'est un secret de session généré à la demande :

```rust
// Coordinator zome : inference-mesh
// Négocier une session QUIC avec un nœud distant

#[hdk_extern]
pub fn negotiate_quic_session(input: QuicNegotiateInput) -> ExternResult<QuicSessionOffer> {
    // Vérifier que le demandeur est un agent valide dans le DHT
    // (présence d'un heartbeat récent dans agent-registry)
    let caller = call_info()?.provenance;
    verify_active_agent(&caller)?;

    // Générer token de session aléatoire (32 bytes, éphémère, non stocké en DHT)
    let session_token = random_bytes(32)?;

    // Ouvrir listener QUIC local (délégué au daemon Rust via signal)
    emit_signal(Signal::OpenQuicListener {
        session_token: session_token.clone(),
        peer: caller.clone(),
        layer_range: input.layer_range,
        expires_in_seconds: 30,
    })?;

    // Retourner l'adresse QUIC au nœud demandeur
    Ok(QuicSessionOffer {
        quic_endpoint: get_local_quic_addr()?,
        session_token,
        layer_range: input.layer_range,
        expires_at: sys_time()?.checked_add(Duration::from_secs(30))
            .ok_or(wasm_error!(WasmErrorInner::Guest("overflow".into())))?,
    })
}
```

---

## Résumé des DNAs et Zomes

| DNA | Integrity Zome | Coordinator Zome | Rôle |
|---|---|---|---|
| `inference-mesh` | Types: Request, Chunk, Metrics | submit, route, execute, publish_metrics | Cœur inférence distribuée |
| `agent-registry` | Types: Capabilities, Heartbeat, LoadedModel | announce, heartbeat, get_available_nodes | Annuaire des nœuds |
| `blackboard` | Types: BlackboardPost | post, search, get_recent_posts | Collaboration agents |
