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

## DNA 4 : `attestation` (nouveau)

### Rôle

DNA dédiée à l'attestation des nœuds et à la validation des modèles. Sépare ces responsabilités de sécurité de la logique d'inférence.

### Types de Données (Integrity Zome)

```rust
use hdi::prelude::*;

// ── Attestation nœud ─────────────────────────────────────────────

#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeAttestation {
    pub agent: AgentPubKey,
    pub timestamp: Timestamp,
    pub hardware_fingerprint: HardwareFingerprint,
    pub benchmark_results: BenchmarkResults,
    pub holochain_version: String,         // ex: "0.6.1"
    pub daemon_version: String,            // version ainonymous-daemon
    pub attestation_signature: Vec<u8>,    // ed25519 sign(agent_key, content_hash)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HardwareFingerprint {
    pub gpu_uuid: Option<String>,           // UUID GPU NVIDIA/AMD si disponible
    pub metal_device_id: Option<u64>,       // Apple Silicon device ID
    pub vram_total_bytes: u64,
    pub ram_total_bytes: u64,
    pub cpu_cores: u32,
    pub cpu_model: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BenchmarkResults {
    pub model_id: String,
    pub tokens_per_second: f32,
    pub ttft_ms: u32,                        // time-to-first-token
    pub benchmark_prompt_hash: Vec<u8>,      // SHA256 du prompt standardisé
    pub measured_at: Timestamp,
}

// ── Manifeste de modèle ──────────────────────────────────────────

#[hdk_entry_helper]
#[derive(Clone)]
pub struct ModelManifest {
    pub model_id: String,
    pub model_hash: Vec<u8>,                // SHA256 du fichier GGUF (32 bytes)
    pub huggingface_repo: Option<String>,   // ex: "google/gemma-4-31b-GGUF"
    pub expected_size_bytes: u64,
    pub quantization: String,               // "Q4_K_M", "Q8_0", "F16", etc.
    pub architecture: String,               // "llama", "gemma", "qwen3"
    pub context_length: u32,
    pub layer_count: u32,
    pub hidden_size: u32,
    pub published_by: AgentPubKey,
    pub verified_by: Vec<AgentPubKey>,      // pairs ayant co-signé ce hash
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct ModelClaim {
    pub manifest_hash: ActionHash,          // référence au ModelManifest
    pub node: AgentPubKey,
    pub verified_locally: bool,             // le nœud a vérifié SHA256 localement
    pub layer_range: Option<(u32, u32)>,    // si pipeline-split
    pub expert_ids: Option<Vec<u32>>,       // si MoE sharding
    pub timestamp: Timestamp,
}

// ── Warrants ─────────────────────────────────────────────────────

#[hdk_entry_helper]
#[derive(Clone)]
pub struct Warrant {
    pub accused: AgentPubKey,
    pub accuser: AgentPubKey,
    pub evidence: ActionHash,               // hash de l'entrée invalide détectée
    pub rule_violated: String,              // ex: "ModelManifest hash mismatch"
    pub timestamp: Timestamp,
    pub expires_at: Timestamp,              // 30 jours par défaut
    pub accuser_signature: Vec<u8>,         // ed25519 sign(accuser_key, warrant_hash)
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct WarrantRefutation {
    pub warrant_hash: ActionHash,           // référence au Warrant contesté
    pub refuted_by: AgentPubKey,            // l'accusé
    pub counter_evidence: ActionHash,       // preuve contradictoire
    pub explanation: String,                // max 2048 chars
    pub signature: Vec<u8>,
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    NodeAttestation(NodeAttestation),
    ModelManifest(ModelManifest),
    ModelClaim(ModelClaim),
    Warrant(Warrant),
    WarrantRefutation(WarrantRefutation),
}

#[hdk_link_types]
pub enum LinkTypes {
    AgentToAttestation,        // AgentPubKey → NodeAttestation
    ManifestToClaim,           // ModelManifest → ModelClaim[]
    AgentToWarrants,           // AgentPubKey → Warrant[] (warrants reçus)
    WarrantToRefutation,       // Warrant → WarrantRefutation
    ModelToManifest,           // model_id anchor → ModelManifest
}
```

### Validation (Integrity Zome)

```rust
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(OpEntry::CreateEntry { app_entry, action }) => {
            match app_entry {
                EntryTypes::NodeAttestation(att) => validate_node_attestation(&att, &action),
                EntryTypes::ModelManifest(m) => validate_model_manifest(&m),
                EntryTypes::ModelClaim(c) => validate_model_claim(&c),
                EntryTypes::Warrant(w) => validate_warrant(&w, &action),
                EntryTypes::WarrantRefutation(r) => validate_warrant_refutation(&r),
            }
        },
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_node_attestation(
    att: &NodeAttestation,
    action: &SignedActionHashed,
) -> ExternResult<ValidateCallbackResult> {
    // L'attestation doit être signée par l'agent qu'elle décrit
    if att.agent != action.action().author().clone() {
        return Ok(ValidateCallbackResult::Invalid(
            "L'attestation doit être publiée par l'agent qu'elle décrit".into()
        ));
    }
    // Vérifier la signature ed25519 interne (en plus de la signature Holochain)
    let content_hash = sha256(&att.content_bytes());
    if !verify_ed25519(&att.agent, &att.attestation_signature, &content_hash)? {
        return Ok(ValidateCallbackResult::Invalid(
            "Signature d'attestation invalide".into()
        ));
    }
    // VRAM déclarée cohérente
    if att.hardware_fingerprint.vram_total_bytes == 0 {
        return Ok(ValidateCallbackResult::Invalid("VRAM doit être > 0".into()));
    }
    // Benchmark récent (timestamp dans les 48h de l'action)
    let age = att.timestamp.checked_sub(att.benchmark_results.measured_at)
        .unwrap_or(Duration::MAX);
    if age > Duration::from_secs(172800) {
        return Ok(ValidateCallbackResult::Invalid(
            "Benchmark trop ancien (> 48h)".into()
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_model_manifest(m: &ModelManifest) -> ExternResult<ValidateCallbackResult> {
    if m.model_hash.len() != 32 {
        return Ok(ValidateCallbackResult::Invalid("model_hash doit être SHA256 (32 bytes)".into()));
    }
    if m.layer_count == 0 || m.hidden_size == 0 {
        return Ok(ValidateCallbackResult::Invalid("layer_count et hidden_size doivent être > 0".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_model_claim(c: &ModelClaim) -> ExternResult<ValidateCallbackResult> {
    // Vérifier que le manifeste référencé existe dans le DHT
    let manifest = get(c.manifest_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Manifeste introuvable".into())))?;
    let manifest_entry: ModelManifest = manifest.entry().to_app_option()?.unwrap();

    // Vérifier cohérence layer_range
    if let Some((start, end)) = c.layer_range {
        if end >= manifest_entry.layer_count {
            return Ok(ValidateCallbackResult::Invalid(
                "layer_range dépasse le nombre de couches du modèle".into()
            ));
        }
        if start > end {
            return Ok(ValidateCallbackResult::Invalid("layer_range invalide (start > end)".into()));
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_warrant(
    w: &Warrant,
    action: &SignedActionHashed,
) -> ExternResult<ValidateCallbackResult> {
    // L'accuser doit être l'auteur de l'entrée warrant
    if &w.accuser != action.action().author() {
        return Ok(ValidateCallbackResult::Invalid("Accuser != auteur du warrant".into()));
    }
    // Vérifier signature interne
    let content_hash = sha256(&w.content_bytes());
    if !verify_ed25519(&w.accuser, &w.accuser_signature, &content_hash)? {
        return Ok(ValidateCallbackResult::Invalid("Signature warrant invalide".into()));
    }
    // Explication non vide
    if w.rule_violated.is_empty() {
        return Ok(ValidateCallbackResult::Invalid("rule_violated ne peut pas être vide".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_warrant_refutation(r: &WarrantRefutation) -> ExternResult<ValidateCallbackResult> {
    if r.explanation.len() > 2048 {
        return Ok(ValidateCallbackResult::Invalid("Explication > 2048 chars".into()));
    }
    Ok(ValidateCallbackResult::Valid)
}
```

### Coordinator Zome — API Attestation

```rust
/// Publier l'attestation de ce nœud (appelé au démarrage + toutes les 24h)
#[hdk_extern]
pub fn publish_attestation(input: AttestationInput) -> ExternResult<ActionHash> {
    // Construire et signer l'attestation
    let agent = agent_info()?.agent_latest_pubkey;
    let att = NodeAttestation {
        agent: agent.clone(),
        timestamp: sys_time()?,
        hardware_fingerprint: input.hardware,
        benchmark_results: input.benchmark,
        holochain_version: input.holochain_version,
        daemon_version: env!("CARGO_PKG_VERSION").into(),
        attestation_signature: sign(agent, &input.content_bytes())?,
    };
    let hash = create_entry(EntryTypes::NodeAttestation(att))?;
    create_link(agent_info()?.agent_latest_pubkey, hash.clone(), LinkTypes::AgentToAttestation, ())?;
    Ok(hash)
}

/// Publier un manifeste de modèle
#[hdk_extern]
pub fn publish_model_manifest(manifest: ModelManifest) -> ExternResult<ActionHash> {
    let hash = create_entry(EntryTypes::ModelManifest(manifest.clone()))?;
    // Lier à l'anchor du model_id pour découverte
    let model_anchor = anchor("models", &manifest.model_id)?;
    create_link(model_anchor, hash.clone(), LinkTypes::ModelToManifest, ())?;
    Ok(hash)
}

/// Déclarer posséder un modèle (ModelClaim)
#[hdk_extern]
pub fn claim_model(input: ModelClaimInput) -> ExternResult<ActionHash> {
    let claim = ModelClaim {
        manifest_hash: input.manifest_hash.clone(),
        node: agent_info()?.agent_latest_pubkey,
        verified_locally: input.verified_locally,
        layer_range: input.layer_range,
        expert_ids: input.expert_ids,
        timestamp: sys_time()?,
    };
    let hash = create_entry(EntryTypes::ModelClaim(claim))?;
    create_link(input.manifest_hash, hash.clone(), LinkTypes::ManifestToClaim, ())?;
    Ok(hash)
}

/// Publier un warrant contre un pair (comportement invalide détecté)
#[hdk_extern]
pub fn publish_warrant(input: WarrantInput) -> ExternResult<ActionHash> {
    let agent = agent_info()?.agent_latest_pubkey;
    let expires_at = sys_time()?.checked_add(Duration::from_secs(30 * 24 * 3600))
        .ok_or(wasm_error!(WasmErrorInner::Guest("overflow".into())))?;
    let warrant = Warrant {
        accused: input.accused.clone(),
        accuser: agent.clone(),
        evidence: input.evidence_hash,
        rule_violated: input.rule_violated,
        timestamp: sys_time()?,
        expires_at,
        accuser_signature: sign(agent, &input.content_bytes())?,
    };
    let hash = create_entry(EntryTypes::Warrant(warrant))?;
    // Lier à l'accusé pour faciliter la recherche
    create_link(input.accused, hash.clone(), LinkTypes::AgentToWarrants, ())?;
    Ok(hash)
}

/// Contester un warrant (par l'accusé)
#[hdk_extern]
pub fn refute_warrant(input: RefutationInput) -> ExternResult<ActionHash> {
    let agent = agent_info()?.agent_latest_pubkey;
    let refutation = WarrantRefutation {
        warrant_hash: input.warrant_hash.clone(),
        refuted_by: agent.clone(),
        counter_evidence: input.counter_evidence,
        explanation: input.explanation,
        signature: sign(agent, &input.content_bytes())?,
    };
    let hash = create_entry(EntryTypes::WarrantRefutation(refutation))?;
    create_link(input.warrant_hash, hash.clone(), LinkTypes::WarrantToRefutation, ())?;
    Ok(hash)
}

/// Récupérer les warrants actifs d'un nœud
#[hdk_extern]
pub fn get_active_warrants(node: AgentPubKey) -> ExternResult<Vec<WarrantWithStatus>> {
    let links = get_links(
        GetLinksInputBuilder::try_new(node, LinkTypes::AgentToWarrants)?.build()
    )?;
    let now = sys_time()?;
    let mut result = Vec::new();

    for link in links {
        if let Some(record) = get(link.target.into_action_hash().unwrap(), GetOptions::default())? {
            if let Ok(Some(warrant)) = record.entry().to_app_option::<Warrant>() {
                // Filtrer les warrants expirés
                if warrant.expires_at > now {
                    // Vérifier s'il y a une réfutation valide
                    let refutations = get_warrant_refutations(&record.action_address())?;
                    result.push(WarrantWithStatus {
                        warrant,
                        refuted: !refutations.is_empty(),
                    });
                }
            }
        }
    }
    Ok(result)
}

/// Vérifier l'attestation d'un nœud avant connexion
#[hdk_extern]
pub fn verify_node_attestation(input: VerifyAttestationInput) -> ExternResult<AttestationStatus> {
    let links = get_links(
        GetLinksInputBuilder::try_new(input.node.clone(), LinkTypes::AgentToAttestation)?.build()
    )?;

    // Prendre l'attestation la plus récente
    let latest = links.last()
        .and_then(|l| get(l.target.into_action_hash()?, GetOptions::default()).ok()?)
        .and_then(|r| r.entry().to_app_option::<NodeAttestation>().ok()?);

    match latest {
        None => Ok(AttestationStatus::Missing),
        Some(att) => {
            let age = sys_time()?.checked_sub(att.timestamp).unwrap_or(Duration::MAX);
            if age > Duration::from_secs(86400) {
                return Ok(AttestationStatus::Expired { age_hours: age.as_secs() / 3600 });
            }
            // Vérifier les warrants actifs
            let warrants = get_active_warrants(input.node)?;
            let active_unrefuted: Vec<_> = warrants.iter().filter(|w| !w.refuted).collect();
            if !active_unrefuted.is_empty() {
                return Ok(AttestationStatus::Warranted { count: active_unrefuted.len() });
            }
            Ok(AttestationStatus::Valid { attestation: att })
        }
    }
}
```

---

## Membrane Proofs — Réseau Public et Privé

### Réseau public (défaut)

AInonymous est par défaut un **réseau public ouvert** : aucune membrane proof requise.

```rust
#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
```

La protection repose sur :
- Validation déterministe dans les integrity zomes
- Warrants : preuves cryptographiques d'invalidité → éjection automatique
- Scoring de réputation basé sur les métriques DHT

### Réseau privé (feature flag `private-network`)

Voir section **2.2 Bootstrap privé** dans `ARCHITECTURE.md` pour l'implémentation complète de `PrivateNetworkProof` et `genesis_self_check` avec vérification de membrane proof.

```rust
// Dans la DNA inference-mesh, mode privé
#[hdk_extern]
pub fn genesis_self_check(data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    #[cfg(feature = "private-network")]
    {
        let proof: PrivateNetworkProof = data.membrane_proof
            .ok_or(wasm_error!(WasmErrorInner::Guest("Membrane proof requise".into())))?
            .try_into()?;
        // Vérification admin + signature + expiration → voir ARCHITECTURE.md §2.2
        verify_private_network_proof(&proof, &data.agent_key)?;
        Ok(ValidateCallbackResult::Valid)
    }
    #[cfg(not(feature = "private-network"))]
    Ok(ValidateCallbackResult::Valid)
}
```

---

## Processus d'Audit Holochain

### Cycle de vie d'un warrant

```
1. DÉTECTION
   └── Pair A observe comportement invalide de Nœud B
       (hash modèle incorrect, entrée invalide, benchmark falsifié, etc.)

2. GÉNÉRATION
   └── A crée Warrant{accused: B, evidence: hash_entrée_invalide, rule: "..."}
   └── A signe le warrant avec sa clé ed25519
   └── Validation Holochain : signature vérifiée par les pairs avant acceptation DHT

3. PUBLICATION
   └── Warrant créé dans la source chain de A → propagé dans le DHT
   └── Lien AgentToWarrants(B) → Warrant publié (découvrable par tous)

4. PROPAGATION & VÉRIFICATION
   └── Pairs voisins reçoivent le warrant via gossip DHT
   └── Chaque pair vérifie indépendamment (integrity zome : validate_warrant)
   └── Si invalid → rejeté localement (pas de propagation)

5. EFFET
   └── Nœuds qui voient le warrant excluent B de leurs plans d'exécution
   └── verify_node_attestation(B) retourne AttestationStatus::Warranted
   └── Score de réputation de B dégradé proportionnellement

6. CONTESTATION (optionnel — par B)
   └── B publie WarrantRefutation{warrant_hash, counter_evidence, explanation}
   └── Les pairs évaluent les deux preuves indépendamment
   └── Si réfutation valide : get_active_warrants filtre ce warrant comme "refuted"
   └── Score de réputation restauré progressivement

7. EXPIRATION
   └── Warrant ignoré après expires_at (30 jours par défaut)
   └── B peut regagner sa réputation via nouvelles métriques positives
```

### Audit automatique (daemon)

```rust
// Lancé toutes les 6h par ainonymous-daemon
pub async fn run_periodic_audit(config: &AuditConfig) -> anyhow::Result<()> {
    let peers = get_all_known_nodes().await?;

    for peer in &peers {
        // 1. Vérifier attestation
        match verify_node_attestation(peer).await? {
            AttestationStatus::Missing => {
                log::warn!("Nœud {} sans attestation — ignoré du routage", peer);
            },
            AttestationStatus::Expired { age_hours } => {
                log::warn!("Attestation expirée ({} h) pour {}", age_hours, peer);
            },
            AttestationStatus::Warranted { count } => {
                log::warn!("{} warrant(s) actifs pour {}", count, peer);
            },
            AttestationStatus::Valid { .. } => {}
        }

        // 2. Vérifier cohérence des ModelClaims
        if config.model_hash_check {
            check_model_claims_consistency(peer).await?;
        }

        // 3. Publier warrant si incohérence détectée
        if config.auto_warrant {
            if let Some(violation) = detect_violations(peer).await? {
                publish_warrant(WarrantInput {
                    accused: peer.clone(),
                    evidence_hash: violation.evidence,
                    rule_violated: violation.rule,
                    content_bytes: violation.to_bytes(),
                }).await?;
            }
        }
    }
    Ok(())
}
```

---

## Résumé des DNAs et Zomes

| DNA | Integrity Zome | Coordinator Zome | Rôle |
|---|---|---|---|
| `inference-mesh` | Types: Request, Chunk, Metrics | submit, route, execute, publish_metrics | Cœur inférence distribuée |
| `agent-registry` | Types: Capabilities, Heartbeat, LoadedModel | announce, heartbeat, get_available_nodes | Annuaire des nœuds |
| `blackboard` | Types: BlackboardPost | post, search, get_recent_posts | Collaboration agents |
| `attestation` | Types: NodeAttestation, ModelManifest, ModelClaim, Warrant, WarrantRefutation | publish_attestation, claim_model, publish_warrant, refute_warrant, verify_node_attestation | Sécurité et audit |
