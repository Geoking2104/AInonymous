//! Model validation and placement logic.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use anyhow::Result;
use crate::error::HybridNodeError;

/// Mirrors the ModelManifest Holochain entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub model_name: String,
    pub version: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub architecture: String,
    pub quant_format: String,
    pub num_layers: u32,
}

/// Local verification result.
#[derive(Debug, Clone)]
pub enum VerificationResult {
    Ok,
    HashMismatch { expected: String, actual: String },
    FileMissing,
}

/// Compute SHA-256 of a GGUF file and compare against the manifest.
pub async fn verify_local_model(manifest: &ModelManifest, path: &Path) -> Result<VerificationResult> {
    if !path.exists() {
        return Ok(VerificationResult::FileMissing);
    }

    let bytes = tokio::fs::read(path).await?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = hex::encode(hasher.finalize());

    if actual == manifest.sha256 {
        Ok(VerificationResult::Ok)
    } else {
        Ok(VerificationResult::HashMismatch {
            expected: manifest.sha256.clone(),
            actual,
        })
    }
}

/// Placement decision for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementPlan {
    pub model_name: String,
    /// Nodes that should hold this model (AgentPubKey hex).
    pub assigned_nodes: Vec<String>,
    /// Total activation transfer size for pipeline inference (MB).
    pub estimated_activation_mb: f64,
}

/// Decide whether a model should be placed on a remote node.
///
/// Formula: activation_size_mb / bandwidth_mbps < latency_budget_ms / 1000
pub fn should_transfer_to_remote(
    activation_size_mb: f64,
    bandwidth_mbps: f64,
    latency_budget_ms: f64,
) -> bool {
    if bandwidth_mbps == 0.0 { return false; }
    let transfer_time_s = (activation_size_mb * 8.0) / bandwidth_mbps / 1000.0;
    transfer_time_s < latency_budget_ms / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_transfer_intra_site() {
        // 50 MB activation, 10 Gbps link, 20ms budget
        // transfer = (50 * 8) / 10_000 / 1000 = 0.00004s = 0.04ms << 20ms
        assert!(should_transfer_to_remote(50.0, 10_000.0, 20.0));
    }

    #[test]
    fn test_should_not_transfer_slow_link() {
        // 500 MB activation, 10 Mbps link, 20ms budget → would take 400s
        assert!(!should_transfer_to_remote(500.0, 10.0, 20.0));
    }
}
