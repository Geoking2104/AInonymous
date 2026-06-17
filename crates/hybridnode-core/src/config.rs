use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeMode {
    Hybridnode,
    HolochainOnly,
    SdwanOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridNodeConfig {
    pub version: String,
    pub mode: NodeMode,
    pub identity: IdentityConfig,
    pub holochain: HolochainConfig,
    pub sdwan: SdwanConfig,
    pub quic: QuicConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub inference: InferenceConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    pub backend: String,
    pub keystore: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolochainConfig {
    pub conductor_url: String,
    pub app_port: u16,
    pub version: String,
    #[serde(default = "default_bootstrap_mode")]
    pub bootstrap_mode: String,
    pub bootstrap_url: Option<String>,
}

fn default_bootstrap_mode() -> String {
    "public".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdwanConfig {
    pub provider: String,
    pub api_url: Option<String>,
    pub api_token_env: Option<String>,
    pub site_id_env: Option<String>,
    #[serde(default = "default_true")]
    pub tls_verify: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u64,
    #[serde(default)]
    pub sla_thresholds: SlaThresholds,
}

fn default_true() -> bool { true }
fn default_poll_interval() -> u64 { 60 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlaThresholds {
    #[serde(default)]
    pub max_latency_ms: LatencyThresholds,
    #[serde(default)]
    pub min_bandwidth_mbps: BandwidthThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyThresholds {
    pub intra_site: f64,
    pub inter_site_local: f64,
    pub inter_site_remote: f64,
}

impl Default for LatencyThresholds {
    fn default() -> Self {
        Self { intra_site: 5.0, inter_site_local: 20.0, inter_site_remote: 80.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthThresholds {
    pub intra_site: f64,
    pub inter_site_local: f64,
    pub inter_site_remote: f64,
}

impl Default for BandwidthThresholds {
    fn default() -> Self {
        Self { intra_site: 10000.0, inter_site_local: 1000.0, inter_site_remote: 100.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    #[serde(default = "default_true")]
    pub mtls_strict: bool,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_true")]
    pub relay_fallback: bool,
    #[serde(default = "default_dscp")]
    pub dscp_marking: u8,
}

fn default_bind_addr() -> String { "0.0.0.0:0".to_string() }
fn default_dscp() -> u8 { 46 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_strategy")]
    pub default_strategy: String,
    #[serde(default = "default_max_activation")]
    pub max_inter_site_activation_mb: u64,
    #[serde(default = "default_max_nodes")]
    pub max_nodes_per_plan: u32,
}

fn default_strategy() -> String { "local_first".to_string() }
fn default_max_activation() -> u64 { 50 }
fn default_max_nodes() -> u32 { 3 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    #[serde(default = "default_llama_port")]
    pub llama_server_port: u16,
    #[serde(default = "default_localhost")]
    pub llama_server_host: String,
    #[serde(default = "default_llama_port")]
    pub api_port: u16,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            llama_server_port: 9337,
            llama_server_host: "127.0.0.1".to_string(),
            api_port: 9337,
            metrics_port: 9338,
        }
    }
}

fn default_llama_port() -> u16 { 9337 }
fn default_metrics_port() -> u16 { 9338 }
fn default_localhost() -> String { "127.0.0.1".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_true")]
    pub prometheus: bool,
    #[serde(default = "default_prometheus_addr")]
    pub prometheus_addr: String,
    #[serde(default)]
    pub otel_endpoint: Option<String>,
    #[serde(default = "default_service_name")]
    pub otel_service_name: String,
    #[serde(default)]
    pub log_level: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            prometheus: true,
            prometheus_addr: "0.0.0.0:9338".to_string(),
            otel_endpoint: None,
            otel_service_name: "hybridnode".to_string(),
            log_level: "info".to_string(),
        }
    }
}

fn default_prometheus_addr() -> String { "0.0.0.0:9338".to_string() }
fn default_service_name() -> String { "hybridnode".to_string() }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_audit_interval")]
    pub interval_hours: u64,
    #[serde(default = "default_true")]
    pub auto_warrant: bool,
}

fn default_audit_interval() -> u64 { 6 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub private_network: bool,
    #[serde(default)]
    pub pow_difficulty: u32,
    #[serde(default = "default_warrant_expiry")]
    pub warrant_expiry_days: u32,
}

fn default_warrant_expiry() -> u32 { 30 }

/// Load configuration from a YAML file.
pub fn load_config(path: &str) -> Result<HybridNodeConfig> {
    let content = std::fs::read_to_string(Path::new(path))?;
    let config: HybridNodeConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}
