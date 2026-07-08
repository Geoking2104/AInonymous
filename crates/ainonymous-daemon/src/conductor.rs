pub async fn build_dynamic_pipeline_plan(
    holochain: &HolochainClient,
    model_id: &str,
) -> Result<ExecutionPlan> {
    // Utilise la version optimisée (cache + scoring)
    let discovered = holochain
        .discover_nodes_p2p_optimized(model_id, Some(8.0), None)
        .await?;

    // ... reste de la fonction (filtrage warrants + construction du plan)
}
