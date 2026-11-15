// Code WIP : quelques champs (ex. models::source) pas encore lus.
#![allow(dead_code)]

mod handlers;
mod mesh_client;
mod router;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

pub use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("ainonymous_proxy=debug".parse()?))
        .init();

    let config = ProxyConfig::from_env();
    let state = Arc::new(AppState::new(config.clone()).await?);

    let app = router::build(state)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("AInonymous proxy démarré sur http://{} (daemon : {})", addr, config.daemon_url);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
    pub llama_server_url: String,
    pub holochain_ws_url: String,
    /// URL de base de l'API HTTP du daemon (`ainonymous-daemon`), sans `/` final.
    /// Source : `AINON_DAEMON_URL`, sinon `AINON_HOLOCHAIN_APP_PORT` (déprécié),
    /// sinon `http://127.0.0.1:8890` (port `daemon_port` par défaut du daemon).
    pub daemon_url: String,
}

impl ProxyConfig {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("AINON_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("AINON_PORT")
                .ok().and_then(|p| p.parse().ok()).unwrap_or(9337),
            llama_server_url: std::env::var("AINON_LLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            holochain_ws_url: std::env::var("AINON_HOLOCHAIN_URL")
                .unwrap_or_else(|_| "ws://127.0.0.1:8888".into()),
            daemon_url: resolve_daemon_url_logged(),
        }
    }
}

fn resolve_daemon_url_logged() -> String {
    use ainonymous_types::endpoints::{daemon_url_from_env, DaemonUrlSource, LEGACY_DAEMON_PORT_ENV};
    let resolved = daemon_url_from_env();
    if resolved.source == DaemonUrlSource::LegacyPortEnv {
        tracing::warn!(
            "{} est déprécié pour le proxy : utilisez AINON_DAEMON_URL={}",
            LEGACY_DAEMON_PORT_ENV,
            resolved.url
        );
    }
    resolved.url
}
