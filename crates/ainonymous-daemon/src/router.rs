/// POST /daemon/rotate-identity
///
/// Effectue une rotation complète de l'identité ed25519 du nœud :
/// 1. Génère une nouvelle clé (keyring + fichier)
/// 2. Ré-annonce la nouvelle pubkey dans le DHT
/// 3. Émet de nouveaux warrants (ModelClaim + NodeCapabilities) avec cleanup
async fn rotate_identity(State(s): State<DaemonState>) -> impl IntoResponse {
    info!("Début de la rotation d'identité...");

    // 1. Rotation de la clé
    let (new_identity, old_pubkey_bytes) = match NodeIdentity::rotate_file(&s.identity_path) {
        Ok(pair) => pair,
        Err(e) => {
            error!("Échec de la rotation de clé: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("rotation échouée: {}", e)
                })),
            ).into_response();
        }
    };

    let old_pubkey_hex = hex::encode(old_pubkey_bytes);
    let new_pubkey_hex = new_identity.public_key_hex();

    info!("Nouvelle identité générée: {} (ancienne: {})", new_pubkey_hex, old_pubkey_hex);

    // 2. Ré-annoncer la nouvelle pubkey dans le DHT
    let dht_updated = match s.holochain.reannounce_pubkey(&new_pubkey_hex, &s.config).await {
        Ok(_) => {
            info!("Pubkey ré-annoncée dans le DHT");
            true
        }
        Err(e) => {
            warn!("Échec de la ré-annonce DHT: {}", e);
            false
        }
    };

    // 3. Émettre de nouveaux warrants avec cleanup
    let mut warrants_emitted = false;

    // Émettre ModelClaim
    if let Err(e) = s.holochain.emit_model_claim(
        "gemma4-e4b",           // TODO: rendre configurable
        "sha256-pending",       // TODO: calculer le vrai hash
        &new_identity,
    ).await {
        warn!("Échec émission ModelClaim après rotation: {}", e);
    } else {
        warrants_emitted = true;
    }

    // Émettre NodeCapabilities
    if let Err(e) = s.holochain.emit_node_capabilities(&new_identity).await {
        warn!("Échec émission NodeCapabilities après rotation: {}", e);
    } else {
        warrants_emitted = true;
    }

    if warrants_emitted {
        info!("Nouveaux warrants émis avec succès après rotation");
    }

    Json(serde_json::json!({
        "status": "success",
        "old_pubkey": old_pubkey_hex,
        "new_pubkey": new_pubkey_hex,
        "restart_required": true,
        "dht_updated": dht_updated,
        "warrants_emitted": warrants_emitted,
    })).into_response()
}
