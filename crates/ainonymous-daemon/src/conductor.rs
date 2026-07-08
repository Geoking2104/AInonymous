pub async fn run_pipeline_inference(
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
    plan: &ExecutionPlan,
    messages: serde_json::Value,
    max_tokens: u32,
    identity: &ainonymous_quic::NodeIdentity,
    eos_token_id: i32,
    speculative_k: u8,
    model_id: &str,
) -> Result<CoordinatorResult> {
    let mut effective_plan = plan.clone();

    // Si le plan est vide ou Solo, on tente une découverte P2P dynamique
    if matches!(plan, ExecutionPlan::Solo { .. }) || matches!(plan, ExecutionPlan::PipelineSplit { stages } if stages.is_empty()) {
        if let Ok(dynamic_plan) = build_dynamic_pipeline_plan(holochain, model_id).await {
            info!("Utilisation d'un plan pipeline découvert dynamiquement via Holochain P2P");
            effective_plan = dynamic_plan;
        }
    }

    let stages = match &effective_plan {
        ExecutionPlan::PipelineSplit { stages } => stages,
        other => anyhow::bail!("run_pipeline_inference: plan non supporté ({:?})", other),
    };

    // Validation des warrants (comme avant)
    for stage in stages {
        if !validate_node_warrants(holochain, &stage.node, Some(model_id)).await? {
            anyhow::bail!("Warrant validation failed for node {}", stage.node);
        }
    }

    // ... reste de la fonction (négociation P2P, etc.)
}
