use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::DaemonConfig;
use crate::holochain::detect_local_capabilities_from_config;

// ── Client HTTP vers llama-server (API OpenAI-compatible) ─────────────────────

/// Client léger vers un processus `llama-server` local.
/// Utilise l'API OpenAI-compatible `/v1/chat/completions`.
#[derive(Clone)]
pub struct LlamaClient {
    base_url: String,
    http: reqwest::Client,
}

// Structs de sérialisation pour l'API llama-server (/v1/chat/completions)
#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a serde_json::Value,
    max_tokens: u32,
    stream: bool,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    completion_tokens: u32,
}

/// Résultat d'une inférence solo via llama-server.
pub struct SoloResult {
    pub text: String,
    pub token_count: u32,
    pub finish_reason: Option<String>,
}

impl LlamaClient {
    pub fn new(port: u16) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("reqwest client"),
        }
    }

    /// Vérifie que llama-server répond sur `/health`.
    pub async fn is_ready(&self) -> bool {
        self.http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Inférence chat via `/v1/chat/completions` (non-streaming).
    ///
    /// `messages` : tableau JSON OpenAI `[{"role":"user","content":"..."}]`.
    /// `model_id`  : transféré dans le champ `model` (llama-server l'ignore mais
    ///               certains clients en ont besoin).
    pub async fn chat_completions(
        &self,
        model_id: &str,
        messages: &serde_json::Value,
        max_tokens: u32,
    ) -> Result<SoloResult> {
        let max_tokens = if max_tokens == 0 { 512 } else { max_tokens };

        let req = ChatCompletionRequest {
            model: model_id,
            messages,
            max_tokens,
            stream: false,
            temperature: 0.7,
        };

        let resp = self.http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&req)
            .send()
            .await
            .context("llama-server /v1/chat/completions")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("llama-server erreur {}: {}", status, body);
        }

        let completion: ChatCompletionResponse = resp
            .json()
            .await
            .context("décodage réponse llama-server")?;

        let choice = completion.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("llama-server: réponse vide (0 choices)"))?;

        let text = choice.message.content.unwrap_or_default();
        let token_count = completion.usage
            .map(|u| u.completion_tokens)
            .unwrap_or(text.split_whitespace().count() as u32); // fallback heuristique

        Ok(SoloResult { text, token_count, finish_reason: choice.finish_reason })
    }
}

// ── Détection GPU (n_gpu_layers auto) ─────────────────────────────────────────

/// Retourne le nombre de couches à déléguer au GPU pour llama-server.
///
/// - `override_layers != -1` → valeur explicite depuis la config, retournée telle quelle.
/// - NVIDIA (nvidia-smi trouvé)  → `-1` (llama.cpp charge tout sur le GPU).
/// - AMD ROCm (rocm-smi trouvé) → `-1`.
/// - Apple Silicon (macOS)       → `-1` (mémoire unifiée = tout sur Metal).
/// - CPU seul                    → `0`.
pub fn detect_gpu_layers(override_layers: i32) -> i32 {
    if override_layers != -1 {
        return override_layers;
    }

    // NVIDIA via nvidia-smi
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            let name = String::from_utf8_lossy(&out.stdout);
            info!("GPU NVIDIA détecté : {} — n_gpu_layers=-1 (auto)", name.trim());
            return -1;
        }
    }

    // AMD via rocm-smi
    if let Ok(out) = std::process::Command::new("rocm-smi")
        .args(["--showproductname"])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            let text = String::from_utf8_lossy(&out.stdout);
            let name = text.lines().nth(1).unwrap_or("AMD GPU").trim().to_string();
            info!("GPU AMD détecté : {} — n_gpu_layers=-1 (auto)", name);
            return -1;
        }
    }

    // Apple Metal (macOS uniquement — mémoire unifiée, tout sur GPU)
    #[cfg(target_os = "macos")]
    {
        info!("Apple Metal disponible — n_gpu_layers=-1 (auto)");
        return -1;
    }

    #[allow(unreachable_code)]
    {
        info!("Aucun GPU détecté — inférence CPU (n_gpu_layers=0)");
        0
    }
}

// ── Estimation VRAM (Palier G1) ────────────────────────────────────────────────

/// Retourne la taille réelle d'un fichier GGUF en Go.
/// Retourne une valeur par défaut (13.0) si le fichier n'existe pas ou est illisible.
pub fn get_model_size_gb(model_path: &std::path::Path) -> f32 {
    match std::fs::metadata(model_path) {
        Ok(metadata) => {
            let size_bytes = metadata.len() as f32;
            let size_gb = size_bytes / (1024.0 * 1024.0 * 1024.0);
            debug!("Taille du modèle {:?} : {:.2} Go", model_path, size_gb);
            size_gb
        }
        Err(e) => {
            warn!("Impossible de lire la taille du modèle {:?}: {}. Utilisation de la valeur par défaut (13 Go).", model_path, e);
            13.0
        }
    }
}

/// Estime la VRAM nécessaire (en MB) pour charger un modèle.
///
/// Formule approximative :
/// - Poids du modèle sur GPU
/// - KV-cache (2 * layers * context * hidden_size * bytes_per_element * parallel)
/// - Activations + overhead
pub fn estimate_model_vram_mb(
    model_size_gb: f32,
    n_layers: u32,
    context_size: u32,
    hidden_size: u32,
    n_gpu_layers: i32,
    kv_bytes: u32, // 2 pour f16, 1 pour q8_0, 0.5 pour q4_0
    parallel: u32,
) -> f32 {
    let gpu_layers = if n_gpu_layers < 0 {
        n_layers as f32
    } else {
        n_gpu_layers as f32
    };

    // Poids du modèle sur GPU
    let model_weights_mb = model_size_gb * 1024.0 * (gpu_layers / n_layers as f32);

    // KV-cache estimation (très approximative)
    let kv_cache_mb = (n_layers as f32)
        * (context_size as f32)
        * (hidden_size as f32)
        * (kv_bytes as f32)
        * 2.0 // K + V
        * (parallel as f32)
        / 1024.0 / 1024.0;

    // Overhead (activations, CUDA context, etc.)
    let overhead_mb = 512.0 + (parallel as f32 * 128.0);

    model_weights_mb + kv_cache_mb + overhead_mb
}

/// Version simplifiée qui utilise des valeurs par défaut raisonnables
pub fn estimate_vram_simple(
    model_size_gb: f32,
    context_size: u32,
    n_gpu_layers: i32,
) -> f32 {
    // Hypothèses raisonnables pour un modèle type 7B-13B
    estimate_model_vram_mb(
        model_size_gb,
        32,   // ~32 layers
        context_size,
        4096, // hidden size typique
        n_gpu_layers,
        2, // f16 KV-cache
        1, // parallel=1
    )
}

// ── Gestionnaire du processus llama-server ─────────────────────────────────────

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

    /// Démarrer llama-server avec le modèle par défaut.
    ///
    /// Palier G1 : détecte la taille réelle du GGUF, estime la VRAM requise et
    /// réduit automatiquement `n_gpu_layers` par paliers de 25% tant que
    /// l'estimation dépasse 85% de la VRAM disponible détectée localement.
    pub async fn start(&self) -> Result<()> {
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

        if self.config.inference.flash_attention {
            cmd.arg("--flash-attn");
        }

        // mlock activé par défaut sur GPU pour réduire la fragmentation
        let use_mlock = self.config.inference.mlock || is_gpu;
        if use_mlock {
            cmd.arg("--mlock");
            info!("mlock activé (réduction fragmentation + swap)");
        }

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
            "llama-server démarré (pid: {}) | GPU={} | ctx={} | kv={} | mlock={} | n_gpu_layers={}",
            child.id(), is_gpu, self.config.inference.context_size, kv_type, use_mlock, ngl
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

        let ngl = detect_gpu_layers(self.config.inference.n_gpu_layers);
        let mut cmd = std::process::Command::new(&self.config.llama_server_bin);
        cmd.args([
            "--host", "127.0.0.1",
            "--port", &self.config.llama_server_port.to_string(),
            "--model", model_path.to_str().unwrap(),
            "--ctx-size", &self.config.inference.context_size.to_string(),
            "--n-gpu-layers", &ngl.to_string(),
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
