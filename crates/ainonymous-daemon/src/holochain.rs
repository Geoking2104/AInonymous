use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use serde_json::json;
use tracing::info;

use ainonymous_types::{Warrant, WarrantType, ModelClaim};
use crate::config::DaemonConfig;
use crate::conductor_client::ConductorClient;

// ... autres imports et code ...

impl HolochainClient {
    // ... méthodes existantes ...

    /// Émet un warrant en utilisant la méthode avec cleanup (remplace les anciens du même type)
    pub async fn emit_warrant_with_cleanup(&self, warrant: &Warrant) -> Result<()> {
        self.zome_call(
            "warrants",
            "coordinator",
            "emit_warrant_with_cleanup",
            serde_json::to_value(warrant)?,
        ).await?;

        info!("Warrant émis avec cleanup: {:?}", warrant.warrant_type);
        Ok(())
    }

    /// Émet un ModelClaim après rotation de clé ou au démarrage
    pub async fn emit_model_claim(
        &self,
        model_id: &str,
        model_hash: &str,
        vram_gb: f32,
        signing_key: &ainonymous_quic::NodeIdentity, // pour signer
    ) -> Result<()> {
        let claim = ModelClaim {
            model_id: model_id.to_string(),
            model_hash: model_hash.to_string(),
            vram_required_gb: vram_gb,
            max_context: 8192,
            supported_backends: vec!["cuda".to_string(), "metal".to_string()],
        };

        let warrant = Warrant::new_signed(
            &signing_key.signing_key, // on utilise la clé du nœud
            WarrantType::ModelClaim,
            serde_json::to_value(claim)?,
            86400 * 90, // 90 jours
        )?;

        self.emit_warrant_with_cleanup(&warrant).await?;
        Ok(())
    }
}
