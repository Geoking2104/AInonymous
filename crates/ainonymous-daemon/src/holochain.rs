/// Validation stricte des warrants d'un nœud (Palier F)
pub async fn validate_node_warrants(
    holochain: &HolochainClient,
    agent_id: &str,
    required_model: Option<&str>,
) -> Result<bool> {
    let warrants = match holochain.get_warrants_for_agent(agent_id).await {
        Ok(w) => w,
        Err(e) => {
            warn!("Impossible de récupérer les warrants de {}: {}", agent_id, e);
            return Ok(false);
        }
    };

    if warrants.is_empty() {
        warn!("Aucun warrant trouvé pour le nœud {}", agent_id);
        return Ok(false);
    }

    let mut has_valid_model_claim = false;
    let mut has_valid_capabilities = false;

    for warrant in &warrants {
        // Vérifier expiration
        if warrant.is_expired() {
            continue;
        }

        // Vérifier signature (si on a la pubkey de l'émetteur)
        // Note: on suppose que l'issuer est l'agent lui-même
        let pubkey = match VerifyingKey::from_bytes(&warrant.issuer) {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        if !warrant.verify(&pubkey) {
            warn!("Warrant invalide (signature) pour {}", agent_id);
            continue;
        }

        match warrant.warrant_type {
            WarrantType::ModelClaim => {
                if let Ok(claim) = serde_json::from_value::<ModelClaim>(warrant.payload.clone()) {
                    if let Some(required) = required_model {
                        if claim.model_id == required {
                            has_valid_model_claim = true;
                        }
                    } else {
                        has_valid_model_claim = true;
                    }
                }
            }
            WarrantType::NodeCapabilities => {
                has_valid_capabilities = true;
            }
            _ => {}
        }
    }

    let is_valid = has_valid_model_claim && has_valid_capabilities;

    if !is_valid {
        warn!(
            "Validation warrants échouée pour {} | ModelClaim: {} | Capabilities: {}",
            agent_id, has_valid_model_claim, has_valid_capabilities
        );
    }

    Ok(is_valid)
}
