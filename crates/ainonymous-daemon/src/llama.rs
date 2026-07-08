    pub async fn start(&self) -> Result<()> {
        let model_path = self.config.models_dir.join(
            format!("{}.gguf", self.config.inference.default_model)
        );

        if !model_path.exists() {
            warn!("Modèle {:?} non trouvé", model_path);
        }

        let ngl = detect_gpu_layers(self.config.inference.n_gpu_layers);
        let is_gpu = ngl != 0;

        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &ngl.to_string(),
            "--parallel", &self.config.inference.parallel_requests.to_string(),
        ]);

        // KV-cache compact par défaut (q8_0 = bon compromis)
        let kv_type = if self.config.inference.kv_cache_type.is_empty() {
            if is_gpu { "q8_0" } else { "f16" }
        } else {
            &self.config.inference.kv_cache_type
        };

        cmd.args(["--cache-type-k", kv_type, "--cache-type-v", kv_type]);

        // Flash Attention
        if self.config.inference.flash_attention {
            cmd.arg("--flash-attn");
        }

        // mlock activé par défaut sur GPU pour réduire la fragmentation
        let use_mlock = self.config.inference.mlock || is_gpu;
        if use_mlock {
            cmd.arg("--mlock");
            info!("mlock activé (réduction fragmentation + swap)");
        }

        // Avertissement si contexte très grand
        if self.config.inference.context_size > 8192 {
            warn!(
                "Contexte très grand ({} tokens). Risque de fragmentation et forte consommation VRAM.",
                self.config.inference.context_size
            );
        }

        if model_path.exists() {
            cmd.args(["--model", model_path.to_str().unwrap()]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn()?;
        info!(
            "llama-server démarré | GPU={} | ctx={} | kv={} | mlock={}",
            is_gpu, self.config.inference.context_size, kv_type, use_mlock
        );

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
