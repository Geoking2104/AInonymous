use std::net::SocketAddr;
use anyhow::Result;
use serde_json::{json, Value};
use tracing::{debug, info};

use ainonymous_types::{ExecutionPlan, NodeCapabilities, NodeHeartbeat};
use crate::DaemonConfig;

/// Client pour le conducteur Holochain (via WebSocket app port)
#[derive(Clone)]
pub struct HolochainClient {
    app_port: u16,
    app_id: String,
    http: reqwest::Client,
    /// Pairs statiques (bootstrap testnet, plan de contrôle hors DHT)
    peers: Vec<crate::config::PeerConfig>,
}

impl HolochainClient {
    pub async fn connect(config: &DaemonConfig) -> Result<Self> {
        let client = Self {
            app_port: config.daemon_port, // port du daemon qui fait l'interface avec Holochain
            app_id: config.holochain_app_id.clone(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            peers: config.peers.clone(),
        };
        Ok(client)
    }

    /// Résoudre l'URL du daemon REST d'un pair via la config bootstrap.
    fn peer_daemon_url(&self, agent_id: &str) -> Option<String> {
        self.peers.iter()
            .find(|p| p.agent_id == agent_id)
            .map(|p| p.daemon_url.clone())
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.app_port)
    }

    /// Appeler une zome function via le conducteur Holochain
    pub async fn zome_call(
        &self,
        dna: &str,
        zome: &str,
        function: &str,
        payload: Value,
    ) -> Result<Value> {
        debug!("Zome call: {}::{}::{}", dna, zome, function);

        // Dans une implémentation complète, on utiliserait holochain_client::AppWebsocket
        // Pour le MVP : on passe par le daemon REST interne
        let resp = self.http
            .post(format!("{}/zome/{}/{}/{}", self.base_url(), dna, zome, function))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await?;
            anyhow::bail!("Zome call {}::{}::{} échouée: {}", dna, zome, function, body);
        }

        Ok(resp.json().await?)
    }

    /// Annoncer les capacités de ce nœud dans le DHT
    pub async fn announce_capabilities(&self, config: &DaemonConfig) -> Result<()> {
        // Détecter les capacités GPU
        let caps = detect_local_capabilities(config);

        self.zome_call(
            "agent-registry",
            "coordinator",
            "announce_capabilities",
            serde_json::to_value(&caps)?,
        ).await?;

        info!("Capacités annoncées: {:.1}GB VRAM, modèles: {:?}",
            caps.vram_gb,
            caps.loaded_models.iter().map(|m| &m.model_id).collect::<Vec<_>>());
        Ok(())
    }

    /// Publier un heartbeat
    pub async fn send_heartbeat(&self, hb: NodeHeartbeat) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "heartbeat",
            serde_json::to_value(&hb)?,
        ).await?;
        Ok(())
    }

    /// Obtenir le plan d'exécution pour un modèle
    pub async fn get_execution_plan(&self, model_id: &str) -> Result<ExecutionPlan> {
        let resp = self.zome_call(
            "inference-mesh",
            "coordinator",
            "compute_execution_plan",
            json!({ "model_id": model_id }),
        ).await?;

        Ok(serde_json::from_value(resp)?)
    }

    /// Récupérer les nœuds disponibles pour un modèle
    pub async fn get_available_nodes(&self, model_id: &str) -> Result<Vec<NodeCapabilities>> {
        let resp = self.zome_call(
            "agent-registry",
            "coordinator",
            "get_available_nodes",
            json!(model_id),
        ).await?;

        Ok(serde_json::from_value(resp)?)
    }

    /// Négocier une session QUIC avec un nœud distant.
    ///
    /// Plan de contrôle bootstrap statique (testnet) : appelle directement le
    /// daemon REST du pair `target_agent` sur `POST /mesh/session/negotiate`.
    /// Le pair génère un token, enregistre l'offre dans son listener QUIC, et
    /// retourne l'offre (endpoint + token). Remplace l'ancien zome_call
    /// auto-référent. L'intégration Holochain réelle se branchera ici plus tard.
    pub async fn negotiate_quic_session(
        &self,
        target_agent: &str,
        layer_range: Option<(u32, u32)>,
        next_agent: Option<String>,
        next_layer_range: Option<(u32, u32)>,
    ) -> Result<ainonymous_quic::SessionOffer> {
        let daemon_url = self.peer_daemon_url(target_agent).ok_or_else(|| {
            anyhow::anyhow!(
                "Pair '{}' introuvable dans la config bootstrap (peers)",
                target_agent
            )
        })?;

        debug!("Négociation session QUIC → pair {} ({})", target_agent, daemon_url);

        let resp = self.http
            .post(format!("{}/mesh/session/negotiate", daemon_url))
            .json(&json!({
                "layer_range": layer_range,
                "next_agent_id": next_agent,
                "next_layer_range": next_layer_range,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Négociation refusée par {}: {}", target_agent, body);
        }

        Ok(resp.json::<ainonymous_quic::SessionOffer>().await?)
    }

    /// Mettre à jour l'endpoint QUIC publié dans le DHT
    pub async fn update_quic_endpoint(&self, addr: SocketAddr) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "update_quic_endpoint",
            json!({ "endpoint": addr.to_string() }),
        ).await?;
        Ok(())
    }

    /// Poster sur le Blackboard
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
        Ok(())
    }

    /// Rechercher dans le Blackboard
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

/// Détecter les capacités GPU/CPU du nœud local
fn detect_local_capabilities(config: &DaemonConfig) -> ainonymous_types::NodeCapabilities {
    // Détection basique — TODO: utiliser nvml, metal-rs, etc.
    let (gpu_vendor, vram_gb) = detect_gpu();

    ainonymous_types::NodeCapabilities {
        agent_id: "local".into(), // sera rempli par Holochain avec la vraie clé
        vram_gb,
        ram_gb: get_total_ram_gb(),
        gpu_vendor: gpu_vendor.clone(),
        compute_backends: detect_compute_backends(&gpu_vendor),
        loaded_models: vec![],
        max_concurrent_requests: config.max_concurrent_requests,
        network_bandwidth_mbps: None,
        region_hint: config.region_hint.clone(),
        quic_endpoint: None, // sera rempli après démarrage QUIC
    }
}

fn detect_gpu() -> (ainonymous_types::GpuVendor, f32) {
    #[cfg(target_os = "macos")]
    {
        // Apple Silicon : mémoire unifiée
        let ram_gb = get_total_ram_gb();
        return (ainonymous_types::GpuVendor::AppleSilicon, ram_gb);
    }

    #[cfg(not(target_os = "macos"))]
    {
        // NVIDIA via nvidia-smi (sans dépendance native nvml)
        if let Some((vram_gb, compute_capability)) = detect_nvidia() {
            return (
                ainonymous_types::GpuVendor::Nvidia { vram_gb, compute_capability },
                vram_gb,
            );
        }
        // AMD via rocm-smi
        if let Some(vram_gb) = detect_amd() {
            return (ainonymous_types::GpuVendor::Amd { vram_gb }, vram_gb);
        }
        (ainonymous_types::GpuVendor::CpuOnly, 0.0)
    }
}

/// VRAM (GiB) + compute capability de la 1ère GPU NVIDIA, via `nvidia-smi`.
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
    Some((mem_mib / 1024.0, cc)) // MiB → GiB
}

/// VRAM (GiB) de la 1ère GPU AMD, via `rocm-smi` (heuristique best-effort).
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
    // Plus grand entier de la sortie = total VRAM en octets (heuristique).
    let max_bytes = text
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u64>().ok())
        .max()?;
    if max_bytes < 1_000_000 {
        return None;
    }
    Some(max_bytes as f32 / 1_073_741_824.0) // octets → GiB
}

fn detect_compute_backends(vendor: &ainonymous_types::GpuVendor) -> Vec<ainonymous_types::ComputeBackend> {
    use ainonymous_types::{ComputeBackend, GpuVendor};
    let mut backends = vec![ComputeBackend::Cpu];
    match vendor {
        GpuVendor::AppleSilicon => backends.push(ComputeBackend::Metal),
        GpuVendor::Nvidia { .. } => backends.push(ComputeBackend::Cuda),
        GpuVendor::Amd { .. } => backends.push(ComputeBackend::Hip),
        GpuVendor::Intel { .. } => backends.push(ComputeBackend::Vulkan),
        GpuVendor::CpuOnly => {}
    }
    backends
}

fn get_total_ram_gb() -> f32 {
    // Lecture de /proc/meminfo sur Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(kb) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb.parse::<u64>() {
                            return kb as f32 / 1_048_576.0;
                        }
                    }
                }
            }
        }
    }
    // Fallback
    16.0
}
