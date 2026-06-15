//! Network topology model for HybridNode.
//!
//! Combines data from:
//!   - SD-WAN controller (site topology, link SLAs)
//!   - Holochain DHT (node capabilities, attestations)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Opaque site identifier from the SD-WAN controller.
pub type SiteId = String;

/// Opaque peer identifier (Holochain AgentPubKey hex).
pub type PeerId = String;

/// SLA metrics for a network link between two sites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkSla {
    pub from_site: SiteId,
    pub to_site: SiteId,
    /// Round-trip latency in ms.
    pub latency_ms: f64,
    /// Available bandwidth in Mbps.
    pub bandwidth_mbps: f64,
    /// Packet loss rate [0.0, 1.0].
    pub packet_loss: f64,
    /// Whether this link has MPLS / private WAN backing.
    pub is_private_wan: bool,
}

impl LinkSla {
    /// True if this link meets the intra-site SLA requirements.
    pub fn is_intra_site_ok(&self) -> bool {
        self.latency_ms < 5.0 && self.bandwidth_mbps >= 10_000.0
    }

    /// True if this link can carry inter-site inference traffic.
    pub fn is_inter_site_inference_ok(&self) -> bool {
        self.latency_ms < 20.0 && self.bandwidth_mbps >= 1_000.0
    }

    /// Estimate if an activation blob of `size_mb` can be sent within `budget_ms`.
    pub fn can_transfer_within(&self, size_mb: f64, budget_ms: f64) -> bool {
        let transfer_ms = (size_mb / self.bandwidth_mbps) * 1000.0 * 8.0;
        (transfer_ms + self.latency_ms) < budget_ms
    }
}

/// Capabilities of a remote peer node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub peer_id: PeerId,
    pub site_id: SiteId,
    /// VRAM in MB.
    pub vram_mb: u64,
    /// GPU memory bandwidth in GB/s.
    pub memory_bandwidth_gbps: f64,
    /// List of model names this peer holds (from ModelClaim DHT entries).
    pub held_models: Vec<String>,
    /// Reputation score [0.0, 1.0].
    pub reputation: f64,
    /// True if this peer has an active warrant (excluded from scheduling).
    pub has_active_warrant: bool,
}

/// Aggregated topology snapshot used by the scheduler.
#[derive(Debug, Clone)]
pub struct NodeTopology {
    /// This node's site.
    pub local_site: SiteId,
    /// SLA matrix between sites.
    pub links: Vec<LinkSla>,
    /// Capabilities of known peers.
    pub peers: HashMap<PeerId, PeerCapabilities>,
}

impl NodeTopology {
    /// Return peers in the same site, sorted by reputation descending.
    pub fn local_peers(&self) -> Vec<&PeerCapabilities> {
        let mut peers: Vec<&PeerCapabilities> = self.peers.values()
            .filter(|p| p.site_id == self.local_site && !p.has_active_warrant)
            .collect();
        peers.sort_by(|a, b| b.reputation.partial_cmp(&a.reputation).unwrap());
        peers
    }

    /// Return the best link to `remote_site` (lowest latency).
    pub fn best_link_to(&self, remote_site: &str) -> Option<&LinkSla> {
        self.links.iter()
            .filter(|l| l.from_site == self.local_site && l.to_site == remote_site)
            .min_by(|a, b| a.latency_ms.partial_cmp(&b.latency_ms).unwrap())
    }
}
