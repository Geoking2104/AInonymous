use thiserror::Error;

#[derive(Error, Debug)]
pub enum QuicError {
    #[error("Connexion QUIC échouée: {0}")]
    ConnectFailed(String),

    #[error("Timeout de connexion QUIC")]
    ConnectTimeout,

    #[error("Erreur de stream QUIC: {0}")]
    StreamError(String),

    #[error("Token de session invalide")]
    InvalidSessionToken,

    #[error("Session expirée")]
    SessionExpired,

    #[error("Payload trop grand: {0} bytes (max: {1} bytes)")]
    PayloadTooLarge(usize, usize),

    #[error("Compression échouée: {0}")]
    CompressionFailed(String),

    #[error("Décompression échouée: {0}")]
    DecompressionFailed(String),

    #[error("Erreur TLS: {0}")]
    TlsError(String),
}

impl From<QuicError> for ainonymous_types::AInonymousError {
    fn from(e: QuicError) -> Self {
        match e {
            QuicError::ConnectFailed(r) | QuicError::StreamError(r) => {
                ainonymous_types::AInonymousError::QuicConnectFailed {
                    peer: "unknown".into(),
                    reason: r,
                }
            }
            QuicError::SessionExpired | QuicError::InvalidSessionToken => {
                ainonymous_types::AInonymousError::QuicSessionExpired
            }
            _ => ainonymous_types::AInonymousError::Internal(e.to_string()),
        }
    }
}
