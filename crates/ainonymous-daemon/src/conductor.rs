/// Construit un plan d'exécution dynamiquement via découverte P2P Holochain.
pub async fn build_dynamic_pipeline_plan(
    holochain: &HolochainClient,
    model_id: &str,
) -> Result<ExecutionPlan> {
    // Découverte P2P des nœuds disponibles
    let discovered = holochain.discover_nodes_p2p(model_id).await?;

    if discovered.is_empty() {
        anyhow::bail!("Aucun nœud découvert via Holochain P2P pour le modèle {}", model_id);
    }

    // Filtrage : on ne garde que les nœuds qui ont des warrants valides
    let mut valid_nodes = Vec::new();
    for node in discovered {
        if validate_node_warrants(holochain, &node.agent_id, Some(model_id)).await? {
            valid_nodes.push(node);
        }
    }

    if valid_nodes.is_empty() {
        anyhow::bail!("Aucun nœud avec warrants valides trouvé pour {}", model_id);
    }

    // Construction d'un plan simple (pour l'instant on prend les 2-3 premiers nœuds valides)
    let mut stages = Vec::new();
    let num_stages = valid_nodes.len().min(3); // max 3 étages pour l'instant

    for (i, node) in valid_nodes.iter().take(num_stages).enumerate() {
        if let Some(ep) = &node.quic_endpoint {
            if let Ok(addr) = ep.parse::<SocketAddr>() {
                stages.push(ainonymous_types::PipelineStage {
                    node: node.agent_id.clone(),
                    quic_endpoint: addr,
                    layer_start: (i * 6) as u32,      // découpage simple
                    layer_end: ((i + 1) * 6) as u32,
                    is_last: i == num_stages - 1,
                });
            }
        }
    }

    if stages.is_empty() {
        anyhow::bail!("Impossible de construire un plan pipeline à partir des nœuds découverts");
    }

    info!("Plan pipeline dynamique construit avec {} nœuds P2P", stages.len());
    Ok(ExecutionPlan::PipelineSplit { stages })
}
