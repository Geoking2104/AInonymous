use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Identifiant d'un agent Holochain (clé publique ed25519, encodée hex)
pub type AgentId = String;

/// Capacités déclarées par un nœud du mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub agent_id: AgentId,
    pub vram_gb: f32,
    pub ram_gb: f32,
    pub gpu_vendor: GpuVendor,
    pub compute_backends: Vec<ComputeBackend>,
    pub loaded_models: Vec<LoadedModel>,
    pub max_concurrent_requests: u8,
    pub network_bandwidth_mbps: Option<u32>,
    pub region_hint: Option<String>,
    /// Adresse QUIC publique pour le canal de données
    pub quic_endpoint: Option<SocketAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    AppleSilicon,
    Nvidia { vram_gb: f32, compute_capability: String },
    Amd { vram_gb: f32 },
    Intel { vram_gb: f32 },
    CpuOnly,
}

impl GpuVendor {
    pub fn vram_gb(&self) -> f32 {
        match self {
            Self::AppleSilicon => 0.0, // shared memory
            Self::Nvidia { vram_gb, .. } => *vram_gb,
            Self::Amd { vram_gb } => *vram_gb,
            Self::Intel { vram_gb } => *vram_gb,
            Self::CpuOnly => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ComputeBackend {
    Metal,
    Cuda,
    Hip,
    Vulkan,
    Cpu,
}

/// Modèle actuellement chargé sur ce nœud
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModel {
    pub model_id: String,
    pub model_hash: String,           // SHA256 hex du fichier GGUF
    pub quantization: Quantization,
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>, // MoE : experts hébergés
    pub context_size: u32,
    pub ready: bool,
    pub tokens_per_second_avg: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Quantization {
    F32,
    F16,
    Bf16,
    Q8_0,
    Q6K,
    Q5KM,
    Q4KM,
    Q4_0,
    Q3KM,
    #[serde(rename = "IQ2_XXS")]
    Iq2Xxs,
    #[serde(untagged)]
    Other(String),
}

/// Heartbeat périodique envoyé par chaque nœud (toutes les 30s)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub agent_id: AgentId,
    pub current_load: f32,           // 0.0 (idle) à 1.0 (saturé)
    pub available_slots: u8,
    pub queue_depth: u32,
    pub memory_pressure: f32,
    pub temperature_c: Option<f32>,
    pub timestamp_ms: i64,
}

/// Score de sélection d'un nœud (plus élevé = préféré)
#[derive(Debug, Clone)]
pub struct NodeScore {
    pub agent_id: AgentId,
    pub score: f32,
    pub caps: NodeCapabilities,
    pub last_heartbeat_age_ms: i64,
}

impl NodeScore {
    pub fn compute(caps: &NodeCapabilities, hb: &NodeHeartbeat, prefer_region: Option<&str>) -> f32 {
        let vram_score = (caps.vram_gb / 80.0).min(1.0) * 30.0;
        let load_score = (1.0 - hb.current_load) * 40.0;
        let slots_score = (hb.available_slots as f32 / 8.0).min(1.0) * 20.0;
        let region_score = match (prefer_region, &caps.region_hint) {
            (Some(pref), Some(region)) if region.starts_with(pref) => 10.0,
            _ => 0.0,
        };
        vram_score + load_score + slots_score + region_score
    }
}

/// Plan d'exécution calculé par le routeur
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ExecutionPlan {
    /// Un seul nœud, modèle entier
    Solo {
        node: AgentId,
        quic_endpoint: SocketAddr,
    },
    /// N nœuds, couches réparties séquentiellement
    PipelineSplit {
        stages: Vec<PipelineStage>,
    },
    /// N nœuds MoE, experts répartis
    ExpertShard {
        stages: Vec<ExpertStage>,
        trunk_node: AgentId,
    },
    /// Décodage spéculatif : draft + verify
    Speculative {
        draft_node: AgentId,
        draft_endpoint: SocketAddr,
        verify_node: AgentId,
        verify_endpoint: SocketAddr,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub node: AgentId,
    pub quic_endpoint: SocketAddr,
    pub layer_start: u32,
    pub layer_end: u32,
    pub is_last: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertStage {
    pub node: AgentId,
    pub quic_endpoint: SocketAddr,
    pub expert_ids: Vec<u32>,
    pub has_trunk: bool,
}
