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

    // Palier D+E : charger (ou générer + persister) l'identité ed25519 du nœud
    // avant toute autre opération. Priorité :
    //   1. Keyring OS natif (feature `secure-keyring`) — chiffrement matériel
    //   2. Fichier `identity_path` (config ou défaut XDG)
    // La clé publique sera annoncée dans le DHT pour le pinning mTLS (palier D).
    let identity_path = config.holochain.identity_path.clone().unwrap_or_else(|| {
        let mut p = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        p.push("ainonymous");
        p.push("node_identity.key");
        p
    });

    // load_or_generate_keyring : tente le keyring OS natif (macOS Keychain /
    // Windows Credential Manager / Linux libsecret) et retombe sur le fichier
    // si le keyring est indisponible. Toujours disponible car la dep ainonymous-quic
    // est compilée avec features = ["secure-keyring"].
    let identity = ainonymous_quic::NodeIdentity::load_or_generate_keyring(
        "ainonymous-daemon",
        "quic-node-identity",
        &identity_path,
    )?;

    info!("Identité ed25519 du nœud : {} (depuis {:?})", identity.public_key_hex(), identity_path);

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

    // Annoncer les capacités de ce nœud dans le DHT, y compris la pubkey ed25519
    // pour le pinning mTLS (palier D). Non-fatal hors Holochain.
    if let Err(e) = holochain.announce_capabilities(&config, Some(&identity.public_key_hex())).await {
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

    // Démarrer le listener QUIC (data plane, mTLS ed25519)
    let quic_addr = format!("0.0.0.0:{}", config.quic_port).parse()?;
    let quic_listener = ainonymous_quic::QuicListener::new(quic_addr, &identity).await?;
    let quic_addr_public = quic_listener.local_addr()?;
    info!("QUIC listener sur {}", quic_addr_public);

    // Handle partagé pour enregistrer les sessions depuis le plan de contrôle REST
    let registry = quic_listener.registry();
    // Second handle pour le plan de contrôle DHT (signaux du conducteur).
    let signal_registry = quic_listener.registry();

    // Adresse QUIC annoncée aux pairs : `quic_advertise` si défini (loopback /
    // IP publique), sinon l'adresse locale du listener.
    let advertise = config.quic_advertise.as_ref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(quic_addr_public);
    info!("Endpoint QUIC annoncé : {}", advertise);

    // En mode conducteur : écouter les signaux `QuicListenerSignal` pour
    // enregistrer les sessions entrantes négociées via le DHT (no-op en statique).
    holochain.listen_quic_signals(signal_registry, advertise, identity.clone()).await;

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
        config.clone(),
        identity_path.clone(),
    );
    let addr = format!("127.0.0.1:{}", config.daemon_port);
    info!("Daemon REST interne sur http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app).await?;
    Ok(())
}