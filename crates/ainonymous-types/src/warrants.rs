impl Warrant {
    /// Crée et signe un nouveau warrant avec Domain Separation
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

        // Domain Separation (meilleure pratique de sécurité)
        const DOMAIN: &[u8] = b"AInonymous-Warrant-v1";

        let mut to_sign = Vec::new();
        to_sign.extend_from_slice(DOMAIN);
        to_sign.extend_from_slice(&issuer);
        to_sign.extend_from_slice(&issued_at.to_le_bytes());
        to_sign.extend_from_slice(warrant_type.to_string().as_bytes());
        to_sign.extend_from_slice(&serde_json::to_vec(&payload)?);

        let signature = signing_key.sign(&to_sign).to_bytes().to_vec();

        Ok(Self {
            issuer,
            warrant_type,
            payload,
            signature,
            issued_at,
            ttl_seconds,
        })
    }

    /// Vérifie la signature avec Domain Separation
    pub fn verify(&self, issuer_pubkey: &VerifyingKey) -> bool {
        if self.issuer != issuer_pubkey.to_bytes() {
            return false;
        }
        if self.is_expired() {
            return false;
        }

        const DOMAIN: &[u8] = b"AInonymous-Warrant-v1";

        let mut to_verify = Vec::new();
        to_verify.extend_from_slice(DOMAIN);
        to_verify.extend_from_slice(&self.issuer);
        to_verify.extend_from_slice(&self.issued_at.to_le_bytes());
        to_verify.extend_from_slice(self.warrant_type.to_string().as_bytes());
        if let Ok(payload_bytes) = serde_json::to_vec(&self.payload) {
            to_verify.extend_from_slice(&payload_bytes);
        }

        if let Ok(sig_array) = <[u8; 64]>::try_from(self.signature.as_slice()) {
            let signature = Signature::from_bytes(&sig_array);
            return issuer_pubkey.verify_strict(&to_verify, &signature).is_ok();
        }
        false
    }
}
