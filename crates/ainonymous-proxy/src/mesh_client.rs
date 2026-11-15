use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};

use ainonymous_types::{ExecutionPlan, InferenceMetrics, NodeSummary};
use crate::ProxyConfig;

/// Client HTTP vers l'API du daemon `ainonymous-daemon` (qui fait les zome
/// calls Holochain) et vers llama-server local
pub struct MeshClient {
    http: Client,
    llama_url: String,
    daemon_url: String,
}

impl MeshClient {
    pub async fn new(config: &ProxyConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        Ok(Self {
            http,
            llama_url: config.llama_server_url.clone(),
            daemon_url: config.daemon_url.trim_end_matches('/').to_string(),
        })
    }

    /// URL de base du daemon (ex. `http://127.0.0.1:8890`)
    pub fn daemon_url(&self) -> &str {
        &self.daemon_url
    }

    /// Client HTTP partagé (timeouts communs)
    pub fn http(&self) -> &Client {
        &self.http
    }

    /// Vérifier si llama-server local est disponible
    pub async fn check_llama_health(&self) -> bool {
        self.http
            .get(format!("{}/health", self.llama_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Obtenir le plan d'exécution depuis Holochain
    pub async fn get_execution_plan(&self, model_id: &str) -> Result<ExecutionPlan> {
        debug!("Calcul plan d'exécution pour modèle: {}", model_id);

        // Appel au daemon ainonymous qui fait la zome call Holochain
        let resp = self.http
            .post(format!("{}/mesh/plan", self.daemon_url))
            .json(&serde_json::json!({ "model_id": model_id }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Échec plan d'exécution ({}): {}", status, body);
        }

        Ok(resp.json::<ExecutionPlan>().await?)
    }

    /// Récupérer liste des nœuds disponibles pour un modèle
    /// (forme `NodeSummary`, identique à la réponse du daemon)
    pub async fn get_available_nodes(&self, model_id: &str) -> Result<Vec<NodeSummary>> {
        let resp = self.http
            .get(format!("{}/mesh/nodes", self.daemon_url))
            .query(&[("model_id", model_id)])
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("daemon /mesh/nodes ({}): {}", status, body);
        }

        Ok(resp.json::<Vec<NodeSummary>>().await?)
    }

    /// Publier les métriques d'une requête sur le DHT Holochain
    pub async fn publish_metrics(&self, metrics: &InferenceMetrics) {
        match self.http
            .post(format!("{}/mesh/metrics", self.daemon_url))
            .json(metrics)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => debug!("Métriques publiées ({})", metrics.request_id),
            Ok(r) => warn!("Publication des métriques refusée par le daemon: {}", r.status()),
            Err(e) => warn!("Impossible de publier les métriques: {}", e),
        }
    }

    /// Envoyer une requête à llama-server local (inférence solo)
    pub async fn llama_chat(
        &self,
        request: &Value,
    ) -> Result<reqwest::Response> {
        let resp = self.http
            .post(format!("{}/v1/chat/completions", self.llama_url))
            .json(request)
            .send()
            .await?;
        Ok(resp)
    }

    /// Vérifier la santé du mesh via le daemon
    pub async fn get_mesh_status(&self) -> Result<Value> {
        let resp = self.http
            .get(format!("{}/mesh/status", self.daemon_url))
            .send()
            .await?;
        Ok(resp.json().await?)
    }

    /// Lancer une inférence distribuée (pipeline-split) via le daemon.
    pub async fn run_mesh_inference(
        &self,
        model_id: &str,
        messages: &Value,
        max_tokens: u32,
    ) -> Result<Value> {
        let resp = self.http
            .post(format!("{}/mesh/infer", self.daemon_url))
            .json(&serde_json::json!({
                "model_id": model_id,
                "messages": messages,
                "max_tokens": max_tokens,
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("mesh /infer: {}", body);
        }
        Ok(resp.json().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::Query,
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn config(daemon_url: String) -> ProxyConfig {
        ProxyConfig {
            host: "127.0.0.1".into(),
            port: 0,
            llama_server_url: "http://127.0.0.1:1".into(),
            holochain_ws_url: "ws://127.0.0.1:1".into(),
            daemon_url,
        }
    }

    /// Faux daemon reproduisant les réponses réelles de `ainonymous-daemon`.
    async fn spawn_fake_daemon(received: Arc<Mutex<Vec<serde_json::Value>>>) -> String {
        let app = Router::new()
            .route(
                "/mesh/nodes",
                get(|Query(q): Query<HashMap<String, String>>| async move {
                    let n = if q.get("model_id").map(String::as_str) == Some("gemma4-e4b") { 2 } else { 0 };
                    let nodes: Vec<_> = (0..n)
                        .map(|i| serde_json::json!({
                            "agent_id": format!("agent-{i}"),
                            "vram_gb": 8.0,
                            "current_load": 0.1,
                            "available_slots": 1,
                            "quic_endpoint": "127.0.0.1:9010",
                            "region_hint": null,
                            "score": 0.9
                        }))
                        .collect();
                    Json(nodes)
                }),
            )
            .route(
                "/mesh/metrics",
                post(move |Json(v): Json<serde_json::Value>| {
                    let received = received.clone();
                    async move {
                        received.lock().unwrap().push(v);
                        (StatusCode::CREATED, Json(serde_json::json!({ "action_hash": "uhCkk" })))
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn nodes_from_daemon_deserialize_and_are_counted() {
        let url = spawn_fake_daemon(Arc::default()).await;
        let client = MeshClient::new(&config(url)).await.unwrap();
        let nodes = client.get_available_nodes("gemma4-e4b").await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].agent_id, "agent-0");
        assert!(client.get_available_nodes("qwen3-8b").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn daemon_error_is_reported_not_hidden() {
        // Port fermé : erreur explicite (le handler /v1/models compte alors 0 nœud)
        let client = MeshClient::new(&config("http://127.0.0.1:1".into())).await.unwrap();
        assert!(client.get_available_nodes("gemma4-e4b").await.is_err());
    }

    #[tokio::test]
    async fn metrics_are_posted_to_daemon() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let url = spawn_fake_daemon(received.clone()).await;
        let client = MeshClient::new(&config(url)).await.unwrap();
        let m = InferenceMetrics {
            request_id: uuid::Uuid::new_v4(),
            model_id: "gemma4-e4b".into(),
            total_latency_ms: 120,
            tokens_per_second: 10.0,
            prompt_tokens: 5,
            completion_tokens: 12,
            nodes_used: 1,
            execution_mode: "solo".into(),
            success: true,
            error_reason: None,
        };
        client.publish_metrics(&m).await;
        let got = received.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["completion_tokens"], 12);
        assert_eq!(got[0]["request_id"], m.request_id.to_string());
    }
}
