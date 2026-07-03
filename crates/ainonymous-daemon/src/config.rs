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
    /// Pairs connus pour le bootstrap statique du testnet (plan de contrôle
    /// hors Holochain). Vide = mode solo / découverte DHT uniquement.
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    /// Plan d'exécution statique (testnet, sans Holochain) : tranches de couches
    /// par étage. Vide = on s'appuie sur le plan calculé par Holochain.
    #[serde(default)]
    pub pipeline_stages: Vec<PipelineStageConfig>,
    /// Adresse QUIC à annoncer aux pairs (ex: "127.0.0.1:9000" en loopback, ou
    /// l'IP publique du nœud). Absent = adresse locale du listener (0.0.0.0:port).
    #[serde(default)]
    pub quic_advertise: Option<String>,
    /// Intégration Holochain : bascule bootstrap statique ↔ conducteur réel.
    #[serde(default)]
    pub holochain: HolochainConfig,
}

/// Choix du plan de contrôle Holochain.
///
/// - `Static` (défaut) : bootstrap sans conducteur (config `peers`/`pipeline_stages`).
/// - `Conductor` : conducteur Holochain réel via `holochain_client::AppWebsocket`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HolochainBackendKind {
    #[default]
    Static,
    Conductor,
}

/// Paramètres de connexion au conducteur Holochain (mode `conductor`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolochainConfig {
    #[serde(default)]
    pub backend: HolochainBackendKind,
    /// Port de l'interface admin du conducteur (émission du token d'app +
    /// autorisation des credentials de signature des appels de zome).
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    /// Port de l'app interface du conducteur (appels de zome).
    #[serde(default = "default_conductor_app_port")]
    pub app_port: u16,
    /// Chemin du fichier de seed ed25519 du nœud. Par défaut :
    /// `$XDG_DATA_HOME/ainonymous/node_identity.key` (Linux/macOS)
    /// ou `%LOCALAPPDATA%\ainonymous\node_identity.key` (Windows).
    /// Permet de lancer plusieurs daemons sur une même machine avec des identités
    /// distinctes (ex: testnet loopback).
    #[serde(default)]
    pub identity_path: Option<PathBuf>,
    /// URL du daemon lair-keystore pour le stockage HSM de la seed (palier F).
    ///
    /// Format : `unix:///path/to/lair.sock` ou `ws://127.0.0.1:55000`.
    /// Si absent ou si lair est injoignable, repli sur le keyring OS natif
    /// (feature `secure-keyring`) puis sur le fichier `identity_path`.
    ///
    /// # Intégration future (palier F)
    /// ```text
    /// LairClient::connect(lair_url)
    ///   .new_seed("ainonymous-quic-node-identity", secret, exportable=true)
    ///   .get_entry(tag) → seed bytes → NodeIdentity::from_seed()
    /// ```
    #[serde(default)]
    pub lair_url: Option<String>,
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
        }
    }
}

/// Un étage du pipeline pour le plan statique de testnet. Le `quic_endpoint`
/// est résolu via l'entrée `peers` correspondante (par `agent_id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStageConfig {
    pub agent_id: String,
    pub layer_start: u32,
    pub layer_end: u32,
}

/// Pair statique du mesh (testnet). Permet la négociation de session QUIC
/// daemon↔daemon sans dépendre du DHT Holochain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Identifiant logique du pair (ex: AgentPubKey hex, ou alias testnet)
    pub agent_id: String,
    /// URL du daemon REST du pair, ex: "http://192.168.1.20:8889"
    pub daemon_url: String,
    /// Endpoint QUIC public du pair, ex: "192.168.1.20:9000" (optionnel,
    /// sinon fourni par la réponse de négociation)
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
    /// Nombre de tokens brouillon pour le décodage spéculatif (T2.4).
    /// 0 = désactivé (décodage classique 1 token/passe).
    /// Valeurs typiques : 3–8 (trade-off latence réseau / acceptance rate).
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
            // Créer le répertoire config si besoin
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            tracing::info!("Config par défaut créée: {:?}", config_path);
            Ok(config)
        }
    }

    /// Chemin du fichier de config. La variable d'environnement `AINON_CONFIG`
    /// permet de lancer plusieurs daemons sur une même machine (testnet loopback).
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
                speculative_k: 0,  // désactivé par défaut
            },
            peers: Vec::new(),
            pipeline_stages: Vec::new(),
            quic_advertise: None,
            holochain: HolochainConfig::default(),
        }
    }
}
