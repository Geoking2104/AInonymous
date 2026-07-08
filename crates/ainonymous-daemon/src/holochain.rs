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
            let score = (15.0 * (1.0 - (distance_km / 20000.0).min(1.0))).max(0.0);
            score
        }
        (Some(_), None) => 8.0,  // Bonus si le nœud partage sa position
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

impl HolochainClient {
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
}
