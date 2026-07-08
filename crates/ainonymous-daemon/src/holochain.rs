use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use ainonymous_types::{ExecutionPlan, NodeHeartbeat, Warrant, ModelClaim};
use crate::config::{DaemonConfig, MembraneProofConfig};
use crate::conductor_client::ConductorClient;

// ... (NodeSummary et Backend inchangés)

#[derive(Clone)]
pub struct HolochainClient {
    // ... champs existants
    membrane_proof: Option<Vec<u8>>,
}

impl HolochainClient {
    // ... connect() et autres méthodes existantes ...

    /// Émet un Warrant (ModelClaim, NodeCapabilities, ExecutionProof...)
    pub async fn emit_warrant(&self, warrant: &Warrant) -> Result<()> {
        self.zome_call(
            "agent-registry",
            "coordinator",
            "emit_warrant",
            serde_json::to_value(warrant)?,
        ).await?;
        info!("Warrant émis: {:?} par {}", warrant.warrant_type, hex::encode(warrant.issuer));
        Ok(())
    }

    /// Vérifie un Warrant via le zome
    pub async fn verify_warrant(&self, warrant: &Warrant) -> Result<bool> {
        let result = self.zome_call(
            "agent-registry",
            "coordinator",
            "verify_warrant",
            serde_json::to_value(warrant)?,
        ).await?;

        Ok(result["valid"].as_bool().unwrap_or(false))
    }

    /// Récupère les warrants valides d'un nœud
    pub async fn get_warrants_for_agent(&self, agent_id: &str) -> Result<Vec<Warrant>> {
        let resp = self.zome_call(
            "agent-registry",
            "coordinator",
            "get_warrants",
            json!({ "agent_id": agent_id }),
        ).await?;

        Ok(serde_json::from_value(resp)?)
    }

    // ... reste des méthodes existantes (announce_capabilities, negotiate_quic_session, etc.)
}

/// Validation simple des warrants avant d'assigner du travail (scheduler)
pub async fn validate_node_warrants(
    holochain: &HolochainClient,
    agent_id: &str,
    required_model: Option<&str>,
) -> Result<bool> {
    let warrants = holochain.get_warrants_for_agent(agent_id).await?;

    let has_valid_model_claim = if let Some(model) = required_model {
        warrants.iter().any(|w| {
            if w.warrant_type == ainonymous_types::WarrantType::ModelClaim {
                if let Ok(claim) = serde_json::from_value::<ModelClaim>(w.payload.clone()) {
                    return claim.model_id == model && !w.is_expired();
                }
            }
            false
        })
    } else {
        true
    };

    Ok(has_valid_model_claim)
}
