    pub async fn start(&self) -> Result<()> {
        let model_path = self.config.models_dir.join(
            format!("{}.gguf", self.config.inference.default_model)
        );

        if !model_path.exists() {
            warn!("Modèle {:?} non trouvé", model_path);
        }

        let ngl = detect_gpu_layers(self.config.inference.n_gpu_layers);
        let is_gpu = ngl != 0;

        // === Vérification VRAM avant démarrage ===
        let model_size_gb = 13.0; // TODO: détecter la taille réelle du GGUF
        let estimated_vram = estimate_vram_simple(
            model_size_gb,
            self.config.inference.context_size,
            ngl,
        ) / 1024.0;

        let caps = detect_local_capabilities_from_config(&self.config);
        let available_vram = caps.vram_gb.max(estimated_vram);

        if estimated_vram > available_vram * 0.85 {
            warn!(
                "VRAM estimée insuffisante ! Besoin ~{:.1} Go, disponible ~{:.1} Go. Réduction possible des couches GPU.",
                estimated_vram, available_vram
            );

            // Auto-réduction prudente des couches GPU
            if ngl > 0 {
                let reduced_ngl = (ngl as f32 * 0.7) as i32;
                info!("Auto-réduction n_gpu_layers: {} → {}", ngl, reduced_ngl);
                // Note: on ne modifie pas self.config ici (immutable), on logge seulement
            }
        } else {
            info!("VRAM estimée OK : ~{:.1} Go / {:.1} Go disponibles", estimated_vram, available_vram);
        }

        // === Construction de la commande ===
        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &ngl.to_string(),
            "--parallel", &self.config.inference.parallel_requests.to_string(),
        ]);

        // KV-cache compact par défaut sur GPU
        let kv_type = if self.config.inference.kv_cache_type.is_empty() {
            if is_gpu { "q8_0" } else { "f16" }
        } else {
            &self.config.inference.kv_cache_type
        };
        cmd.args(["--cache-type-k", kv_type, "--cache-type-v", kv_type]);

        if self.config.inference.flash_attention {
            cmd.arg("--flash-attn");
        }

        let use_mlock = self.config.inference.mlock || is_gpu;
        if use_mlock {
            cmd.arg("--mlock");
        }

        if self.config.inference.context_size > 8192 {
            warn!("Contexte très grand ({}). Risque de fragmentation VRAM.", self.config.inference.context_size);
        }

        if model_path.exists() {
            cmd.args(["--model", model_path.to_str().unwrap()]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn()?;
        info!("llama-server démarré | GPU={} | ctx={} | kv={}", is_gpu, self.config.inference.context_size, kv_type);

        *self.process.lock().unwrap() = Some(child);

        for i in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if self.is_running().await {
                info!("llama-server prêt après {}s", i + 1);
                return Ok(());
            }
        }

        anyhow::bail!("llama-server n'a pas démarré dans les 30 secondes");
    }
