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
    ) -> Result<Self, QuicError> {
        if offer.is_expired() {
            return Err(QuicError::SessionExpired);
        }

        let addr = offer.quic_endpoint
            .ok_or_else(|| QuicError::ConnectFailed("endpoint QUIC manquant dans l'offre".into()))?;

        debug!("Connexion QUIC vers {}", addr);

        // Connexion QUIC (TLS skip verify pour cert self-signed entre nœuds connus,
        // l'authentification réelle se fait via le session token)
        let client_tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(SkipVerification))
            .with_no_client_auth();

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

/// Vérificateur TLS qui accepte tous les certificats self-signed
/// (l'authentification est faite via le session token)
#[derive(Debug)]
struct SkipVerification;

impl rustls::client::danger::ServerCertVerifier for SkipVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        use rustls::SignatureScheme::*;
        vec![
            RSA_PKCS1_SHA256, RSA_PKCS1_SHA384, RSA_PKCS1_SHA512,
            ECDSA_NISTP256_SHA256, ECDSA_NISTP384_SHA384,
            RSA_PSS_SHA256, RSA_PSS_SHA384, RSA_PSS_SHA512,
            ED25519,
        ]
    }
}
