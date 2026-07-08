    /// Charger un modèle avec une plage de couches spécifique (pour pipeline distribué)
    pub async fn load_model_with_layers(
        &self,
        model_id: &str,
        layer_range: Option<(u32, u32)>,
    ) -> Result<()> {
        info!("Chargement modèle {} avec layer_range: {:?}", model_id, layer_range);

        if let Some((start, end)) = layer_range {
            info!("Ce nœud est responsable des couches {} à {}", start, end);
            // TODO G2: Actuellement llama-server ne supporte pas nativement --layer-range.
            // Solutions possibles :
            //   1. Utiliser un fork/custom llama-server qui supporte le chargement partiel
            //   2. Utiliser llama.cpp en mode RPC / split model
            //   3. Chaque nœud charge le modèle complet + on route intelligemment (moins efficace)
            //
            // Pour l'instant on logge et on charge le modèle complet.
        }

        self.load_model(model_id, layer_range).await
    }

    async fn restart_with_model(
        &self,
        model_id: &str,
        layer_range: Option<(u32, u32)>,
    ) -> Result<()> {
        if let Some(mut child) = self.process.lock().unwrap().take() {
            let _ = child.kill();
        }

        let model_path = self.config.models_dir.join(format!("{}.gguf", model_id));
        let ngl = detect_gpu_layers(self.config.inference.n_gpu_layers);

        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--model", model_path.to_str().unwrap(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &ngl.to_string(),
            "--parallel", &self.config.inference.parallel_requests.to_string(),
        ]);

        if let Some((start, end)) = layer_range {
            // TODO G2: Ajouter --layer-range quand llama.cpp le supportera pleinement
            debug!("Layer range demandée: {}-{} (non supportée nativement pour l'instant)", start, end);
        }

        if self.config.inference.flash_attention {
            cmd.arg("--flash-attn");
        }

        let child = cmd.spawn()?;
        *self.process.lock().unwrap() = Some(child);

        for _ in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if self.is_running().await {
                return Ok(());
            }
        }

        anyhow::bail!("Redémarrage llama-server échoué");
    }
