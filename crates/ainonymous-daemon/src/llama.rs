    /// Démarrer llama-server avec optimisations CUDA/mémoire
    pub async fn start(&self) -> Result<()> {
        let model_path = self.config.models_dir.join(
            format!("{}.gguf", self.config.inference.default_model)
        );

        if !model_path.exists() {
            warn!("Modèle {:?} non trouvé", model_path);
        }

        let ngl = detect_gpu_layers(self.config.inference.n_gpu_layers);

        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &ngl.to_string(),
            "--parallel", &self.config.inference.parallel_requests.to_string(),
            "--cache-type-k", &self.config.inference.kv_cache_type,
            "--cache-type-v", &self.config.inference.kv_cache_type,
        ]);

        // Flash Attention (fortement recommandé pour CUDA)
        if self.config.inference.flash_attention {
            cmd.arg("--flash-attn");
            info!("Flash Attention activé");
        }

        // mlock pour éviter le swapping (utile sur gros modèles)
        if self.config.inference.mlock {
            cmd.arg("--mlock");
            info!("mlock activé (pas de swap)");
        }

        if model_path.exists() {
            cmd.args(["--model", model_path.to_str().unwrap()]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn()?;
        info!("llama-server démarré (pid: {}, n_gpu_layers: {})", child.id(), ngl);

        *self.process.lock().unwrap() = Some(child);

        // Attente du démarrage
        for i in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if self.is_running().await {
                info!("llama-server prêt après {}s", i + 1);
                return Ok(());
            }
        }

        anyhow::bail!("llama-server n'a pas démarré dans les 30 secondes");
    }
