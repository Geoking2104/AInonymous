// Dans run_pipeline_inference, remplacer la négociation par la version P2P
let offer = holochain.negotiate_quic_session_p2p(
    &first.node,
    Some((first.layer_start, first.layer_end)),
    next.map(|s| s.node.clone()),
    next.map(|s| (s.layer_start, s.layer_end)),
    Some(identity.public_key_bytes()),
).await.context("négociation session 1er étage (P2P)")?;

// Plus loin dans la boucle, pour les nœuds suivants :
let next_offer = holochain.negotiate_quic_session_p2p(
    next_agent,
    offer.next_layer_range,
    None,
    None,
    Some(identity.public_key_bytes()),
).await.context("négociation nœud suivant (P2P)")?;
