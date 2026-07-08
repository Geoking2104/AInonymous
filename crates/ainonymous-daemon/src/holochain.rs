// Amélioration de la négociation P2P via Holochain
impl HolochainClient {
    /// Négocie une session QUIC de manière P2P via Holochain (préfère le chemin DHT)
    pub async fn negotiate_quic_session_p2p(
        &self,
        target_agent: &str,
        layer_range: Option<(u32, u32)>,
        next_agent: Option<String>,
        next_layer_range: Option<(u32, u32)>,
        requester_pubkey: Option<[u8; 32]>,
    ) -> Result<ainonymous_quic::SessionOffer> {
        // En mode Conductor (Holochain réel), on passe toujours par le zome
        // qui fait du call_remote P2P sur le DHT.
        if matches!(&self.backend, Backend::Conductor(_)) {
            return self.negotiate_quic_session(
                target_agent,
                layer_range,
                next_agent,
                next_layer_range,
                requester_pubkey,
            ).await;
        }

        // En mode Static, on garde le comportement REST (fallback)
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
            });
        }
        Ok(nodes)
    }
}
