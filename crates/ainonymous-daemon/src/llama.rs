use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use anyhow::Result;
use tracing::{debug, error, info, warn};

use crate::DaemonConfig;

/// Gestionnaire du processus llama-server
pub struct LlamaManager {
    config: DaemonConfig,
    process: Arc<Mutex<Option<Child>>>,
}

impl LlamaManager {
    pub fn new(config: DaemonConfig) -> Self {
        Self { config, process: Arc::new(Mutex::new(None)) }
    }

    /// Vérifier si llama-server répond sur le port configuré
    pub async fn is_running(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/health", self.config.llama_server_port);
        reqwest::get(&url).await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Démarrer llama-server avec le modèle par défaut
    pub async fn start(&self) -> Result<()> {
        let model_path = self.config.models_dir.join(
            format!("{}.gguf", self.config.inference.default_model)
        );

        if !model_path.exists() {
            warn!("Modèle {:?} non trouvé, llama-server démarré sans modèle", model_path);
        }

        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &self.config.inference.n_gpu_layers.to_string(),
            "--parallel", &self.config.inference.parallel_requests.to_string(),
            "--cache-type-k", &self.config.inference.kv_cache_type,
            "--cache-type-v", &self.config.inference.kv_cache_type,
        ]);

        if self.config.inference.flash_attention {
            cmd.arg("--flash-attn");
        }

        if model_path.exists() {
            cmd.args(["--model", model_path.to_str().unwrap()]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn()?;
        info!("llama-server démarré (pid: {})", child.id());

        *self.process.lock().unwrap() = Some(child);

        // Attendre que le serveur soit prêt (max 30s)
        for i in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if self.is_running().await {
                info!("llama-server prêt après {}s", i + 1);
                return Ok(());
            }
        }

        anyhow::bail!("llama-server n'a pas démarré dans les 30 secondes")
    }

    /// Charger un modèle dans llama-server (via API)
    pub async fn load_model(&self, model_id: &str, layer_range: Option<(u32, u32)>) -> Result<()> {
        let model_path = self.config.models_dir.join(format!("{}.gguf", model_id));

        if !model_path.exists() {
            anyhow::bail!("Modèle {} non trouvé dans {:?}", model_id, self.config.models_dir);
        }

        info!("Chargement modèle {} (couches {:?})", model_id, layer_range);

        // llama-server ne supporte pas le rechargement à chaud
        // Il faut redémarrer avec le nouveau modèle
        self.restart_with_model(model_id, layer_range).await
    }

    async fn restart_with_model(&self, model_id: &str, layer_range: Option<(u32, u32)>) -> Result<()> {
        // Arrêter l'instance courante
        if let Some(mut child) = self.process.lock().unwrap().take() {
            let _ = child.kill();
        }

        // Redémarrer avec le nouveau modèle et les couches appropriées
        let model_path = self.config.models_dir.join(format!("{}.gguf", model_id));

        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--model", model_path.to_str().unwrap(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &self.config.inference.n_gpu_layers.to_string(),
            "--parallel", &self.config.inference.parallel_requests.to_string(),
            "--flash-attn",
        ]);

        // Pour le pipeline-split : spécifier la plage de couches
        // Note : llama.cpp supporte --tensor-split pour le multi-GPU
        // Pour le pipeline cross-nœuds : il faut un fork ou RPC interne
        if let Some((start, end)) = layer_range {
            debug!("Couches {}-{} assignées à ce nœud", start, end);
            // TODO: implémenter --layer-range quand llama.cpp le supportera
        }

        let child = cmd.spawn()?;
        *self.process.lock().unwrap() = Some(child);

        // Attendre démarrage
        for _ in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if self.is_running().await { return Ok(()); }
        }

        anyhow::bail!("Redémarrage llama-server échoué")
    }
}

impl Drop for LlamaManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
            }
        }
    }
}
