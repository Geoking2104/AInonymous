use thiserror::Error;

#[derive(Error, Debug)]
pub enum AInonymousError {
    #[error("Aucun nœud disponible pour le modèle '{model}'")]
    NoCapableNode { model: String },

    #[error("Modèle '{model}' non reconnu")]
    UnknownModel { model: String },

    #[error("Connexion QUIC échouée vers {peer}: {reason}")]
    QuicConnectFailed { peer: String, reason: String },

    #[error("Transfer d'activations interrompu: {reason}")]
    ActivationTransferFailed { reason: String },

    #[error("Session QUIC expirée (token invalide)")]
    QuicSessionExpired,

    #[error("Holochain conductor inaccessible: {reason}")]
    ConductorUnavailable { reason: String },

    #[error("Zome call échouée: {zome}::{function} — {reason}")]
    ZomeCallFailed { zome: String, function: String, reason: String },

    #[error("llama-server inaccessible sur le port {port}")]
    LlamaServerUnavailable { port: u16 },

    #[error("Requête malformée: {reason}")]
    BadRequest { reason: String },

    #[error("Mesh saturé, réessayer dans {retry_after_seconds}s")]
    MeshOverloaded { retry_after_seconds: u32 },

    #[error("Erreur interne: {0}")]
    Internal(String),
}

impl AInonymousError {
    pub fn http_status(&self) -> u16 {
        match self {
            Self::NoCapableNode { .. } => 503,
            Self::UnknownModel { .. } => 404,
            Self::QuicConnectFailed { .. } => 503,
            Self::ActivationTransferFailed { .. } => 503,
            Self::QuicSessionExpired => 503,
            Self::ConductorUnavailable { .. } => 503,
            Self::ZomeCallFailed { .. } => 503,
            Self::LlamaServerUnavailable { .. } => 503,
            Self::BadRequest { .. } => 400,
            Self::MeshOverloaded { .. } => 429,
            Self::Internal(_) => 500,
        }
    }
}

impl From<anyhow::Error> for AInonymousError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}
