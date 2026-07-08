use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub daemon_port: u16,
    pub quic_port: u16,
    pub llama_server_port: u16,
    pub pipeline_server_port: u16,
    pub llama_server_bin: String,
    pub models_dir: PathBuf,
    pub holochain_conductor_url: String,
    pub holochain_app_id: String,
    pub region_hint: Option<String>,
    pub max_concurrent_requests: u8,
    pub network: NetworkConfig,
    pub inference: InferenceConfig,
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    #[serde(default)]
    pub pipeline_stages: Vec<PipelineStageConfig>,
    #[serde(default)]
    pub quic_advertise: Option<String>,
    #[serde(default)]
    pub holochain: HolochainConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HolochainBackendKind {
    #[default]
    Static,
    Conductor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolochainConfig {
    #[serde(default)]
    pub backend: HolochainBackendKind,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    #[serde(default = "default_conductor_app_port")]
    pub app_port: u16,
    #[serde(default)]
    pub identity_path: Option<PathBuf>,
    #[serde(default)]
    pub lair_url: Option<String>,

    /// Membrane Proof pour rejoindre un réseau privé / consortium (Palier F).
    ///
    /// Peut être fourni sous forme de bytes (base64 dans le TOML) ou via un fichier.
    /// Utilisé lors de l'installation de l'app ou pour les appels de zome
    /// qui exigent une preuve d'appartenance au réseau privé.
    #[serde(default)]
    pub membrane_proof: Option<MembraneProofConfig>,
}

/// Configuration d'un Membrane Proof (ou PrivateNetworkProof).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MembraneProofConfig {
    /// Preuve fournie directement en base64
    Base64(String),
    /// Chemin vers un fichier contenant la preuve (binaire ou base64)
    File { path: PathBuf },
}

impl MembraneProofConfig {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        match self {
            MembraneProofConfig::Base64(b64) => {
                use base64::{engine::general_purpose, Engine as _};
                general_purpose::STANDARD.decode(b64).map_err(|e| anyhow::anyhow!("invalid base64 membrane proof: {e}"))
            }
            MembraneProofConfig::File { path } => {
                std::fs::read(path).map_err(|e| anyhow::anyhow!("failed to read membrane proof file: {e}"))
            }
        }
    }
}

fn default_admin_port() -> u16 { 8888 }
fn default_conductor_app_port() -> u16 { 8890 }

impl Default for HolochainConfig {
    fn default() -> Self {
        Self {
            backend: HolochainBackendKind::Static,
            admin_port: default_admin_port(),
            app_port: default_conductor_app_port(),
            identity_path: None,
            lair_url: None,
            membrane_proof: None,
        }
    }
}

// ... (le reste du fichier config.rs reste inchangé : PipelineStageConfig, PeerConfig, NetworkConfig, etc.)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStageConfig {
    pub agent_id: String,
    pub layer_start: u32,
    pub layer_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub agent_id: String,
    pub daemon_url: String,
    #[serde(default)]
    pub quic_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub quic_relay_fallback: bool,
    pub activation_compression: CompressionMode,
    pub compression_threshold_gbps: f64,
    pub max_activation_size_mb: usize,
    pub max_concurrent_quic_sessions: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionMode {
    Auto,
    Zstd,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    pub default_model: String,
    pub context_size: u32,
    pub n_gpu_layers: i32,
    pub flash_attention: bool,
    pub kv_cache_type: String,
    pub parallel_requests: u8,
    #[serde(default)]
    pub speculative_k: u8,
}

impl DaemonConfig {
    pub fn load() -> Result<Self> {
        let config_path = Self::resolve_config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            let config = Self::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, toml::to_string_pretty(&config)?);
            tracing::info!("Config par défaut créée: {:?}", config_path);
            Ok(config);
        }
    }

    pub fn resolve_config_path() -> PathBuf {
        if let Ok(p) = std::env::var("AINON_CONFIG") {
            return PathBuf::from(p);
        }
        Self::default_config_path()
    }

    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ainonymous")
            .join("config.toml")
    }

    pub fn config_path(&self) -> PathBuf {
        Self::resolve_config_path()
    }

    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }
}