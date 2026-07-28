use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::VerifyingKey;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use ainonymous_types::{ExecutionPlan, GeoLocation, ModelClaim, NodeHeartbeat, Warrant, WarrantType};
use crate::config::DaemonConfig;
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
    #[serde(default)]
    pub geo_location: Option<GeoLocation>,
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
    config: DaemonConfig,
}

impl HolochainClient {
    pub async fn connect(config: &DaemonConfig) -> Result<Self> {
        let membrane_proof = config.holochain.membrane_proof.clone();

        let backend = match config.holochain.backend {
            crate::config::HolochainBackendKind::Static => Backend::Static,
            crate::config::HolochainBackendKind::Conductor => {
                let connect_fut = ConductorClient::connect(
                    config.holochain.admin_port,
                    config.holochain.app_port,
                    &config.holochain_app_id,
                    membrane_proof.clone(),
                );
                match tokio::time::timeout(Duration::from_secs(60), connect_fut).await {
                    Ok(Ok(c)) => Backend::Conductor(Arc::new(c)),
                    Ok(Err(e)) => {
                        warn!("Conducteur Holochain injoignable ({e}) — repli sur bootstrap statique");
                        Backend::Static
                    }
                    Err(_elapsed) => {
                        warn!("Conducteur Holochain — timeout 60s — repli sur bootstrap statique");
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
            config: config.clone(),
        };
        Ok(client)
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

    /// Appelle une fonction de zome avec une meilleure gestion d'erreur
    pub async fn zome_call(
        &self,
        dna: &str,
        zome: &str,
        function: &str,
        payload: Value,
    ) -> Result<Value> {
        debug!("Zome call: {}::{}::{}", dna, zome, function);

        match &self.backend {
            Backend::Conductor(c) => {
                c.call_zome_json(dna, zome, function, payload)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!("Holochain zome call failed [{}::{}::{}]: {}", dna, zome, function, e)
                    })
            }
            Backend::Static => {
                let resp = self
                    .http
                    .post(format!("{}/zome/{}/{}/{}", self.base_url(), dna, zome, function))
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("Static zome call HTTP error: {}", e))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    anyhow::bail!("Static zome call failed [{}::{}::{}]: HTTP {} - {}",
                        dna, zome, function, status, body);
                }

                resp.json::<Value>()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to parse static zome response: {}", e))
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
        Ok(())
    }

    pub async fn send_heartbeat(&self, hb: NodeHeartbeat) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "heartbeat",
            serde_json::to_value(&hb)?,
        ).await?;
        Ok(())
    }

    pub async fn get_execution_plan(&self, model_id: &str) -> Result<ExecutionPlan> {
        let resp = self.zome_call(
            "inference-mesh",
            "coordinator",
            "compute_execution_plan",
            json!({ "model_id": model_id }),
        ).await?;

        Ok(serde_json::from_value(resp)?)
    }

    pub async fn get_available_nodes(&self, model_id: &str) -> Result<Vec<NodeSummary>> {
        let resp = self.zome_call(
            "agent-registry",
            "coordinator",
            "get_available_nodes",
            json!(model_id),
        ).await?;

        Ok(serde_json::from_value(resp)?)
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
                    .parse()?;
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

                Ok(resp.json::<ainonymous_quic::SessionOffer>().await?)
            }
        }
    }

    /// Négocie une session QUIC de manière P2P via Holochain (préfère le chemin DHT)
    pub async fn negotiate_quic_session_p2p(
        &self,
        target_agent: &str,
        layer_range: Option<(u32, u32)>,
        next_agent: Option<String>,
        next_layer_range: Option<(u32, u32)>,
        requester_pubkey: Option<[u8; 32]>,
    ) -> Result<ainonymous_quic::SessionOffer> {
        // En mode Conductor (Holochain réel) comme en mode Static, la négociation
        // passe par le même chemin : le zome (ou le fallback REST) fait autorité.
        self.negotiate_quic_session(
            target_agent,
            layer_range,
            next_agent,
            next_layer_range,
            requester_pubkey,
        ).await
    }

    /// Découverte P2P des nœuds disponibles via Holochain DHT
    pub async fn discover_nodes_p2p(&self, model_id: &str) -> Result<Vec<NodeSummary>> {
        // En mode Conductor, on interroge directement le DHT
        if matches!(&self.backend, Backend::Conductor(_)) {
            return self.get_available_nodes(model_id).await;
        }

        // En mode statique, on retourne les peers configurés
        let mut nodes = Vec::new();
        for peer in &self.peers {
            nodes.push(NodeSummary {
                agent_id: peer.agent_id.clone(),
                vram_gb: 0.0,
                current_load: 0.0,
                available_slots: 4,
                quic_endpoint: peer.quic_endpoint.clone(),
                region_hint: None,
                score: 1.0,
                node_pubkey: None,
                geo_location: None,
            });
        }
        Ok(nodes)
    }

    /// Version optimisée avec cache de la découverte P2P
    pub async fn discover_nodes_p2p_cached(&self, model_id: &str) -> Result<Vec<NodeSummary>> {
        let cache_key = model_id.to_string();

        // Vérifier le cache
        {
            let cache = NODE_DISCOVERY_CACHE.read().await;
            if let Some((nodes, timestamp)) = cache.get(&cache_key) {
                if timestamp.elapsed() < DISCOVERY_CACHE_TTL {
                    debug!("Utilisation du cache de découverte DHT pour {}", model_id);
                    return Ok(nodes.clone());
                }
            }
        }

        // Requête réelle sur le DHT
        let nodes = self.discover_nodes_p2p(model_id).await?;

        // Mise à jour du cache
        {
            let mut cache = NODE_DISCOVERY_CACHE.write().await;
            cache.insert(cache_key, (nodes.clone(), Instant::now()));
        }

        Ok(nodes)
    }

    /// Découverte optimisée avec scoring géographique et filtrage
    pub async fn discover_nodes_p2p_optimized(
        &self,
        model_id: &str,
        min_vram_gb: Option<f32>,
        reference_geo: Option<&GeoLocation>, // Position de référence (coordinateur)
    ) -> Result<Vec<NodeSummary>> {
        let mut nodes = self.discover_nodes_p2p_cached(model_id).await?;

        if let Some(min_vram) = min_vram_gb {
            nodes.retain(|n| n.vram_gb >= min_vram);
        }

        // Calcul du score amélioré
        for node in &mut nodes {
            let vram_score = (node.vram_gb / 24.0).min(1.0) * 35.0;
            let load_score = (1.0 - node.current_load.clamp(0.0, 1.0)) * 25.0;
            let slots_score = ((node.available_slots as f32) / 8.0).min(1.0) * 15.0;

            let geo_score = if let Some(geo) = &node.geo_location {
                geographic_proximity_score(Some(geo), reference_geo)
            } else {
                5.0
            };

            node.score = vram_score + load_score + slots_score + geo_score;
        }

        // Tri par score décroissant
        nodes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(nodes)
    }

    pub async fn reannounce_pubkey(
        &self,
        new_pubkey_hex: &str,
        config: &DaemonConfig,
    ) -> Result<()> {
        self.announce_capabilities(config, Some(new_pubkey_hex)).await?;
        info!("DHT : nouvelle clé publique annoncée après rotation : {}", new_pubkey_hex);
        Ok(())
    }

    pub async fn update_quic_endpoint(&self, addr: SocketAddr) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "update_quic_endpoint",
            json!({ "endpoint": addr.to_string() }),
        ).await?;
        Ok(())
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
        Ok(())
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

    // ── Warrants (Palier F) ─────────────────────────────────────────────────
    // Zome dédié "warrants"/"coordinator" (cf. zomes/warrants/README.md).

    /// Émet un warrant (appel direct, fatal en cas d'erreur)
    pub async fn emit_warrant(&self, warrant: &Warrant) -> Result<()> {
        self.zome_call(
            "warrants",
            "coordinator",
            "emit_warrant",
            serde_json::to_value(warrant)?,
        ).await?;
        info!("Warrant émis: {:?} par {}", warrant.warrant_type, hex::encode(warrant.issuer));
        Ok(())
    }

    /// Émet un warrant en supprimant les anciens du même type (recommandé)
    pub async fn emit_warrant_with_cleanup(&self, warrant: &Warrant) -> Result<()> {
        self.zome_call(
            "warrants",
            "coordinator",
            "emit_warrant_with_cleanup",
            serde_json::to_value(warrant)?,
        ).await?;
        info!("Warrant émis avec cleanup: {:?}", warrant.warrant_type);
        Ok(())
    }

    /// Émet un warrant de façon non-fatale (ne fait pas crasher le daemon si le
    /// zome 'warrants' n'est pas encore intégré au DNA)
    pub async fn try_emit_warrant(&self, warrant: &Warrant) -> Result<()> {
        match self.emit_warrant_with_cleanup(warrant).await {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Impossible d'émettre le warrant ({:?}): {}. Le zome 'warrants' est peut-être pas encore intégré.",
                      warrant.warrant_type, e);
                Ok(())
            }
        }
    }

    /// Vérifie un Warrant via le zome
    pub async fn verify_warrant(&self, warrant: &Warrant) -> Result<bool> {
        let result = self.zome_call(
            "warrants",
            "coordinator",
            "verify_warrant",
            serde_json::to_value(warrant)?,
        ).await?;

        Ok(result["valid"].as_bool().unwrap_or(false))
    }

    /// Récupère les warrants valides d'un nœud
    pub async fn get_warrants_for_agent(&self, agent_id: &str) -> Result<Vec<Warrant>> {
        let resp = self.zome_call(
            "warrants",
            "coordinator",
            "get_warrants",
            json!({ "agent_id": agent_id }),
        ).await?;

        Ok(serde_json::from_value(resp)?)
    }

    /// Émet un ModelClaim après rotation de clé ou au démarrage (fatal)
    pub async fn emit_model_claim(
        &self,
        model_id: &str,
        model_hash: &str,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        let caps = detect_local_capabilities_from_config(&self.config);

        let claim = ModelClaim {
            model_id: model_id.to_string(),
            model_hash: model_hash.to_string(),
            vram_required_gb: caps.vram_gb.max(8.0),
            max_context: 8192,
            supported_backends: caps.compute_backends.iter().map(|b| format!("{:?}", b)).collect(),
        };

        let warrant = Warrant::new_signed(
            identity.signing_key(),
            WarrantType::ModelClaim,
            serde_json::to_value(claim)?,
            86400 * 90, // 90 jours
        )?;

        self.emit_warrant_with_cleanup(&warrant).await
    }

    /// Émet un warrant de capacités du nœud (fatal)
    pub async fn emit_node_capabilities(
        &self,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        let caps = detect_local_capabilities_from_config(&self.config);

        let warrant = Warrant::new_signed(
            identity.signing_key(),
            WarrantType::NodeCapabilities,
            serde_json::to_value(&caps)?,
            86400 * 30, // 30 jours
        )?;

        self.emit_warrant_with_cleanup(&warrant).await
    }

    /// Émet un ModelClaim de façon sûre (non-fatale)
    pub async fn try_emit_model_claim(
        &self,
        model_id: &str,
        model_hash: &str,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        let caps = detect_local_capabilities_from_config(&self.config);

        let claim = ModelClaim {
            model_id: model_id.to_string(),
            model_hash: model_hash.to_string(),
            vram_required_gb: caps.vram_gb.max(8.0),
            max_context: 8192,
            supported_backends: caps.compute_backends
                .iter()
                .map(|b| format!("{:?}", b))
                .collect(),
        };

        let warrant = match Warrant::new_signed(
            identity.signing_key(),
            WarrantType::ModelClaim,
            serde_json::to_value(claim)?,
            86400 * 90, // 90 jours
        ) {
            Ok(w) => w,
            Err(e) => {
                warn!("Impossible de créer le ModelClaim warrant: {}", e);
                return Ok(()); // non fatal
            }
        };

        self.try_emit_warrant(&warrant).await
    }

    /// Émet NodeCapabilities avec estimation VRAM réaliste, de façon sûre (non-fatale)
    pub async fn try_emit_node_capabilities(
        &self,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        let caps = detect_local_capabilities_from_config(&self.config);

        // Estimation VRAM plus précise si possible
        let estimated_vram = if caps.vram_gb > 0.0 {
            caps.vram_gb
        } else {
            // Fallback : estimation simple basée sur le modèle par défaut
            crate::llama::estimate_vram_simple(
                13.0, // hypothèse modèle ~13B
                self.config.inference.context_size,
                crate::llama::detect_gpu_layers(self.config.inference.n_gpu_layers),
            ) / 1024.0
        };

        let mut final_caps = caps;
        final_caps.vram_gb = estimated_vram;

        let warrant = match Warrant::new_signed(
            identity.signing_key(),
            WarrantType::NodeCapabilities,
            serde_json::to_value(&final_caps)?,
            86400 * 30,
        ) {
            Ok(w) => w,
            Err(e) => {
                warn!("Impossible de créer le NodeCapabilities warrant: {}", e);
                return Ok(()); // non fatal
            }
        };

        self.try_emit_warrant(&warrant).await
    }
}

// Cache simple pour la découverte DHT (Palier G : optimisation)
static NODE_DISCOVERY_CACHE: Lazy<RwLock<HashMap<String, (Vec<NodeSummary>, Instant)>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
const DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(30);

/// Calcule un score géographique basé sur la distance (si les coordonnées sont partagées)
fn geographic_proximity_score(
    node_geo: Option<&GeoLocation>,
    reference_geo: Option<&GeoLocation>,
) -> f32 {
    match (node_geo, reference_geo) {
        (Some(node), Some(reference)) => {
            let distance_km = haversine_distance(
                node.latitude,
                node.longitude,
                reference.latitude,
                reference.longitude,
            );
            // Score inversement proportionnel à la distance (max 15 points)
            (15.0 * (1.0 - (distance_km / 20000.0).min(1.0))).max(0.0)
        }
        (Some(_), None) => 8.0, // Bonus si le nœud partage sa position
        _ => 3.0,               // Score neutre si pas d'info géo
    }
}

/// Distance de Haversine en kilomètres
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f32 {
    let r = 6371.0; // Rayon de la Terre en km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    (r * c) as f32
}

/// Validation stricte des warrants d'un nœud (Palier F) : signature + expiration + capacités
pub async fn validate_node_warrants(
    holochain: &HolochainClient,
    agent_id: &str,
    required_model: Option<&str>,
) -> Result<bool> {
    let warrants = match holochain.get_warrants_for_agent(agent_id).await {
        Ok(w) => w,
        Err(e) => {
            warn!("Impossible de récupérer les warrants de {}: {}", agent_id, e);
            return Ok(false);
        }
    };

    if warrants.is_empty() {
        warn!("Aucun warrant trouvé pour le nœud {}", agent_id);
        return Ok(false);
    }

    let mut has_valid_model_claim = false;
    let mut has_valid_capabilities = false;

    for warrant in &warrants {
        if warrant.is_expired() {
            continue;
        }

        // Vérifier signature (on suppose que l'issuer est l'agent lui-même)
        let pubkey = match VerifyingKey::from_bytes(&warrant.issuer) {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        if !warrant.verify(&pubkey) {
            warn!("Warrant invalide (signature) pour {}", agent_id);
            continue;
        }

        match warrant.warrant_type {
            WarrantType::ModelClaim => {
                if let Ok(claim) = serde_json::from_value::<ModelClaim>(warrant.payload.clone()) {
                    if let Some(required) = required_model {
                        if claim.model_id == required {
                            has_valid_model_claim = true;
                        }
                    } else {
                        has_valid_model_claim = true;
                    }
                }
            }
            WarrantType::NodeCapabilities => {
                has_valid_capabilities = true;
            }
            _ => {}
        }
    }

    let is_valid = has_valid_model_claim && has_valid_capabilities;

    if !is_valid {
        warn!(
            "Validation warrants échouée pour {} | ModelClaim: {} | Capabilities: {}",
            agent_id, has_valid_model_claim, has_valid_capabilities
        );
    }

    Ok(is_valid)
}

pub(crate) fn detect_local_capabilities(config: &DaemonConfig) -> ainonymous_types::NodeCapabilities {
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
        geo_location: None,
    }
}

/// Petit helper qui réutilise `detect_local_capabilities` — utilisé par les
/// fonctions `emit_*`/`try_emit_*` ainsi que par `llama::LlamaManager::start`.
pub(crate) fn detect_local_capabilities_from_config(config: &DaemonConfig) -> ainonymous_types::NodeCapabilities {
    detect_local_capabilities(config)
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
