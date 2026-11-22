use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Deployment policy supplied by the process environment.
    #[serde(skip)]
    pub conductor_only: bool,
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
    Static,
    #[default]
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

    /// Membrane proof used while installing a private-network hApp.
    ///
    /// It can be supplied as base64 or read from a file. A membrane proof is a
    /// genesis/install-time credential; it is not injected into ordinary zome calls.
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
                general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| anyhow::anyhow!("invalid base64 membrane proof: {e}"))
            }
            MembraneProofConfig::File { path } => std::fs::read(path)
                .map_err(|e| anyhow::anyhow!("failed to read membrane proof file: {e}")),
        }
    }
}

fn default_admin_port() -> u16 {
    8888
}
fn default_conductor_app_port() -> u16 {
    8889
}

impl Default for HolochainConfig {
    fn default() -> Self {
        Self {
            backend: HolochainBackendKind::Conductor,
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
    /// mlock la mémoire du modèle (évite le swap). Activé automatiquement sur
    /// GPU quel que soit ce champ (cf. llama.rs::LlamaManager::start).
    #[serde(default)]
    pub mlock: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            quic_relay_fallback: true,
            activation_compression: CompressionMode::Auto,
            compression_threshold_gbps: 1.0,
            max_activation_size_mb: 512,
            max_concurrent_quic_sessions: 8,
        }
    }
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            default_model: "gemma4-e4b".to_string(),
            context_size: 8192,
            n_gpu_layers: -1,
            flash_attention: true,
            kv_cache_type: "q8_0".to_string(),
            parallel_requests: 1,
            speculative_k: 0,
            mlock: false,
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            conductor_only: false,
            daemon_port: 8890,
            quic_port: 9000,
            llama_server_port: 8081,
            pipeline_server_port: 8180,
            llama_server_bin: "llama-server".to_string(),
            models_dir: PathBuf::from("models"),
            holochain_conductor_url: "ws://127.0.0.1:8888".to_string(),
            holochain_app_id: "ainonymous".to_string(),
            region_hint: None,
            max_concurrent_requests: 4,
            network: NetworkConfig::default(),
            inference: InferenceConfig::default(),
            peers: Vec::new(),
            pipeline_stages: Vec::new(),
            quic_advertise: None,
            holochain: HolochainConfig::default(),
        }
    }
}

impl DaemonConfig {
    pub fn load() -> Result<Self> {
        let config_path = Self::resolve_config_path();

        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content)?
        } else {
            let config = Self::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            tracing::info!("Config par défaut créée: {:?}", config_path);
            config
        };
        if let Some(policy) = std::env::var_os("AINON_DISCOVERY_POLICY") {
            anyhow::ensure!(policy == "conductor-only", "unknown AINON_DISCOVERY_POLICY");
            config.enable_conductor_only()?;
        }
        Ok(config)
    }

    fn enable_conductor_only(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.holochain.backend == HolochainBackendKind::Conductor
                && self.peers.is_empty()
                && self.pipeline_stages.is_empty(),
            "conductor-only discovery requires the conductor backend and empty peers and pipeline_stages"
        );
        self.conductor_only = true;
        Ok(())
    }

    /// Apply the deployment policy after configuring timeouts for each local runtime.
    pub fn build_runtime_http(
        &self,
        mut builder: reqwest::ClientBuilder,
    ) -> reqwest::Result<reqwest::Client> {
        if self.conductor_only {
            builder = builder.no_proxy().redirect(reqwest::redirect::Policy::none());
        }
        builder.build()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Les configs de test référencées par la doc doivent rester conformes
    /// au schéma `DaemonConfig` (régression : daemon_t51.toml ne se chargeait plus).
    #[test]
    fn testnet_configs_parse() {
        for (name, src) in [
            ("daemon_t41.toml", include_str!("../../../scripts/testnet/daemon_t41.toml")),
            ("daemon_t51.toml", include_str!("../../../scripts/testnet/daemon_t51.toml")),
        ] {
            let cfg: DaemonConfig =
                toml::from_str(src).unwrap_or_else(|e| panic!("{name} invalide : {e}"));
            assert_ne!(cfg.daemon_port, cfg.holochain.app_port, "{name}");
        }
        let t51: DaemonConfig =
            toml::from_str(include_str!("../../../scripts/testnet/daemon_t51.toml")).unwrap();
        assert_eq!(t51.daemon_port, 18890);
        assert_eq!(t51.holochain.backend, HolochainBackendKind::Conductor);
    }

    #[test]
    fn private_policy_rejects_static_config_and_disables_the_implicit_solo_plan() {
        let mut config = DaemonConfig::default();
        assert!(crate::conductor::static_plan_from_config(&config).is_some());
        config.enable_conductor_only().unwrap();
        assert!(crate::conductor::static_plan_from_config(&config).is_none());
        assert!(!toml::to_string(&config).unwrap().contains("conductor_only"));

        let mut static_config: DaemonConfig =
            toml::from_str(include_str!("../../../scripts/testnet/daemon_t41.toml")).unwrap();
        assert!(static_config.enable_conductor_only().is_err());
        let mut config = DaemonConfig::default();
        config.peers.push(PeerConfig {
            agent_id: "worker".into(), daemon_url: "http://localhost".into(), quic_endpoint: None,
        });
        assert!(config.enable_conductor_only().is_err());
        config.peers.clear();
        config.pipeline_stages.push(PipelineStageConfig {
            agent_id: "worker".into(), layer_start: 0, layer_end: 7,
        });
        assert!(config.enable_conductor_only().is_err());
    }

    #[test]
    fn default_ports_do_not_overlap() {
        let config = DaemonConfig::default();

        assert_eq!(config.holochain.admin_port, 8888);
        assert_eq!(config.holochain.app_port, 8889);
        assert_eq!(config.daemon_port, 8890);
        assert_ne!(config.holochain.admin_port, config.holochain.app_port);
        assert_ne!(config.holochain.app_port, config.daemon_port);
    }
}
