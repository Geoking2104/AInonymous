mod config;
mod conductor;
mod holochain;
mod llama;
mod router;
mod heartbeat;

use std::sync::Arc;
use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

pub use config::DaemonConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive("ainonymous_daemon=debug".parse()?))
        .init();

    info!("AInonymous daemon v{}", env!("CARGO_PKG_VERSION"));

    let config = DaemonConfig::load()?;
    info!("Config chargée depuis {:?}", config.config_path());

    // Démarrer les sous-systèmes
    let conductor = Arc::new(conductor::Conductor::new(config.clone()).await?);

    // Démarrer llama-server si pas déjà en cours
    let llama = llama::LlamaManager::new(config.clone());
    if !llama.is_running().await {
        info!("Démarrage llama-server...");
        llama.start().await?;
    }

    // Connexion au conducteur Holochain
    info!("Connexion au conducteur Holochain...");
    let holochain = holochain::HolochainClient::connect(&config).await?;

    // Annoncer les capacités de ce nœud dans le DHT
    holochain.announce_capabilities(&config).await?;
    info!("Capacités annoncées dans le mesh");

    // Démarrer le heartbeat périodique (toutes les 30s)
    let hb_holochain = holochain.clone();
    let hb_config = config.clone();
    tokio::spawn(async move {
        heartbeat::run_heartbeat(hb_holochain, hb_config).await;
    });

    // Démarrer le serveur REST interne (pour le proxy)
    let app = router::build(conductor.clone(), holochain.clone());
    let addr = format!("127.0.0.1:{}", config.daemon_port);
    info!("Daemon REST interne sur http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Démarrer le listener QUIC
    let quic_addr = format!("0.0.0.0:{}", config.quic_port).parse()?;
    let quic_listener = ainonymous_quic::QuicListener::new(quic_addr).await?;
    let quic_addr_public = quic_listener.local_addr()?;
    info!("QUIC listener sur {}", quic_addr_public);

    // Annoncer l'adresse QUIC dans le DHT
    holochain.update_quic_endpoint(quic_addr_public).await?;

    // Lancer le listener QUIC en background
    let hl = holochain.clone();
    tokio::spawn(async move {
        quic_listener.run(move |conn, offer| {
            let hl = hl.clone();
            async move {
                info!("Session QUIC entrante, couches: {:?}", offer.layer_range);
                // Traiter la session (inférence des couches assignées)
                if let Err(e) = conductor::handle_pipeline_session(conn, offer, &hl).await {
                    error!("Erreur session pipeline: {}", e);
                }
            }
        }).await;
    });

    axum::serve(listener, app).await?;
    Ok(())
}
