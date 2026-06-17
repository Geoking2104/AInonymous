use thiserror::Error;

#[derive(Debug, Error)]
pub enum HybridNodeError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Identity/keystore error: {0}")]
    Identity(String),

    #[error("Holochain conductor error: {0}")]
    Holochain(String),

    #[error("SD-WAN API error: {0}")]
    Sdwan(String),

    #[error("QUIC/mTLS error: {0}")]
    Quic(String),

    #[error("Scheduling error: {0}")]
    Scheduler(String),

    #[error("Model validation error: {0}")]
    ModelValidation(String),

    #[error("Observability error: {0}")]
    Observability(String),

    #[error("Warrant published for peer {peer_id}: {reason}")]
    WarrantPublished { peer_id: String, reason: String },

    #[error("SLA violation on link {link_id}: latency={latency_ms}ms budget={budget_ms}ms")]
    SlaViolation { link_id: String, latency_ms: f64, budget_ms: f64 },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
