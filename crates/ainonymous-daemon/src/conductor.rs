pub async fn handle_pipeline_session(
    conn: quinn::Connection,
    offer: SessionOffer,
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
    identity: &ainonymous_quic::NodeIdentity,
) -> Result<()> {
    let layer_range = offer.layer_range.unwrap_or((0, 0));

    // === Palier F : Validation côté worker ===
    // On vérifie que ce nœud a bien un warrant valide pour les couches demandées.
    // Note: on utilise un model_id générique pour l'instant.
    if !validate_node_warrants(holochain, "local", None).await? {
        warn!("Worker refused pipeline session: missing valid warrants");
        anyhow::bail!("This node does not have valid warrants to participate in the mesh");
    }

    info!("Session pipeline entrante validée par warrants — couches [{}, {}[", layer_range.0, layer_range.1);

    // ... reste de la fonction handle_pipeline_session inchangé
}