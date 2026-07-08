/// POST /daemon/rotate-identity
async fn rotate_identity(State(s): State<DaemonState>) -> impl IntoResponse {
    let (new_identity, old_pubkey_bytes) = match NodeIdentity::rotate_file(&s.identity_path) {
        Ok(pair) => pair,
        Err(e) => return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("rotation échouée: {e}")})),
        ).into_response(),
    };

    let old_pubkey_hex = hex::encode(old_pubkey_bytes);
    let new_pubkey_hex = new_identity.public_key_hex();

    // Re-annoncer dans le DHT
    let dht_updated = s.holochain
        .reannounce_pubkey(&new_pubkey_hex, &s.config)
        .await
        .is_ok();

    // === Palier F : Émettre un nouveau warrant après rotation ===
    // On émet un ModelClaim basique (à améliorer avec les vraies capacités)
    if let Err(e) = s.holochain.emit_model_claim(
        "gemma4-e4b",
        "sha256-placeholder", // TODO: calculer le vrai hash du modèle
        24.0, // VRAM exemple
        &new_identity,
    ).await {
        warn!("Impossible d'émettre le warrant après rotation: {}", e);
    } else {
        info!("Nouveau warrant ModelClaim émis après rotation de clé");
    }

    Json(serde_json::json!({
        "old_pubkey": old_pubkey_hex,
        "new_pubkey": new_pubkey_hex,
        "restart_required": true,
        "dht_updated": dht_updated,
        "warrant_emitted": true,
    })).into_response()
}
