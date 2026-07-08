    // 3. Émettre de nouveaux warrants de façon sûre (non-fatale)
    if let Err(e) = s.holochain.try_emit_warrant(&model_claim_warrant).await {
        warn!("Échec émission ModelClaim warrant: {}", e);
    }

    if let Err(e) = s.holochain.try_emit_warrant(&capabilities_warrant).await {
        warn!("Échec émission NodeCapabilities warrant: {}", e);
    }
