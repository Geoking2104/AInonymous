use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use anyhow::Result;
use ainonymous_types::{NodeCapabilities, NodeHeartbeat};
use crate::{mesh_client::MeshClient, ProxyConfig};

/// État partagé du proxy (injecté dans tous les handlers via Arc)
pub struct AppState {
    pub config: ProxyConfig,
    pub mesh: MeshClient,
    pub local_models: Arc<RwLock<HashMap<String, ModelStatus>>>,
    pub metrics: Arc<RwLock<ProxyMetrics>>,
}

#[derive(Debug, Clone)]
pub struct ModelStatus {
    pub model_id: String,
    pub ready: bool,
    pub vram_gb: f32,
    pub context_size: u32,
    pub multimodal: bool,
    pub architecture: ModelArchitecture,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelArchitecture {
    Dense,
    MoE { active_params_b: f32 },
    DenseEdge,
}

#[derive(Debug, Default)]
pub struct ProxyMetrics {
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_failed: u64,
    pub tokens_generated: u64,
    pub avg_latency_ms: f32,
    pub start_time: std::time::Instant,
}

impl AppState {
    pub async fn new(config: ProxyConfig) -> Result<Self> {
        let mesh = MeshClient::new(&config).await?;

        // Modèles connus avec métadonnées
        let mut local_models = HashMap::new();
        for (id, status) in default_model_registry() {
            local_models.insert(id, status);
        }

        Ok(Self {
            config,
            mesh,
            local_models: Arc::new(RwLock::new(local_models)),
            metrics: Arc::new(RwLock::new(ProxyMetrics {
                start_time: std::time::Instant::now(),
                ..Default::default()
            })),
        })
    }

    pub fn get_model_status(&self, model_id: &str) -> Option<ModelStatus> {
        self.local_models.read().ok()?.get(model_id).cloned()
    }

    pub fn record_request(&self, success: bool, latency_ms: u32, tokens: u32) {
        if let Ok(mut m) = self.metrics.write() {
            m.requests_total += 1;
            if success { m.requests_success += 1; } else { m.requests_failed += 1; }
            m.tokens_generated += tokens as u64;
            let n = m.requests_total as f32;
            m.avg_latency_ms = (m.avg_latency_ms * (n - 1.0) + latency_ms as f32) / n;
        }
    }
}

fn default_model_registry() -> Vec<(String, ModelStatus)> {
    vec![
        ("gemma4-e2b".into(), ModelStatus {
            model_id: "gemma4-e2b".into(),
            ready: false,
            vram_gb: 3.0,
            context_size: 131072,
            multimodal: true,
            architecture: ModelArchitecture::DenseEdge,
        }),
        ("gemma4-e4b".into(), ModelStatus {
            model_id: "gemma4-e4b".into(),
            ready: false,
            vram_gb: 5.0,
            context_size: 131072,
            multimodal: true,
            architecture: ModelArchitecture::DenseEdge,
        }),
        ("gemma4-26b-moe".into(), ModelStatus {
            model_id: "gemma4-26b-moe".into(),
            ready: false,
            vram_gb: 18.0,
            context_size: 262144,
            multimodal: true,
            architecture: ModelArchitecture::MoE { active_params_b: 4.0 },
        }),
        ("gemma4-31b".into(), ModelStatus {
            model_id: "gemma4-31b".into(),
            ready: false,
            vram_gb: 20.0,
            context_size: 131072,
            multimodal: true,
            architecture: ModelArchitecture::Dense,
        }),
        ("qwen3-8b".into(), ModelStatus {
            model_id: "qwen3-8b".into(),
            ready: false,
            vram_gb: 5.0,
            context_size: 32768,
            multimodal: false,
            architecture: ModelArchitecture::Dense,
        }),
        ("qwen3-32b".into(), ModelStatus {
            model_id: "qwen3-32b".into(),
            ready: false,
            vram_gb: 20.0,
            context_size: 32768,
            multimodal: false,
            architecture: ModelArchitecture::Dense,
        }),
    ]
}
