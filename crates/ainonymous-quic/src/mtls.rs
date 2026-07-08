impl NodeIdentity {
    /// Crée et signe un Warrant de façon sécurisée (la clé privée ne sort jamais de la structure)
    pub fn sign_warrant(
        &self,
        warrant_type: WarrantType,
        payload: serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<Warrant> {
        Warrant::new_signed(&self.signing_key, warrant_type, payload, ttl_seconds)
    }

    /// Retourne uniquement la clé publique (jamais la clé privée)
    pub fn verifying_key(&self) -> VerifyingKey {
        self.verifying_key
    }
}
