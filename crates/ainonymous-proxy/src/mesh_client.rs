use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};

use ainonymous_types::{ExecutionPlan, InferenceMetrics, NodeCapabilities};
use crate::ProxyConfig;

/// Client HTTP vers le daemon Holochain (via app websocket)
/// et vers llama-server local
pub struct MeshClient {
    http: Client,
    llama_url: String,
    holochain_url: String,
}

impl MeshClient {
    pub async fn new(config: &ProxyConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        Ok(Self {
            http,
            llama_url: config.llama_server_url.clone(),
            holochain_url: format!("http://127.0.0.1:{}", config.holochain_app_port),
        })
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
            .post(format!("{}/mesh/plan", self.holochain_url))
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
    pub async fn get_available_nodes(&self, model_id: &str) -> Result<Vec<NodeCapabilities>> {
        let resp = self.http
            .get(format!("{}/mesh/nodes", self.holochain_url))
            .query(&[("model_id", model_id)])
            .send()
            .await?;

        Ok(resp.json::<Vec<NodeCapabilities>>().await?)
    }

    /// Publier les métriques d'une requête sur le DHT Holochain
    pub async fn publish_metrics(&self, metrics: &InferenceMetrics) {
        if let Err(e) = self.http
            .post(format!("{}/mesh/metrics", self.holochain_url))
            .json(metrics)
            .send()
            .await
        {
            warn!("Impossible de publier les métriques: {}", e);
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
            .get(format!("{}/mesh/status", self.holochain_url))
            .send()
            .await?;
        Ok(resp.json().await?)
    }
}
