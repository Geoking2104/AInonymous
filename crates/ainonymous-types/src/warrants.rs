use anyhow::Result;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Attestation signée par un nœud sur ses propres capacités ou sur un modèle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warrant {
    /// Pubkey ed25519 de l'émetteur du warrant
    pub issuer: [u8; 32],
    /// Type de warrant
    pub warrant_type: WarrantType,
    /// Contenu spécifique (ModelClaim, NodeCapabilities, etc.)
    pub payload: serde_json::Value,
    /// Signature ed25519 du payload + issuer
    pub signature: Vec<u8>,
    /// Timestamp de création (unix seconds)
    pub issued_at: u64,
    /// Durée de validité en secondes (0 = illimité)
    #[serde(default)]
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarrantType {
    /// Le nœud atteste qu'il héberge un modèle donné avec certaines caractéristiques
    ModelClaim,
    /// Attestation des capacités du nœud (VRAM, GPU, région, etc.)
    NodeCapabilities,
    /// Attestation croisée : un nœud certifie qu'un autre a bien exécuté des couches
    ExecutionProof,
    /// Custom / extensible
    Custom(String),
}

impl fmt::Display for WarrantType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WarrantType::ModelClaim => write!(f, "model_claim"),
            WarrantType::NodeCapabilities => write!(f, "node_capabilities"),
            WarrantType::ExecutionProof => write!(f, "execution_proof"),
            WarrantType::Custom(s) => write!(f, "custom:{s}"),
        }
    }
}

/// Claim spécifique pour un modèle (utilisé dans ModelClaim)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelClaim {
    pub model_id: String,
    pub model_hash: String, // SHA256 du GGUF ou du manifest
    pub vram_required_gb: f32,
    pub max_context: u32,
    pub supported_backends: Vec<String>,
}

impl Warrant {
    /// Vrai si le warrant a dépassé sa durée de validité (ttl_seconds = 0 → jamais)
    pub fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now > self.issued_at + self.ttl_seconds
    }

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
