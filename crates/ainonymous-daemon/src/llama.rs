        let model_path = self.config.models_dir.join(
            format!("{}.gguf", self.config.inference.default_model)
        );

        // Détection réelle de la taille du modèle
        let model_size_gb = get_model_size_gb(&model_path);

        if !model_path.exists() {
            warn!("Modèle {:?} non trouvé (taille estimée: {:.1} Go)", model_path, model_size_gb);
        }

        let mut ngl = detect_gpu_layers(self.config.inference.n_gpu_layers);
        let is_gpu = ngl != 0;

        // === Ajustement automatique de n_gpu_layers si VRAM insuffisante ===
        let mut estimated_vram = estimate_vram_simple(
            model_size_gb,
            self.config.inference.context_size,
            ngl,
        ) / 1024.0;

        let caps = detect_local_capabilities_from_config(&self.config);
        let available_vram = caps.vram_gb.max(4.0);

        let safety_margin = 0.85;

        while estimated_vram > available_vram * safety_margin && ngl > 0 {
            let previous = ngl;
            ngl = (ngl as f32 * 0.75) as i32;
            if ngl < 0 { ngl = 0; }

            estimated_vram = estimate_vram_simple(
                model_size_gb,
                self.config.inference.context_size,
                ngl,
            ) / 1024.0;

            info!("Auto-réduction n_gpu_layers: {} → {} (VRAM estimée: {:.1} Go)", previous, ngl, estimated_vram);
        }
