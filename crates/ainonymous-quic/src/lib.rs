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

pub use session::{QuicSession, SessionOffer, SessionConfig};
pub use transfer::{ActivationTransfer, TokenStream};
pub use listener::QuicListener;
pub use error::QuicError;

use std::net::SocketAddr;
use anyhow::Result;

/// Point d'entrée principal : créer un endpoint QUIC local
pub async fn create_endpoint(bind_addr: Option<SocketAddr>) -> Result<quinn::Endpoint> {
    let addr = bind_addr.unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());

    // TLS self-signed pour le transport QUIC
    // (l'authentification réelle est faite via le session token Holochain)
    let (certs, key) = generate_self_signed_cert()?;

    let mut server_config = quinn::ServerConfig::with_single_cert(certs, key)?;
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        std::time::Duration::from_secs(120).try_into()?
    ));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(15)));
    server_config.transport_config(std::sync::Arc::new(transport));

    let endpoint = quinn::Endpoint::server(server_config, addr)?;
    Ok(endpoint)
}

fn generate_self_signed_cert() -> Result<(Vec<rustls::Certificate>, rustls::PrivateKey)> {
    let cert = rcgen::generate_simple_self_signed(vec!["ainonymous.local".to_string()])?;
    let cert_der = cert.serialize_der()?;
    let key_der = cert.serialize_private_key_der();
    Ok((
        vec![rustls::Certificate(cert_der)],
        rustls::PrivateKey(key_der),
    ))
}

/// Taille maximale d'un bloc d'activations (512 MB)
pub const MAX_ACTIVATION_SIZE: usize = 512 * 1024 * 1024;

/// Seuil de bande passante pour activer la compression (1 Gbps)
pub const COMPRESSION_THRESHOLD_BPS: u64 = 1_000_000_000;

/// TTL d'un session token (30 secondes)
pub const SESSION_TOKEN_TTL_SECS: u64 = 30;
