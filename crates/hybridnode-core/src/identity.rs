//! Identity management — wraps Holochain lair-keystore.
//!
//! The AgentPubKey (ed25519) serves triple duty:
//!   1. DHT identity in Holochain
//!   2. QUIC/mTLS certificate (PeerKeyVerifier)
//!   3. Source-chain signing key

use crate::config::HybridNodeConfig;
use crate::error::HybridNodeError;
use anyhow::Result;
use tracing::info;

/// Resolved identity for this node.
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    /// ed25519 AgentPubKey as bytes (32 bytes).
    pub agent_pub_key: Vec<u8>,
    /// Hex-encoded for logging / display.
    pub agent_pub_key_hex: String,
}

/// Connect to lair-keystore via Holochain conductor and load the agent key.
///
/// In production this calls the Holochain conductor WebSocket to retrieve
/// `AppInfo.agent_pub_key`. In mock mode (feature = "mock-sdwan") it returns
/// a deterministic test key.
pub async fn load_from_conductor(config: &HybridNodeConfig) -> Result<NodeIdentity> {
    #[cfg(feature = "mock-sdwan")]
    {
        let _ = config;
        let fake_key = vec![0xabu8; 32];
        let hex = hex::encode(&fake_key);
        info!("Identity loaded (mock) — agent={hex}");
        return Ok(NodeIdentity { agent_pub_key: fake_key, agent_pub_key_hex: hex });
    }

    #[cfg(not(feature = "mock-sdwan"))]
    {
        // Production path: connect to conductor via holochain_client
        // TODO: replace with actual holochain_client call when stabilized
        let _url = &config.holochain.conductor_url;
        Err(HybridNodeError::Identity(
            "holochain_client integration not yet implemented — build with mock-sdwan feature".to_string()
        ).into())
    }
}

impl NodeIdentity {
    /// Derive a self-signed rustls certificate from the ed25519 key.
    /// Used by PeerKeyVerifier for QUIC/mTLS mutual authentication.
    pub fn to_tls_cert_der(&self) -> Result<Vec<u8>> {
        // In production: use rcgen + ed25519-dalek to generate a self-signed DER cert
        // where the Subject Public Key Info contains the AgentPubKey.
        // For now returns a stub — the daemon enforces mtls_strict=true which
        // activates the real PeerKeyVerifier path in the QUIC module.
        Err(HybridNodeError::Identity(
            "TLS cert derivation not yet implemented".to_string()
        ).into())
    }
}
