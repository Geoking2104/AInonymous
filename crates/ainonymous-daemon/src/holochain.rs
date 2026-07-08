use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use ainonymous_types::{ExecutionPlan, NodeHeartbeat};
use crate::config::{DaemonConfig, MembraneProofConfig};
use crate::conductor_client::ConductorClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSummary {
    pub agent_id: String,
    pub vram_gb: f32,
    pub current_load: f32,
    pub available_slots: u8,
    pub quic_endpoint: Option<String>,
    pub region_hint: Option<String>,
    pub score: f32,
    #[serde(default)]
    pub node_pubkey: Option<String>,
}

#[derive(Clone)]
enum Backend {
    Static,
    Conductor(Arc<ConductorClient>),
}

#[derive(Clone)]
pub struct HolochainClient {
    app_port: u16,
    app_id: String,
    http: reqwest::Client,
    peers: Vec<crate::config::PeerConfig>,
    backend: Backend,
    membrane_proof: Option<Vec<u8>>,
}

impl HolochainClient {
    pub async fn connect(config: &DaemonConfig) -> Result<Self> {
        let membrane_proof = config.holochain.membrane_proof.clone();

        let backend = match config.holochain.backend {
            crate::config::HolochainBackendKind::Static => Backend::Static,
            crate::config::HolochainBackendKind::Conductor => {
                match ConductorClient::connect(
                    config.holochain.admin_port,
                    config.holochain.app_port,
                    &config.holochain_app_id,
                    membrane_proof.clone(),
                )
                .await
                {
                    Ok(c) => Backend::Conductor(Arc::new(c)),
                    Err(e) => {
                        warn!("Conducteur Holochain injoignable ({e}) — repli sur bootstrap statique");
                        Backend::Static
                    }
                }
            }
        };

        let proof_bytes = membrane_proof.and_then(|p| p.to_bytes().ok());

        let client = Self {
            app_port: config.daemon_port,
            app_id: config.holochain_app_id.clone(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            peers: config.peers.clone(),
            backend,
            membrane_proof: proof_bytes,
        };
        Ok(client);
    }

    pub fn membrane_proof(&self) -> Option<&[u8]> {
        self.membrane_proof.as_deref()
    }

    pub async fn listen_quic_signals(
        &self,
        registry: ainonymous_quic::SessionRegistry,
        advertise: SocketAddr,
        identity: ainonymous_quic::NodeIdentity,
    ) {
        match &self.backend {
            Backend::Conductor(c) => c.listen_quic_signals(registry, advertise, identity).await,
            Backend::Static => {
                debug!("Signaux QUIC Holochain ignorés (backend statique)");
            }
        }
    }

    fn peer_daemon_url(&self, agent_id: &str) -> Option<String> {
        self.peers.iter()
            .find(|p| p.agent_id == agent_id)
            .map(|p| p.daemon_url.clone())
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.app_port)
    }

    pub async fn zome_call(
        &self,
        dna: &str,
        zome: &str,
        function: &str,
        payload: Value,
    ) -> Result<Value> {
        debug!("Zome call: {}::{}::{}", dna, zome, function);

        match &self.backend {
            Backend::Conductor(c) => c.call_zome_json(dna, zome, function, payload).await,
            Backend::Static => {
                let resp = self.http
                    .post(format!("{}/zome/{}/{}/{}", self.base_url(), dna, zome, function))
                    .json(&payload)
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let body = resp.text().await?;
                    anyhow::bail!("Zome call {}::{}::{} échouée: {}", dna, zome, function, body);
                }

                Ok(resp.json().await?);
            }
        }
    }

    pub async fn announce_capabilities(
        &self,
        config: &DaemonConfig,
        node_pubkey_hex: Option<&str>,
    ) -> Result<()> {
        let mut caps = detect_local_capabilities(config);
        caps.node_pubkey = node_pubkey_hex.map(|s| s.to_string());

        self.zome_call(
            "agent-registry",
            "coordinator",
            "announce_capabilities",
            serde_json::to_value(&caps)?,
        ).await?;

        info!("Capacités annoncées: {:.1}GB VRAM, node_pubkey: {}",
            caps.vram_gb,
            node_pubkey_hex.unwrap_or("<non fournie>"));
        Ok(());
    }

    pub async fn send_heartbeat(&self, hb: NodeHeartbeat) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "heartbeat",
            serde_json::to_value(&hb)?,
        ).await?;
        Ok(());
    }

    pub async fn get_execution_plan(&self, model_id: &str) -> Result<ExecutionPlan> {
        let resp = self.zome_call(
            "inference-mesh",
            "coordinator",
            "compute_execution_plan",
            json!({ "model_id": model_id }),
        ).await?;

        Ok(serde_json::from_value(resp)?);
    }

    pub async fn get_available_nodes(&self, model_id: &str) -> Result<Vec<NodeSummary>> {
        let resp = self.zome_call(
            "agent-registry",
            "coordinator",
            "get_available_nodes",
            json!(model_id),
        ).await?;

        Ok(serde_json::from_value(resp)?);
    }

    pub async fn negotiate_quic_session(
        &self,
        target_agent: &str,
        layer_range: Option<(u32, u32)>,
        next_agent: Option<String>,
        next_layer_range: Option<(u32, u32)>,
        requester_pubkey: Option<[u8; 32]>,
    ) -> Result<ainonymous_quic::SessionOffer> {
        match &self.backend {
            Backend::Conductor(c) => {
                let result = c
                    .call_zome_json(
                        "inference-mesh",
                        "coordinator",
                        "request_remote_session",
                        json!({
                            "target": target_agent,
                            "layer_range": layer_range,
                            "next_agent_id": next_agent.clone(),
                            "next_layer_range": next_layer_range,
                            "requester_pubkey": requester_pubkey,
                        }),
                    )
                    .await?;

                let endpoint: SocketAddr = result["quic_endpoint"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("réponse de négociation sans quic_endpoint"))?
                    .parse()?
                    ;
                let token: Vec<u8> = serde_json::from_value(result["session_token"].clone())?;

                let peer_pubkey: Option<[u8; 32]> = result["node_pubkey"]
                    .as_str()
                    .and_then(|hex_str| hex::decode(hex_str).ok())
                    .and_then(|b| b.try_into().ok());

                let mut offer = ainonymous_quic::SessionOffer::new(endpoint, layer_range);
                offer.session_token = token;
                offer.next_agent_id = next_agent;
                offer.next_layer_range = next_layer_range;
                offer.peer_pubkey = peer_pubkey;
                Ok(offer)
            }
            Backend::Static => {
                let daemon_url = self.peer_daemon_url(target_agent).ok_or_else(|| {
                    anyhow::anyhow!("Pair '{}' introuvable dans la config bootstrap", target_agent)
                })?;

                let resp = self.http
                    .post(format!("{}/mesh/session/negotiate", daemon_url))
                    .json(&json!({
                        "layer_range": layer_range,
                        "next_agent_id": next_agent,
                        "next_layer_range": next_layer_range,
                        "requester_pubkey": requester_pubkey,
                    }))
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    anyhow::bail!("Négociation refusée par {}: {}", target_agent, body);
                }

                Ok(resp.json::<ainonymous_quic::SessionOffer>().await?);
            }
        }
    }

    pub async fn reannounce_pubkey(
        &self,
        new_pubkey_hex: &str,
        config: &DaemonConfig,
    ) -> Result<()> {
        self.announce_capabilities(config, Some(new_pubkey_hex)).await?;
        info!("DHT : nouvelle clé publique annoncée après rotation : {}", new_pubkey_hex);
        Ok(());
    }

    pub async fn update_quic_endpoint(&self, addr: SocketAddr) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "update_quic_endpoint",
            json!({ "endpoint": addr.to_string() }),
        ).await?;
        Ok(());
    }

    pub async fn blackboard_post(&self, prefix: &str, content: &str, tags: Vec<String>) -> Result<()> {
        self.zome_call(
            "blackboard",
            "coordinator",
            "post",
            json!({
                "prefix": prefix,
                "content": content,
                "tags": tags,
                "ttl_hours": 48,
            }),
        ).await?;
        Ok(());
    }

    pub async fn blackboard_search(
        &self,
        terms: Vec<String>,
        prefix_filter: Option<String>,
    ) -> Result<Value> {
        self.zome_call(
            "blackboard",
            "coordinator",
            "search",
            json!({
                "terms": terms,
                "prefix_filter": prefix_filter,
                "limit": 20,
            }),
        ).await
    }
}

fn detect_local_capabilities(config: &DaemonConfig) -> ainonymous_types::NodeCapabilities {
    let (gpu_vendor, vram_gb) = detect_gpu();

    ainonymous_types::NodeCapabilities {
        agent_id: "local".into(),
        vram_gb,
        ram_gb: get_total_ram_gb(),
        gpu_vendor: gpu_vendor.clone(),
        compute_backends: detect_compute_backends(&gpu_vendor),
        loaded_models: vec![],
        max_concurrent_requests: config.max_concurrent_requests,
        network_bandwidth_mbps: None,
        region_hint: config.region_hint.clone(),
        quic_endpoint: None,
        node_pubkey: None,
    }
}

fn detect_gpu() -> (ainonymous_types::GpuVendor, f32) {
    #[cfg(target_os = "macos")]
    {
        let ram_gb = get_total_ram_gb();
        return (ainonymous_types::GpuVendor::AppleSilicon, ram_gb);
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some((vram_gb, compute_capability)) = detect_nvidia() {
            return (ainonymous_types::GpuVendor::Nvidia { vram_gb, compute_capability }, vram_gb);
        }
        if let Some(vram_gb) = detect_amd() {
            return (ainonymous_types::GpuVendor::Amd { vram_gb }, vram_gb);
        }
        (ainonymous_types::GpuVendor::CpuOnly, 0.0)
    }
}

#[cfg(not(target_os = "macos"))]
fn detect_nvidia() -> Option<(f32, String)> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total,compute_cap", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?;
    let mut parts = line.split(',').map(|s| s.trim());
    let mem_mib: f32 = parts.next()?.parse().ok()?;
    let cc = parts.next().unwrap_or("").to_string();
    Some((mem_mib / 1024.0, cc))
}

#[cfg(not(target_os = "macos"))]
fn detect_amd() -> Option<f32> {
    let out = std::process::Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--csv"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 {
            if let Ok(total_bytes) = parts[2].trim().parse::<u64>() {
                return Some(total_bytes as f32 / (1024.0 * 1024.0 * 1024.0));
            }
        }
    }
    None
}

fn get_total_ram_gb() -> f32 {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok();
        if let Some(out) = out {
            if let Ok(s) = String::from_utf8(out.stdout) {
                if let Ok(bytes) = s.trim().parse::<u64>() {
                    return bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    let kb: u64 = line.split_whitespace().nth(1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    return kb as f32 / (1024.0 * 1024.0);
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("wmic")
            .args(["computersystem", "get", "TotalPhysicalMemory", "/value"])
            .output()
            .ok();
        if let Some(out) = out {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(val) = line.strip_prefix("TotalPhysicalMemory=") {
                    if let Ok(bytes) = val.trim().parse::<u64>() {
                        return bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                    }
                }
            }
        }
    }
    8.0
}

fn detect_compute_backends(vendor: &ainonymous_types::GpuVendor) -> Vec<ainonymous_types::ComputeBackend> {
    use ainonymous_types::{ComputeBackend, GpuVendor};
    match vendor {
        GpuVendor::AppleSilicon => vec![ComputeBackend::Metal, ComputeBackend::Cpu],
        GpuVendor::Nvidia { .. } => vec![ComputeBackend::Cuda, ComputeBackend::Cpu],
        GpuVendor::Amd { .. } => vec![ComputeBackend::Hip, ComputeBackend::Cpu],
        GpuVendor::Intel { .. } => vec![ComputeBackend::Vulkan, ComputeBackend::Cpu],
        GpuVendor::CpuOnly => vec![ComputeBackend::Cpu],
    }
}
