use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// Cache simple pour la découverte DHT
static NODE_DISCOVERY_CACHE: once_cell::sync::Lazy<RwLock<HashMap<String, (Vec<NodeSummary>, Instant)>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

const DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(30); // 30 secondes de cache

impl HolochainClient {
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
        let nodes = self.get_available_nodes(model_id).await?;

        // Mise à jour du cache
        {
            let mut cache = NODE_DISCOVERY_CACHE.write().await;
            cache.insert(cache_key, (nodes.clone(), Instant::now()));
        }

        Ok(nodes)
    }

    /// Découverte optimisée avec scoring et filtrage
    pub async fn discover_nodes_p2p_optimized(
        &self,
        model_id: &str,
        min_vram_gb: Option<f32>,
        preferred_region: Option<&str>,
    ) -> Result<Vec<NodeSummary>> {
        let mut nodes = self.discover_nodes_p2p_cached(model_id).await?;

        // Filtrage
        if let Some(min_vram) = min_vram_gb {
            nodes.retain(|n| n.vram_gb >= min_vram);
        }

        if let Some(region) = preferred_region {
            nodes.retain(|n| {
                n.region_hint.as_deref() == Some(region) || n.region_hint.is_none()
            });
        }

        // Scoring : on privilégie les nœuds avec peu de charge et beaucoup de VRAM
        nodes.sort_by(|a, b| {
            let score_a = (a.vram_gb * 2.0) - (a.current_load * 10.0);
            let score_b = (b.vram_gb * 2.0) - (b.current_load * 10.0);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(nodes)
    }
}
