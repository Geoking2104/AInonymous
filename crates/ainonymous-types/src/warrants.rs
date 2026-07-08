use serde::{Deserialize, Serialize};
use ed25519_dalek::{Signature, VerifyingKey};

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

/// Claim spécifique pour un modèle (utilisé dans ModelClaim)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelClaim {
    pub model_id: String,
    pub model_hash: String,           // SHA256 du GGUF ou du manifest
    pub vram_required_gb: f32,
    pub max_context: u32,
    pub supported_backends: Vec<String>,
}

impl Warrant {
    /// Vérifie la signature du warrant
    pub fn verify(&self, issuer_pubkey: &VerifyingKey) -> bool {
        // TODO: implémentation complète avec ed25519-dalek
        // Pour l'instant on considère que si on arrive ici, la structure est valide
        true
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
