use ed25519_dalek::{Signature, VerifyingKey};

#[hdk_extern]
pub fn verify_warrant(warrant: Warrant) -> ExternResult<bool> {
    // Vérification basique
    if warrant.signature.len() != 64 {
        return Ok(false);
    }

    if warrant.is_expired() {
        return Ok(false);
    }

    // Vérification cryptographique Ed25519
    let pubkey = match VerifyingKey::from_bytes(&warrant.issuer) {
        Ok(pk) => pk,
        Err(_) => return Ok(false),
    };

    let sig_array: [u8; 64] = match warrant.signature.try_into() {
        Ok(arr) => arr,
        Err(_) => return Ok(false),
    };

    let signature = Signature::from_bytes(&sig_array);

    // Reconstruire les données signées (doit matcher exactement new_signed)
    let mut message = Vec::new();
    message.extend_from_slice(&warrant.issuer);
    message.extend_from_slice(&warrant.issued_at.to_le_bytes());
    message.extend_from_slice(warrant.warrant_type.to_string().as_bytes());

    if let Ok(payload_bytes) = serde_json::to_vec(&warrant.payload) {
        message.extend_from_slice(&payload_bytes);
    }

    match pubkey.verify_strict(&message, &signature) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
