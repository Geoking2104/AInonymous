// Ajout du paramètre model_id pour rendre la validation dynamique
pub async fn run_pipeline_inference(
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
    plan: &ExecutionPlan,
    messages: serde_json::Value,
    max_tokens: u32,
    identity: &ainonymous_quic::NodeIdentity,
    eos_token_id: i32,
    speculative_k: u8,
    model_id: &str,                    // ← NOUVEAU : passé dynamiquement
) -> Result<CoordinatorResult> {
    let stages = match plan {
        ExecutionPlan::PipelineSplit { stages } => stages,
        other => anyhow::bail!("run_pipeline_inference: plan non-pipeline ({:?})", other),
    };

    // === Palier F : Validation dynamique des Warrants ===
    for stage in stages {
        if !validate_node_warrants(holochain, &stage.node, Some(model_id)).await? {
            anyhow::bail!(
                "Warrant validation failed for node {} (required model: {})",
                stage.node, model_id
            );
        }
    }
    info!("Warrant check passed for all nodes on model '{}'. Starting pipeline.", model_id);

    // ... reste de la fonction inchangé
}