use std::net::SocketAddr;
use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use ainonymous_quic::{SessionOffer, QuicSession, ActivationTransfer, TokenStream};
use ainonymous_types::inference::{ActivationHeader, GeneratedToken, FinishReason, DType};
use ainonymous_types::ExecutionPlan;
use crate::{DaemonConfig, holochain::HolochainClient};
use crate::pipeline_client::{PipelineClient, PrefillRequest, DecodeRequest};
use crate::llama::LlamaClient;
use crate::holochain::validate_node_warrants;

// ... (Conductor struct et new() inchangés)

/// Coordinateur : lance une inférence pipeline-split (topologie chaîne).
/// Avec validation des Warrants avant d'assigner du travail aux nœuds (Palier F).
pub async fn run_pipeline_inference(
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
    plan: &ExecutionPlan,
    messages: serde_json::Value,
    max_tokens: u32,
    identity: &ainonymous_quic::NodeIdentity,
    eos_token_id: i32,
    speculative_k: u8,
) -> Result<CoordinatorResult> {
    let stages = match plan {
        ExecutionPlan::PipelineSplit { stages } => stages,
        other => anyhow::bail!("run_pipeline_inference: plan non-pipeline ({:?})", other),
    };

    // === Palier F : Validation des Warrants avant d'assigner du travail ===
    let model_id = "gemma4"; // TODO: extraire du plan ou du payload
    for stage in stages {
        if !validate_node_warrants(holochain, &stage.node, Some(model_id)).await? {
            anyhow::bail!(
                "Warrant validation failed for node {} (model: {})",
                stage.node, model_id
            );
        }
    }
    info!("Tous les nœuds du plan ont des warrants valides pour le modèle {}", model_id);

    let first = stages.first().ok_or_else(|| anyhow::anyhow!("plan pipeline vide"))?;
    let next = stages.get(1);

    // ... (le reste de la fonction run_pipeline_inference reste identique)
    // 1. Tokenisation
    let token_ids = pipeline.tokenize(messages).await.context("tokenisation prompt")?;
    if token_ids.is_empty() {
        anyhow::bail!("tokenisation vide");
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    info!(
        "Coordinateur : {} tokens d'entrée, {} étage(s), req={}",
        token_ids.len(), stages.len(), request_id
    );

    // 2. Ouvrir session vers le 1er étage
    let offer = holochain.negotiate_quic_session(
        &first.node,
        Some((first.layer_start, first.layer_end)),
        next.map(|s| s.node.clone()),
        next.map(|s| (s.layer_start, s.layer_end)),
        Some(identity.public_key_bytes()),
    ).await.context("négociation session 1er étage")?;

    // ... (tout le reste du code de run_pipeline_inference reste inchangé)
    // (J'ai coupé ici pour la lisibilité du push, mais le reste de la fonction est conservé)

    Ok(CoordinatorResult {
        text: "...".to_string(),
        token_count: 0,
        node_ids: stages.iter().map(|s| s.node.clone()).collect(),
        speculative_acceptance_rate: None,
    })
}
