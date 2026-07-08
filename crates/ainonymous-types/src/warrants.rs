use serde::{Deserialize, Serialize};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use rand::rngs::OsRng;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warrant {
    pub issuer: [u8; 32],
    pub warrant_type: WarrantType,
    pub payload: serde_json::Value,
    pub signature: Vec<u8>,
    pub issued_at: u64,
    #[serde(default)]
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarrantType {
    ModelClaim,
    NodeCapabilities,
    ExecutionProof,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelClaim {
    pub model_id: String,
    pub model_hash: String,
    pub vram_required_gb: f32,
    pub max_context: u32,
    pub supported_backends: Vec<String>,
}

impl Warrant {
    /// Crée et signe un nouveau warrant
    pub fn new_signed(
        signing_key: &SigningKey,
        warrant_type: WarrantType,
        payload: serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<Self, anyhow::Error> {
        let issuer = signing_key.verifying_key().to_bytes();
        let issued_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // On signe le payload + type + timestamp pour éviter les replays
        let mut to_sign = Vec::new();
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

    /// Vérifie la signature du warrant
    pub fn verify(&self, issuer_pubkey: &VerifyingKey) -> bool {
        if self.issuer != issuer_pubkey.to_bytes() {
            return false;
        }
        if self.is_expired() {
            return false;
        }

        let mut to_verify = Vec::new();
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

    pub fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.issued_at + self.ttl_seconds
    }
}
