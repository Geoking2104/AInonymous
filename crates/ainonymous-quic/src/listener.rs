use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use anyhow::Result;
use tracing::{debug, info, warn};

use crate::{QuicError, SessionOffer, SESSION_TOKEN_TTL_SECS};

/// Listener QUIC côté nœud worker
/// Attend les connexions entrantes, vérifie les tokens de session
pub struct QuicListener {
    endpoint: quinn::Endpoint,
    pending_sessions: Arc<Mutex<HashMap<Vec<u8>, SessionOffer>>>,
}

impl QuicListener {
    pub async fn new(bind_addr: SocketAddr) -> Result<Self> {
        let endpoint = crate::create_endpoint(Some(bind_addr)).await?;
        info!("QUIC listener démarré sur {}", endpoint.local_addr()?);

        Ok(Self {
            endpoint,
            pending_sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Enregistrer une offre de session (appelé après négociation Holochain)
    pub fn register_session(&self, offer: SessionOffer) {
        let mut sessions = self.pending_sessions.lock().unwrap();
        // Nettoyer les sessions expirées
        let now_ms = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        sessions.retain(|_, v| v.expires_at_unix_ms > now_ms);
        // Enregistrer la nouvelle
        sessions.insert(offer.session_token.clone(), offer);
        debug!("Session enregistrée ({} sessions actives)", sessions.len());
    }

    /// Boucle principale d'acceptation des connexions QUIC entrantes
    pub async fn run<F, Fut>(&self, handler: F)
    where
        F: Fn(quinn::Connection, SessionOffer) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        info!("Attente connexions QUIC...");
        while let Some(incoming) = self.endpoint.accept().await {
            let sessions = self.pending_sessions.clone();
            let handler = handler.clone();

            tokio::spawn(async move {
                match Self::accept_connection(incoming, sessions, handler).await {
                    Ok(()) => debug!("Connexion QUIC traitée"),
                    Err(e) => warn!("Erreur connexion QUIC: {}", e),
                }
            });
        }
    }

    async fn accept_connection<F, Fut>(
        incoming: quinn::Connecting,
        sessions: Arc<Mutex<HashMap<Vec<u8>, SessionOffer>>>,
        handler: F,
    ) -> Result<(), QuicError>
    where
        F: Fn(quinn::Connection, SessionOffer) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let conn = incoming.await
            .map_err(|e| QuicError::ConnectFailed(e.to_string()))?;

        debug!("Connexion QUIC entrante depuis {}", conn.remote_address());

        // Lire le token d'authentification
        let mut auth_stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            conn.accept_uni()
        )
        .await
        .map_err(|_| QuicError::ConnectTimeout)?
        .map_err(|e| QuicError::StreamError(e.to_string()))?;

        let mut token = vec![0u8; 32];
        // Lecture du token
        let mut offset = 0;
        while offset < 32 {
            let chunk = auth_stream.read_chunk(32 - offset, true).await
                .map_err(|e| QuicError::StreamError(e.to_string()))?
                .ok_or(QuicError::StreamError("auth stream fermé".into()))?;
            let n = chunk.bytes.len();
            token[offset..offset + n].copy_from_slice(&chunk.bytes);
            offset += n;
        }

        // Vérifier le token
        let offer = {
            let mut sessions = sessions.lock().unwrap();
            let offer = sessions.remove(&token)
                .ok_or(QuicError::InvalidSessionToken)?;
            if offer.is_expired() {
                return Err(QuicError::SessionExpired);
            }
            offer
        };

        info!("Session QUIC authentifiée (couches {:?})", offer.layer_range);
        handler(conn, offer).await;
        Ok(())
    }
}
