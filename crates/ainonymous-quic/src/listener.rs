use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use anyhow::Result;
use tracing::{debug, info, warn};

use hex;
use crate::{QuicError, SessionOffer};

/// Registre partageable des sessions en attente.
/// Permet au plan de contrôle (REST/Holochain) d'enregistrer une offre de
/// session avant que le pair distant n'ouvre la connexion QUIC.
#[derive(Clone)]
pub struct SessionRegistry {
    pending: Arc<Mutex<HashMap<Vec<u8>, SessionOffer>>>,
}

impl SessionRegistry {
    /// Enregistrer une offre de session (indexée par son token).
    /// Purge au passage les sessions expirées.
    pub fn register(&self, offer: SessionOffer) {
        let mut sessions = self.pending.lock().unwrap();
        sessions.retain(|_, v| !v.is_expired());
        let token = offer.session_token.clone();
        sessions.insert(token, offer);
        debug!("Session enregistrée ({} sessions actives)", sessions.len());
    }
}

/// Listener QUIC côté nœud worker
/// Attend les connexions entrantes, vérifie les tokens de session
pub struct QuicListener {
    endpoint: quinn::Endpoint,
    pending_sessions: Arc<Mutex<HashMap<Vec<u8>, SessionOffer>>>,
}

impl QuicListener {
    pub async fn new(bind_addr: SocketAddr, identity: &crate::mtls::NodeIdentity) -> Result<Self> {
        let endpoint = crate::create_endpoint(Some(bind_addr), identity).await?;
        info!("QUIC listener démarré sur {}", endpoint.local_addr()?);

        Ok(Self {
            endpoint,
            pending_sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Obtenir un handle partageable pour enregistrer des sessions depuis
    /// le plan de contrôle (router REST du daemon).
    pub fn registry(&self) -> SessionRegistry {
        SessionRegistry { pending: self.pending_sessions.clone() }
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
        incoming: quinn::Incoming,
        sessions: Arc<Mutex<HashMap<Vec<u8>, SessionOffer>>>,
        handler: F,
    ) -> Result<(), QuicError>
    where
        F: Fn(quinn::Connection, SessionOffer) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let conn = incoming.accept()
            .map_err(|e| QuicError::ConnectFailed(e.to_string()))?
            .await
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

        // T3.2b — vérification mTLS côté serveur : si l'offre spécifie une clé
        // client attendue, on extrait la clé publique du certificat TLS présenté
        // lors du handshake et on vérifie qu'elle correspond.
        if let Some(expected_key) = offer.client_pubkey {
            let actual_key = extract_peer_pubkey(&conn)
                .ok_or_else(|| QuicError::ConnectFailed(
                    "cert client absent ou non-ed25519 après handshake".into()
                ))?;
            if actual_key != expected_key {
                warn!(
                    "mTLS: clé client reçue {} ≠ attendue {} — connexion refusée",
                    hex::encode(actual_key),
                    hex::encode(expected_key)
                );
                return Err(QuicError::ConnectFailed(
                    "clé publique client non autorisée".into()
                ));
            }
            info!("mTLS client vérifié: {}", hex::encode(actual_key));
        }

        info!("Session QUIC authentifiée (couches {:?})", offer.layer_range);
        handler(conn, offer).await;
        Ok(())
    }
}

/// Extrait la clé publique ed25519 (32 octets) du certificat TLS présenté par
/// le pair lors du handshake QUIC.
///
/// Pour une connexion QUIC rustls, `peer_identity()` retourne
/// `Box<Vec<CertificateDer<'static>>>`.
fn extract_peer_pubkey(conn: &quinn::Connection) -> Option<[u8; 32]> {
    let any_id = conn.peer_identity()?;
    let certs = any_id
        .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
        .ok()?;
    let cert = certs.first()?;
    crate::mtls::ed25519_pubkey_from_cert(cert).ok()
}
