//! Identity management — wraps Holochain lair-keystore.
//!
//! Holochain keeps its AgentPubKey inside lair. The QUIC implementation uses a
//! separate, rotatable Ed25519 transport key and publishes that public key in a
//! Holochain-authored capability record. Reusing or extracting the lair key is
//! neither implemented nor required.

use crate::config::HybridNodeConfig;
use anyhow::{anyhow, Context, Result};
use holochain_client::{AdminWebsocket, AppWebsocket, CellInfo, ClientAgentSigner};
use tracing::info;

/// Resolved identity for this node.
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    /// Public identity returned by the conductor (not an extractable secret).
    pub agent_pub_key: Vec<u8>,
    /// Hex-encoded for logging / display.
    pub agent_pub_key_hex: String,
}

/// Connect to lair-keystore via Holochain conductor and load the agent key.
///
/// This always calls the Holochain conductor. Mocking the SD-WAN topology must
/// never silently replace the cryptographic control-plane identity.
pub async fn load_from_conductor(config: &HybridNodeConfig) -> Result<NodeIdentity> {
    let admin = AdminWebsocket::connect(("127.0.0.1", config.holochain.admin_port), None)
        .await
        .context("failed to connect to the Holochain 0.7 admin interface")?;
    let token = admin
        .issue_app_auth_token(config.holochain.app_id.clone().into())
        .await
        .context("failed to issue a Holochain app authentication token")?;
    let signer = ClientAgentSigner::default();
    let app = AppWebsocket::connect(
        ("127.0.0.1", config.holochain.app_port),
        token.token,
        signer.into(),
        None,
    )
    .await
    .context("failed to connect to the Holochain 0.7 app interface")?;

    let agent = app
        .cached_app_info()
        .cell_info
        .values()
        .flatten()
        .find_map(|cell| match cell {
            CellInfo::Provisioned(provisioned) => Some(provisioned.cell_id.agent_pubkey().clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("the configured Holochain app has no provisioned cell"))?;

    let raw = agent.get_raw_39().to_vec();
    let encoded = agent.to_string();
    info!("Holochain identity loaded — agent={encoded}");
    Ok(NodeIdentity {
        agent_pub_key: raw,
        agent_pub_key_hex: encoded,
    })
}
