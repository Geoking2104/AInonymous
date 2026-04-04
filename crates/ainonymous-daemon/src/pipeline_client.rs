/// Client HTTP vers le pipeline_server.py local.
/// Chaque nœud démarre son propre pipeline_server.py au démarrage du daemon.
/// Le conductor appelle ce client pour exécuter les couches assignées.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::{debug, info, warn};

/// URL du pipeline server local (configuré via DaemonConfig)
const DEFAULT_PIPELINE_PORT: u16 = 9340;

// ── Requêtes / réponses (mirror de pipeline_server.py) ─────────────────────

#[derive(Debug, Serialize)]
pub struct PrefillRequest {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_ids: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_states_b64: Option<String>,
    pub seq_len: usize,
    pub hidden_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct PrefillResponse {
    pub request_id: String,
    pub hidden_states_b64: Option<String>,
    pub seq_len: usize,
    pub hidden_size: usize,
    pub next_token_id: Option<i32>,
    pub next_token_text: Option<String>,
    pub is_last_node: bool,
}

#[derive(Debug, Serialize)]
pub struct DecodeRequest {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_ids: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_states_b64: Option<String>,
    pub seq_len: usize,
    pub hidden_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct DecodeResponse {
    pub request_id: String,
    pub hidden_states_b64: Option<String>,
    pub seq_len: usize,
    pub hidden_size: usize,
    pub next_token_id: Option<i32>,
    pub next_token_text: Option<String>,
    pub is_last_node: bool,
}

#[derive(Debug, Deserialize)]
pub struct PipelineStatus {
    pub model_id: String,
    pub layer_start: u32,
    pub layer_end: u32,
    pub total_layers: u32,
    pub is_first_node: bool,
    pub is_last_node: bool,
    pub active_requests: usize,
    pub device: String,
}

// ── Client ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PipelineClient {
    base_url: String,
    http: reqwest::Client,
}

impl PipelineClient {
    pub fn new(port: u16) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("HTTP client"),
        }
    }

    pub fn local() -> Self {
        Self::new(DEFAULT_PIPELINE_PORT)
    }

    /// Vérifie que le pipeline_server.py tourne et est prêt
    pub async fn health_check(&self) -> Result<PipelineStatus> {
        let resp = self.http
            .get(format!("{}/status", self.base_url))
            .send()
            .await
            .context("pipeline_server inaccessible")?;

        if !resp.status().is_success() {
            anyhow::bail!("pipeline_server /status → {}", resp.status());
        }

        Ok(resp.json::<PipelineStatus>().await?)
    }

    /// Prefill : traite le prompt complet
    /// - Premier nœud  : fournir input_ids
    /// - Autres nœuds  : fournir hidden_states_b64 + seq_len + hidden_size
    pub async fn prefill(&self, req: &PrefillRequest) -> Result<PrefillResponse> {
        debug!(
            "Prefill {} — seq_len={} hidden={}",
            req.request_id, req.seq_len, req.hidden_size
        );

        let resp = self.http
            .post(format!("{}/prefill", self.base_url))
            .json(req)
            .send()
            .await
            .context("POST /prefill échoué")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("pipeline_server /prefill → {}", body);
        }

        Ok(resp.json::<PrefillResponse>().await?)
    }

    /// Decode : génère un token supplémentaire (utilise le KV-cache sur le serveur)
    pub async fn decode(&self, req: &DecodeRequest) -> Result<DecodeResponse> {
        let resp = self.http
            .post(format!("{}/decode", self.base_url))
            .json(req)
            .send()
            .await
            .context("POST /decode échoué")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("pipeline_server /decode → {}", body);
        }

        Ok(resp.json::<DecodeResponse>().await?)
    }

    /// Libère le KV-cache d'une requête terminée
    pub async fn clear(&self, request_id: &str) -> Result<()> {
        self.http
            .post(format!("{}/clear", self.base_url))
            .json(&serde_json::json!({ "request_id": request_id }))
            .send()
            .await
            .context("POST /clear échoué")?;
        Ok(())
    }
}

// ── Utilitaires ──────────────────────────────────────────────────────────────

/// Sérialiser un slice de bytes (tenseur float16 brut) en base64
pub fn bytes_to_b64(data: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    STANDARD.encode(data)
}

/// Désérialiser base64 → bytes
pub fn b64_to_bytes(s: &str) -> Result<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    Ok(STANDARD.decode(s)?)
}

/// Extraire les token IDs d'une requête de tokenisation simple
/// (utilisé pour convertir le prompt texte → IDs via le daemon)
pub fn extract_token_ids(prompt: &str) -> Vec<i32> {
    // Dans une implémentation complète, on appellerait le tokenizer local
    // Pour le MVP, le pipeline_server.py gère lui-même la tokenisation
    // → on passe le texte brut au premier nœud via l'API /tokenize si besoin
    vec![]
}
