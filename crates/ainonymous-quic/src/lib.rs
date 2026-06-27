//! Canal QUIC inter-nœuds pour le transport des activations tensorielles.
//!
//! # Architecture
//! - Plan de contrôle : Holochain (négociation de session, routing)
//! - Plan de données  : ce crate (transfert des activations, tokens stream)
//!
//! # Protocole de session
//! 1. Nœud A demande via Holochain un token de session à Nœud B
//! 2. Nœud B ouvre un listener QUIC et retourne {endpoint, token, expiry}
//! 3. Nœud A se connecte directement en QUIC avec authentification par token
//! 4. Les activations transitent en streams QUIC binaires compressés
//! 5. Après complétion, les métriques sont publiées sur Holochain

pub mod session;
pub mod transfer;
pub mod listener;
pub mod codec;
pub mod error;
pub mod mtls;

pub use session::{QuicSession, SessionOffer, SessionConfig};
pub use transfer::{ActivationTransfer, TokenStream};
pub use listener::{QuicListener, SessionRegistry};
pub use error::QuicError;
pub use mtls::NodeIdentity;

use std::net::SocketAddr;
use anyhow::Result;

/// Créer un endpoint QUIC local avec mTLS ed25519.
///
/// Le serveur présente le certificat ed25519 de `identity` et **exige** un
/// certificat client ed25519 valide (`Ed25519ClientVerifier`). L'authentification
/// par token de session reste un second facteur (liaison à l'agent demandeur).
pub async fn create_endpoint(
    bind_addr: Option<SocketAddr>,
    identity: &mtls::NodeIdentity,
) -> Result<quinn::Endpoint> {
    // Installer le provider crypto par défaut (ring) — idempotent
    let _ = rustls::crypto::ring::default_provider().install_default();

    let addr = bind_addr.unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());

    let (cert, key) = identity.tls_cert()?;
    let tls = rustls::ServerConfig::builder()
        .with_client_cert_verifier(std::sync::Arc::new(mtls::Ed25519ClientVerifier::new()))
        .with_single_cert(vec![cert], key)?;
    let quic_sc = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| anyhow::anyhow!("QuicServerConfig: {}", e))?;

    let mut server_config = quinn::ServerConfig::with_crypto(std::sync::Arc::new(quic_sc));
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        std::time::Duration::from_secs(120).try_into()?
    ));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(15)));
    server_config.transport_config(std::sync::Arc::new(transport));

    let endpoint = quinn::Endpoint::server(server_config, addr)?;
    Ok(endpoint)
}

/// Taille maximale d'un bloc d'activations (512 MB)
pub const MAX_ACTIVATION_SIZE: usize = 512 * 1024 * 1024;

/// Seuil de bande passante pour activer la compression (1 Gbps)
pub const COMPRESSION_THRESHOLD_BPS: u64 = 1_000_000_000;

/// TTL d'un session token (30 secondes)
pub const SESSION_TOKEN_TTL_SECS: u64 = 30;
