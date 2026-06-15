//! Prometheus + OpenTelemetry integration for HybridNode.

use crate::config::ObservabilityConfig;
use anyhow::Result;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Shared metrics state.
pub struct HybridNodeMetrics {
    pub registry: Registry,
    pub requests_total: Counter,
    pub scheduling_decisions_total: Counter,
    pub sdwan_link_latency_ms: Gauge<f64, std::sync::atomic::AtomicU64>,
    pub vram_used_bytes: Gauge,
}

impl HybridNodeMetrics {
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let requests_total = Counter::default();
        registry.register(
            "ainonymous_requests",
            "Total inference requests",
            requests_total.clone(),
        );

        let scheduling_decisions_total = Counter::default();
        registry.register(
            "hybridnode_scheduler_decisions",
            "Total scheduling decisions made",
            scheduling_decisions_total.clone(),
        );

        let sdwan_link_latency_ms: Gauge<f64, std::sync::atomic::AtomicU64> = Gauge::default();
        registry.register(
            "hybridnode_sdwan_link_latency_ms",
            "Current SD-WAN link latency in ms",
            sdwan_link_latency_ms.clone(),
        );

        let vram_used_bytes: Gauge = Gauge::default();
        registry.register(
            "ainonymous_vram_used_bytes",
            "VRAM currently in use by loaded models",
            vram_used_bytes.clone(),
        );

        Self {
            registry,
            requests_total,
            scheduling_decisions_total,
            sdwan_link_latency_ms,
            vram_used_bytes,
        }
    }
}

impl Default for HybridNodeMetrics {
    fn default() -> Self { Self::new() }
}

/// Start the Prometheus HTTP scrape endpoint.
pub async fn start_prometheus(config: &ObservabilityConfig) -> Result<()> {
    if !config.prometheus {
        return Ok(());
    }

    let addr: std::net::SocketAddr = config.prometheus_addr.parse()?;
    let metrics = Arc::new(Mutex::new(HybridNodeMetrics::new()));

    info!("Prometheus metrics endpoint starting on {addr}");

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.expect("bind metrics port");
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let metrics = metrics.clone();
                tokio::spawn(async move {
                    let m = metrics.lock().await;
                    let mut buf = String::new();
                    encode(&mut buf, &m.registry).unwrap_or(());
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                        buf.len(),
                        buf
                    );
                    use tokio::io::AsyncWriteExt;
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        }
    });

    Ok(())
}
