    /// Émet NodeCapabilities avec estimation VRAM réaliste
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
            estimate_vram_simple(
                13.0, // hypothèse modèle ~13B
                self.config.inference.context_size,
                detect_gpu_layers(self.config.inference.n_gpu_layers),
            ) / 1024.0
        };

        let mut final_caps = caps;
        final_caps.vram_gb = estimated_vram;

        let warrant = Warrant::new_signed(
            &identity.signing_key,
            WarrantType::NodeCapabilities,
            serde_json::to_value(&final_caps)?,
            86400 * 30,
        )?;

        self.try_emit_warrant(&warrant).await
    }
