use ed25519_dalek::{Signature, SigningKey, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};

impl Warrant {
    /// Crée et signe un warrant en utilisant Ed25519ctx (RFC 8032)
    pub fn new_signed(
        signing_key: &SigningKey,
        warrant_type: WarrantType,
        payload: serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<Self> {
        let issuer = signing_key.verifying_key().to_bytes();
        let issued_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // Contexte Ed25519ctx (RFC 8032)
        let context = b"AInonymous-Warrant-v1";

        // Construction des données à signer
        let mut message = Vec::new();
        message.extend_from_slice(&issuer);
        message.extend_from_slice(&issued_at.to_le_bytes());
        message.extend_from_slice(warrant_type.to_string().as_bytes());
        message.extend_from_slice(&serde_json::to_vec(&payload)?);

        // Signature avec contexte Ed25519ctx
        let signature = signing_key
            .sign_prehashed(
                &message,
                Some(context),
            )
            .map_err(|e| anyhow::anyhow!("Ed25519ctx signing failed: {}", e))?
            .to_bytes()
            .to_vec();

        Ok(Self {
            issuer,
            warrant_type,
            payload,
            signature,
            issued_at,
            ttl_seconds,
        })
    }

    /// Vérifie la signature en utilisant Ed25519ctx
    pub fn verify(&self, issuer_pubkey: &VerifyingKey) -> bool {
        if self.issuer != issuer_pubkey.to_bytes() {
            return false;
        }
        if self.is_expired() {
            return false;
        }

        let context = b"AInonymous-Warrant-v1";

        let mut message = Vec::new();
        message.extend_from_slice(&self.issuer);
        message.extend_from_slice(&self.issued_at.to_le_bytes());
        message.extend_from_slice(self.warrant_type.to_string().as_bytes());
        if let Ok(payload_bytes) = serde_json::to_vec(&self.payload) {
            message.extend_from_slice(&payload_bytes);
        }

        if let Ok(sig_array) = <[u8; 64]>::try_from(self.signature.as_slice()) {
            let signature = Signature::from_bytes(&sig_array);

            return issuer_pubkey
                .verify_prehashed(&message, Some(context), &signature)
                .is_ok();
        }
        false
    }
}
