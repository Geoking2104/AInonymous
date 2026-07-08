    // Reconstruire exactement le message signé (avec Domain Separation)
    const DOMAIN: &[u8] = b"AInonymous-Warrant-v1";

    let mut message = Vec::new();
    message.extend_from_slice(DOMAIN);
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
