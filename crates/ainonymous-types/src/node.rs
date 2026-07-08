/// Informations géographiques optionnelles d'un nœud (si partagé par le nœud)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    /// Rayon de précision en km (optionnel)
    #[serde(default)]
    pub accuracy_km: Option<f32>,
}

/// Capacités d'un nœud (déjà existant, on ajoute juste la géolocalisation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub agent_id: String,
    pub vram_gb: f32,
    pub ram_gb: f32,
    pub gpu_vendor: GpuVendor,
    pub compute_backends: Vec<ComputeBackend>,
    pub loaded_models: Vec<String>,
    pub max_concurrent_requests: u8,
    pub network_bandwidth_mbps: Option<f32>,
    pub region_hint: Option<String>,
    pub quic_endpoint: Option<String>,
    pub node_pubkey: Option<String>,
    /// Localisation géographique optionnelle (si le nœud choisit de la partager)
    #[serde(default)]
    pub geo_location: Option<GeoLocation>,
}
