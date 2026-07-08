impl NodeIdentity {
    /// Retourne la clé publique sous forme de bytes [u8; 32]
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Retourne la clé publique au format hexadécimal
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    /// Vérifie une signature ed25519 faite avec cette clé publique
    pub fn verify_signature(&self, data: &[u8], signature: &[u8]) -> bool {
        if signature.len() != 64 {
            return false;
        }

        let sig_array: [u8; 64] = match signature.try_into() {
            Ok(arr) => arr,
            Err(_) => return false,
        };

        let signature = Signature::from_bytes(&sig_array);
        self.verifying_key.verify_strict(data, &signature).is_ok()
    }

    /// Crée et signe un Warrant de façon sécurisée
    pub fn sign_warrant(
        &self,
        warrant_type: WarrantType,
        payload: serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<Warrant> {
        Warrant::new_signed(&self.signing_key, warrant_type, payload, ttl_seconds)
    }
}
