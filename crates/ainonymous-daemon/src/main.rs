// Code WIP : certaines API (load_model, métriques, champs de désérialisation,
// helpers Phase 2) ne sont pas encore toutes câblées. Évite le bruit de warnings.
#![allow(dead_code)]

mod config;
mod conductor;
mod conductor_client;
mod holochain;
mod llama;
mod router;
mod heartbeat;
mod pipeline_client;

use std::sync::Arc;
use anyhow::Result;
use tracing::{error, info, warn};
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

    // Démarrer llama-server si pas déjà en cours (non-fatal : le mode mesh/
    // pipeline n'en dépend pas).
    let llama = llama::LlamaManager::new(config.clone());
    if !llama.is_running().await {
        info!("Démarrage llama-server...");
        if let Err(e) = llama.start().await {
            warn!("llama-server non démarré ({}). Mode mesh/pipeline uniquement.", e);
        }
    }

    // Connexion au conducteur Holochain
    info!("Connexion au conducteur Holochain...");
    let holochain = holochain::HolochainClient::connect(&config).await?;

    // Annoncer les capacités de ce nœud dans le DHT (non-fatal hors Holochain)
    if let Err(e) = holochain.announce_capabilities(&config).await {
        warn!("announce_capabilities ignoré ({})", e);
    } else {
        info!("Capacités annoncées dans le mesh");
    }

    // Démarrer le heartbeat périodique (toutes les 30s)
    let hb_holochain = holochain.clone();
    let hb_config = config.clone();
    tokio::spawn(async move {
        heartbeat::run_heartbeat(hb_holochain, hb_config).await;
    });

    // Identité ed25519 du nœud (mTLS QUIC + AgentPubKey). Éphémère pour le
    // testnet ; à persister (lair-keystore) en production.
    let identity = ainonymous_quic::NodeIdentity::generate();
    info!("Identité ed25519 du nœud : {}", identity.public_key_hex());

    // Démarrer le listener QUIC (data plane, mTLS ed25519)
    let quic_addr = format!("0.0.0.0:{}", config.quic_port).parse()?;
    let quic_listener = ainonymous_quic::QuicListener::new(quic_addr, &identity).await?;
    let quic_addr_public = quic_listener.local_addr()?;
    info!("QUIC listener sur {}", quic_addr_public);

    // Handle partagé pour enregistrer les sessions depuis le plan de contrôle REST
    let registry = quic_listener.registry();

    // Adresse QUIC annoncée aux pairs : `quic_advertise` si défini (loopback /
    // IP publique), sinon l'adresse locale du listener.
    let advertise = config.quic_advertise.as_ref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(quic_addr_public);
    info!("Endpoint QUIC annoncé : {}", advertise);

    // Annoncer l'adresse QUIC dans le DHT (non-fatal hors Holochain)
    if let Err(e) = holochain.update_quic_endpoint(advertise).await {
        warn!("update_quic_endpoint ignoré ({})", e);
    }

    // Lancer le listener QUIC en background
    let hl = holochain.clone();
    let pipeline = conductor.pipeline.clone();
    let worker_identity = identity.clone();
    tokio::spawn(async move {
        quic_listener.run(move |conn, offer| {
            let hl = hl.clone();
            let pipeline = pipeline.clone();
            let identity = worker_identity.clone();
            async move {
                info!("Session QUIC entrante, couches: {:?}", offer.layer_range);
                // Traiter la session (inférence des couches assignées)
                if let Err(e) =
                    conductor::handle_pipeline_session(conn, offer, &hl, &pipeline, &identity).await
                {
                    error!("Erreur session pipeline: {}", e);
                }
            }
        }).await;
    });

    // Démarrer le serveur REST interne (pour le proxy + plan de contrôle pairs)
    let app = router::build(
        conductor.clone(),
        holochain.clone(),
        registry,
        advertise,
        identity.clone(),
    );
    let addr = format!("127.0.0.1:{}", config.daemon_port);
    info!("Daemon REST interne sur http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app).await?;
    Ok(())
}
