impl HolochainClient {
    /// Émet un ModelClaim après rotation de clé ou au démarrage.
    /// Utilise les capacités réelles détectées du nœud (VRAM, backends).
    pub async fn emit_model_claim(
        &self,
        model_id: &str,
        model_hash: &str,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        // Détection des capacités réelles du nœud
        let caps = detect_local_capabilities_from_config(&self.config); // on réutilise la logique existante

        let claim = ModelClaim {
            model_id: model_id.to_string(),
            model_hash: model_hash.to_string(),
            vram_required_gb: caps.vram_gb.max(8.0), // au moins 8 Go
            max_context: 8192,
            supported_backends: caps.compute_backends
                .iter()
                .map(|b| format!("{:?}", b))
                .collect(),
        };

        let warrant = Warrant::new_signed(
            &identity.signing_key,
            WarrantType::ModelClaim,
            serde_json::to_value(claim)?,
            86400 * 90, // 90 jours de validité
        )?;

        self.emit_warrant_with_cleanup(&warrant).await?;

        info!(
            "ModelClaim émis pour '{}' (VRAM: {:.1} Go, backends: {:?})",
            model_id, claim.vram_required_gb, claim.supported_backends
        );

        Ok(())
    }

    /// Émet aussi un warrant de capacités du nœud (NodeCapabilities)
    pub async fn emit_node_capabilities(
        &self,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        let caps = detect_local_capabilities_from_config(&self.config);

        let warrant = Warrant::new_signed(
            &identity.signing_key,
            WarrantType::NodeCapabilities,
            serde_json::to_value(&caps)?,
            86400 * 30, // 30 jours
        )?;

        self.emit_warrant_with_cleanup(&warrant).await?;
        info!("NodeCapabilities warrant émis");
        Ok(())
    }
}

/// Version helper qui appelle detect_local_capabilities (déjà présente dans le fichier)
fn detect_local_capabilities_from_config(config: &DaemonConfig) -> ainonymous_types::NodeCapabilities {
    detect_local_capabilities(config)
}
