use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub daemon_port: u16,
    pub quic_port: u16,
    pub llama_server_port: u16,
    /// Port du pipeline_server.py local (HuggingFace transformers)
    pub pipeline_server_port: u16,
    pub llama_server_bin: String,
    pub models_dir: PathBuf,
    pub holochain_conductor_url: String,
    pub holochain_app_id: String,
    pub region_hint: Option<String>,
    pub max_concurrent_requests: u8,
    pub network: NetworkConfig,
    pub inference: InferenceConfig,
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
}

impl DaemonConfig {
    pub fn load() -> Result<Self> {
        let config_path = Self::default_config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            let config = Self::default();
            // Créer le répertoire config si besoin
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            tracing::info!("Config par défaut créée: {:?}", config_path);
            Ok(config)
        }
    }

    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ainonymous")
            .join("config.toml")
    }

    pub fn config_path(&self) -> PathBuf {
        Self::default_config_path()
    }

    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let models_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".models");

        Self {
            daemon_port: 8889,
            quic_port: 0,        // 0 = port éphémère automatique
            llama_server_port: 8080,
            pipeline_server_port: 9340,
            llama_server_bin: "llama-server".into(),
            models_dir,
            holochain_conductor_url: "ws://127.0.0.1:8888".into(),
            holochain_app_id: "ainonymous-core".into(),
            region_hint: None,
            max_concurrent_requests: 4,
            network: NetworkConfig {
                quic_relay_fallback: true,
                activation_compression: CompressionMode::Auto,
                compression_threshold_gbps: 1.0,
                max_activation_size_mb: 512,
                max_concurrent_quic_sessions: 4,
            },
            inference: InferenceConfig {
                default_model: "gemma4-e4b".into(),
                context_size: 8192,
                n_gpu_layers: -1,  // -1 = toutes les couches sur GPU
                flash_attention: true,
                kv_cache_type: "q8_0".into(),
                parallel_requests: 4,
            },
        }
    }
}
