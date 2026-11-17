use std::sync::OnceLock;

use anyhow::Result;
use reqwest::{Client, Url};
use serde_json::Value;
use tracing::{debug, warn};

use ainonymous_types::{ExecutionPlan, InferenceMetrics, NodeSummary};
use crate::ProxyConfig;

/// URL de base de l'API du daemon, fixée une seule fois au démarrage à partir
/// de la configuration (`AINON_DAEMON_URL`, cf. `ainonymous_types::endpoints`).
/// Elle n'est jamais dérivée d'une requête entrante.
static DAEMON_BASE: OnceLock<Url> = OnceLock::new();

/// Valide une URL de base de daemon : http(s), avec hôte, sans query ni fragment.
pub fn parse_daemon_base(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw.trim())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        anyhow::bail!("URL de daemon invalide (http/https attendu) : {raw}");
    }
    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("URL de daemon invalide (query/fragment interdits) : {raw}");
    }
    if !url.path().ends_with('/') {
        let p = format!("{}/", url.path());
        url.set_path(&p);
    }
    Ok(url)
}

fn daemon_base() -> &'static Url {
    DAEMON_BASE.get_or_init(|| {
        let raw = ainonymous_types::endpoints::daemon_url_from_env().url;
        parse_daemon_base(&raw).unwrap_or_else(|e| {
            warn!("{e} ; utilisation de http://127.0.0.1:8890");
            Url::parse("http://127.0.0.1:8890/").expect("URL constante valide")
        })
    })
}

/// URL d'un endpoint du daemon : base fixe + chemin constant.
pub fn daemon_endpoint(path: &'static str) -> Url {
    daemon_base()
        .join(path.trim_start_matches('/'))
        .expect("chemin constant valide")
}

/// Client HTTP vers l'API du daemon `ainonymous-daemon` (qui fait les zome
/// calls Holochain) et vers llama-server local
pub struct MeshClient {
    http: Client,
    llama_url: String,
}

impl MeshClient {
    pub async fn new(config: &ProxyConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        let base = parse_daemon_base(&config.daemon_url)?;
        if DAEMON_BASE.set(base.clone()).is_err() && DAEMON_BASE.get() != Some(&base) {
            warn!("URL du daemon déjà fixée à {} ; {} ignorée", daemon_base(), base);
        }

        Ok(Self {
            http,
            llama_url: config.llama_server_url.clone(),
        })
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
            .post(daemon_endpoint("mesh/plan"))
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
    /// (forme `NodeSummary`, identique à la réponse du daemon).
    /// `model_id` est une constante (`state::known_model_id`, ou `""` pour tous) :
    /// aucune valeur fournie par le client n'entre dans l'URL sortante.
    pub async fn get_available_nodes(&self, model_id: &'static str) -> Result<Vec<NodeSummary>> {
        // Base fixe + chemin constant ; model_id (constante de la liste blanche)
        // passe uniquement en paramètre de requête encodé.
        let resp = self.http
            .get(daemon_endpoint("mesh/nodes"))
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
            .post(daemon_endpoint("mesh/metrics"))
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
            .get(daemon_endpoint("mesh/status"))
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
            .post(daemon_endpoint("mesh/infer"))
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
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Métriques reçues par le faux daemon (partagé par les tests du binaire).
    static RECEIVED: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
    /// Requêtes brutes (`query string`) reçues sur /mesh/nodes.
    static RAW_QUERIES: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Faux daemon unique (l'URL du daemon est fixée une seule fois par
    /// processus), servi depuis un thread dédié.
    fn fake_daemon_url() -> String {
        static URL: OnceLock<String> = OnceLock::new();
        URL.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async move {
                    let app = Router::new()
                        .route(
                            "/mesh/nodes",
                            get(|uri: axum::http::Uri, Query(q): Query<HashMap<String, String>>| async move {
                                RAW_QUERIES.lock().unwrap().push(uri.query().unwrap_or("").to_string());
                                let model = q.get("model_id").cloned().unwrap_or_default();
                                if model == "qwen3-32b" {
                                    return (StatusCode::SERVICE_UNAVAILABLE, "conducteur absent").into_response();
                                }
                                let n = if model == "gemma4-e4b" { 2 } else { 0 };
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
                                Json(nodes).into_response()
                            }),
                        )
                        .route(
                            "/mesh/metrics",
                            post(|Json(v): Json<serde_json::Value>| async move {
                                RECEIVED.lock().unwrap().push(v);
                                (StatusCode::CREATED, Json(serde_json::json!({ "action_hash": "uhCkk" })))
                            }),
                        );
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    tx.send(listener.local_addr().unwrap()).unwrap();
                    axum::serve(listener, app).await.unwrap();
                });
            });
            format!("http://{}", rx.recv().unwrap())
        })
        .clone()
    }

    async fn client() -> MeshClient {
        let config = ProxyConfig {
            host: "127.0.0.1".into(),
            port: 0,
            llama_server_url: "http://127.0.0.1:1".into(),
            holochain_ws_url: "ws://127.0.0.1:1".into(),
            daemon_url: fake_daemon_url(),
        };
        MeshClient::new(&config).await.unwrap()
    }

    #[test]
    fn daemon_base_validation() {
        assert_eq!(parse_daemon_base("http://127.0.0.1:18890").unwrap().as_str(), "http://127.0.0.1:18890/");
        assert_eq!(parse_daemon_base("https://d.example/api").unwrap().as_str(), "https://d.example/api/");
        assert!(parse_daemon_base("ftp://127.0.0.1").is_err());
        assert!(parse_daemon_base("pas une url").is_err());
        assert!(parse_daemon_base("http://127.0.0.1:8890/?x=1").is_err());
    }

    #[tokio::test]
    async fn nodes_from_daemon_deserialize_and_are_counted() {
        let client = client().await;
        let nodes = client.get_available_nodes("gemma4-e4b").await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].agent_id, "agent-0");
        assert!(client.get_available_nodes("qwen3-8b").await.unwrap().is_empty());
        // model_id uniquement en paramètre de requête, sur le chemin fixe
        assert!(RAW_QUERIES.lock().unwrap().iter().any(|q| q == "model_id=gemma4-e4b"));
        assert_eq!(daemon_endpoint("mesh/nodes").path(), "/mesh/nodes");
    }

    #[tokio::test]
    async fn daemon_error_is_reported_not_hidden() {
        // Le daemon répond 503 : erreur explicite (/v1/models compte alors 0 nœud)
        let client = client().await;
        assert!(client.get_available_nodes("qwen3-32b").await.is_err());
    }

    #[tokio::test]
    async fn metrics_are_posted_to_daemon() {
        let client = client().await;
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
        let got = RECEIVED.lock().unwrap();
        let mine: Vec<_> = got.iter().filter(|v| v["request_id"] == m.request_id.to_string()).collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0]["completion_tokens"], 12);
    }
}
