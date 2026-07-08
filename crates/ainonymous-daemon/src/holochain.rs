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
            vram_required_gb: caps.vram_gb,
            max_context: 8192,
            supported_backends: caps.compute_backends.iter().map(|b| format!("{:?}", b)).collect(),
        };

        let warrant = Warrant::new_signed(
            &identity.signing_key,
            WarrantType::ModelClaim,
            serde_json::to_value(claim)?,
            86400 * 90,
        )?;

        self.try_emit_warrant(&warrant).await
    }

    /// Émet NodeCapabilities de façon sûre (non-fatale)
    pub async fn try_emit_node_capabilities(
        &self,
        identity: &ainonymous_quic::NodeIdentity,
    ) -> Result<()> {
        let caps = detect_local_capabilities_from_config(&self.config);

        let warrant = Warrant::new_signed(
            &identity.signing_key,
            WarrantType::NodeCapabilities,
            serde_json::to_value(&caps)?,
            86400 * 30,
        )?;

        self.try_emit_warrant(&warrant).await
    }
