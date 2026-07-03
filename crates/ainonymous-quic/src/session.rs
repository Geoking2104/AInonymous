use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::Result;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{QuicError, SESSION_TOKEN_TTL_SECS};

/// Offre de session retournée au nœud demandeur (via Holochain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOffer {
    pub quic_endpoint: Option<SocketAddr>,
    pub session_token: Vec<u8>,    // 32 bytes aléatoires
    pub expires_at_unix_ms: u64,
    pub layer_range: Option<(u32, u32)>,
    pub expert_ids: Option<Vec<u32>>,
    /// Agent Holochain du nœud suivant dans la chaîne pipeline
    pub next_agent_id: Option<String>,
    /// Tranche de couches du nœud suivant
    pub next_layer_range: Option<(u32, u32)>,
    /// Clé publique ed25519 (32 octets) attendue du SERVEUR, pour le pinning
    /// mTLS côté client. None = repli sans pinning d'identité.
    #[serde(default)]
    pub peer_pubkey: Option<[u8; 32]>,
    /// Clé publique ed25519 (32 octets) attendue du CLIENT (coordinateur ou
    /// nœud intermédiaire). Fournie lors de la négociation ; utilisée par le
    /// listener QUIC pour vérifier côté serveur que le cert TLS client correspond
    /// à l'agent identifié dans le plan de contrôle.
    /// None = accepter tout cert ed25519 valide (repli testnet/bootstrap statique).
    #[serde(default)]
    pub client_pubkey: Option<[u8; 32]>,
}

impl SessionOffer {
    pub fn new(endpoint: SocketAddr, layer_range: Option<(u32, u32)>) -> Self {
        let mut token = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut token);
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
            + SESSION_TOKEN_TTL_SECS * 1000;
        Self {
            quic_endpoint: Some(endpoint),
            session_token: token,
            expires_at_unix_ms: expires_at,
            layer_range,
            expert_ids: None,
            next_agent_id: None,
            next_layer_range: None,
            peer_pubkey: None,
            client_pubkey: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        now_ms > self.expires_at_unix_ms
    }
}

/// Configuration d'une session QUIC
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub connect_timeout: Duration,
    pub stream_timeout: Duration,
    pub bandwidth_bps: Option<u64>,   // estimé pour décider de la compression
    pub compress: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            stream_timeout: Duration::from_secs(30),
            bandwidth_bps: None,
            compress: false,
        }
    }
}

/// Session QUIC active entre deux nœuds
pub struct QuicSession {
    pub connection: quinn::Connection,
    pub offer: SessionOffer,
    pub config: SessionConfig,
    pub established_at: Instant,
}

impl QuicSession {
    /// Établir une connexion QUIC vers un nœud distant
    /// Le token de session doit avoir été négocié au préalable (plan de contrôle)
    pub async fn connect(
        endpoint: &quinn::Endpoint,
        offer: SessionOffer,
        config: SessionConfig,
        identity: &crate::mtls::NodeIdentity,
    ) -> Result<Self, QuicError> {
        if offer.is_expired() {
            return Err(QuicError::SessionExpired);
        }

        let addr = offer.quic_endpoint
            .ok_or_else(|| QuicError::ConnectFailed("endpoint QUIC manquant dans l'offre".into()))?;

        debug!("Connexion QUIC vers {}", addr);

        // mTLS : on présente notre certificat ed25519 et on vérifie que le
        // certificat serveur porte la clé attendue (offer.peer_pubkey).
        let (cert, key) = identity.tls_cert()
            .map_err(|e| QuicError::ConnectFailed(format!("cert ed25519: {}", e)))?;
        let client_tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(
                crate::mtls::PeerKeyVerifier::new(offer.peer_pubkey),
            ))
            .with_client_auth_cert(vec![cert], key)
            .map_err(|e| QuicError::ConnectFailed(e.to_string()))?;

        let quic_tls = quinn::crypto::rustls::QuicClientConfig::try_from(client_tls)
            .map_err(|e| QuicError::ConnectFailed(e.to_string()))?;
        let client_config = quinn::ClientConfig::new(std::sync::Arc::new(quic_tls));

        let conn = tokio::time::timeout(
            config.connect_timeout,
            endpoint.connect_with(client_config, addr, "ainonymous.local")
                .map_err(|e| QuicError::ConnectFailed(e.to_string()))?
        )
        .await
        .map_err(|_| QuicError::ConnectTimeout)?
        .map_err(|e| QuicError::ConnectFailed(e.to_string()))?;

        // Authentifier avec le session token
        let mut auth_stream = conn.open_uni().await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        auth_stream.write_all(&offer.session_token).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        auth_stream.finish()
            .map_err(|e| QuicError::StreamError(e.to_string()))?;

        info!("Session QUIC établie → {}", addr);

        Ok(Self {
            connection: conn,
            offer,
            config,
            established_at: Instant::now(),
        })
    }

    /// Fermer proprement la session
    pub fn close(self) {
        self.connection.close(0u32.into(), b"done");
        debug!("Session QUIC fermée (durée: {:?})", self.established_at.elapsed());
    }

    /// Durée de la session
    pub fn age(&self) -> Duration {
        self.established_at.elapsed()
    }
}
