//! Locality-aware inference scheduler.
//!
//! Combines SD-WAN SLA data with Holochain node capabilities to decide
//! where to route inference work (layer ranges, activation transfers).

use crate::topology::{NodeTopology, PeerCapabilities, PeerId};
use serde::{Deserialize, Serialize};

/// Strategy for selecting inference peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingStrategy {
    /// Prefer peers in the same SD-WAN site.
    LocalFirst,
    /// Strict round-robin across all available peers.
    RoundRobin,
    /// Always pick the peer with lowest measured latency.
    LowestLatency,
    /// Maximize bandwidth for large activation transfers.
    HighestBandwidth,
}

impl Default for SchedulingStrategy {
    fn default() -> Self { Self::LocalFirst }
}

/// Input context for a scheduling decision.
#[derive(Debug, Clone)]
pub struct SchedulingContext {
    pub model_name: String,
    /// Size of the activation blob to be transferred to the chosen peer (MB).
    pub activation_size_mb: f64,
    /// Maximum acceptable end-to-end latency budget (ms).
    pub latency_budget_ms: f64,
    pub strategy: SchedulingStrategy,
    pub topology: NodeTopology,
}

/// Result of a scheduling decision.
#[derive(Debug, Clone)]
pub struct SchedulingDecision {
    pub peer_id: PeerId,
    /// Estimated latency to that peer (ms).
    pub estimated_latency_ms: f64,
    /// Whether the peer is on the same site (no WAN hop).
    pub is_local: bool,
    pub reason: String,
}

/// Main scheduling entry point.
///
/// Returns `None` if no eligible peer could be found within the latency budget.
pub fn schedule(ctx: &SchedulingContext) -> Option<SchedulingDecision> {
    match ctx.strategy {
        SchedulingStrategy::LocalFirst => schedule_local_first(ctx),
        SchedulingStrategy::LowestLatency => schedule_lowest_latency(ctx),
        SchedulingStrategy::HighestBandwidth => schedule_highest_bandwidth(ctx),
        SchedulingStrategy::RoundRobin => schedule_round_robin(ctx),
    }
}

/// Prefer same-site peers; fall back to remote only if no local peer can serve.
fn schedule_local_first(ctx: &SchedulingContext) -> Option<SchedulingDecision> {
    let local_peers = ctx.topology.local_peers();
    let capable: Vec<&&PeerCapabilities> = local_peers.iter()
        .filter(|p| p.held_models.contains(&ctx.model_name))
        .collect();

    if let Some(peer) = capable.first() {
        return Some(SchedulingDecision {
            peer_id: peer.peer_id.clone(),
            estimated_latency_ms: 1.0, // intra-site ≈ 1ms
            is_local: true,
            reason: "local peer holds model".to_string(),
        });
    }

    // Fall back: find a remote peer that holds the model and satisfies the budget
    for peer in ctx.topology.peers.values() {
        if peer.has_active_warrant { continue; }
        if !peer.held_models.contains(&ctx.model_name) { continue; }
        if peer.site_id == ctx.topology.local_site { continue; }

        if let Some(link) = ctx.topology.best_link_to(&peer.site_id) {
            if link.can_transfer_within(ctx.activation_size_mb, ctx.latency_budget_ms) {
                return Some(SchedulingDecision {
                    peer_id: peer.peer_id.clone(),
                    estimated_latency_ms: link.latency_ms,
                    is_local: false,
                    reason: format!("remote peer at {} — latency={:.1}ms", peer.site_id, link.latency_ms),
                });
            }
        }
    }

    None
}

fn schedule_lowest_latency(ctx: &SchedulingContext) -> Option<SchedulingDecision> {
    ctx.topology.peers.values()
        .filter(|p| !p.has_active_warrant && p.held_models.contains(&ctx.model_name))
        .filter_map(|peer| {
            let latency = if peer.site_id == ctx.topology.local_site {
                1.0
            } else {
                ctx.topology.best_link_to(&peer.site_id)?.latency_ms
            };
            if latency < ctx.latency_budget_ms {
                Some((peer, latency))
            } else {
                None
            }
        })
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(peer, latency)| SchedulingDecision {
            peer_id: peer.peer_id.clone(),
            estimated_latency_ms: latency,
            is_local: peer.site_id == ctx.topology.local_site,
            reason: format!("lowest latency {latency:.1}ms"),
        })
}

fn schedule_highest_bandwidth(ctx: &SchedulingContext) -> Option<SchedulingDecision> {
    ctx.topology.peers.values()
        .filter(|p| !p.has_active_warrant && p.held_models.contains(&ctx.model_name))
        .filter_map(|peer| {
            let (latency, bw) = if peer.site_id == ctx.topology.local_site {
                (1.0, 25_000.0_f64)
            } else {
                let link = ctx.topology.best_link_to(&peer.site_id)?;
                (link.latency_ms, link.bandwidth_mbps)
            };
            if latency < ctx.latency_budget_ms { Some((peer, latency, bw)) } else { None }
        })
        .max_by(|(_, _, a), (_, _, b)| a.partial_cmp(b).unwrap())
        .map(|(peer, latency, bw)| SchedulingDecision {
            peer_id: peer.peer_id.clone(),
            estimated_latency_ms: latency,
            is_local: peer.site_id == ctx.topology.local_site,
            reason: format!("highest bandwidth {bw:.0}Mbps"),
        })
}

fn schedule_round_robin(ctx: &SchedulingContext) -> Option<SchedulingDecision> {
    // Deterministic round-robin: pick the first eligible peer alphabetically.
    // A real implementation would persist a cursor in shared state.
    let mut eligible: Vec<&PeerCapabilities> = ctx.topology.peers.values()
        .filter(|p| !p.has_active_warrant && p.held_models.contains(&ctx.model_name))
        .collect();
    eligible.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    eligible.first().map(|peer| {
        let latency = if peer.site_id == ctx.topology.local_site { 1.0 }
            else { ctx.topology.best_link_to(&peer.site_id).map(|l| l.latency_ms).unwrap_or(50.0) };
        SchedulingDecision {
            peer_id: peer.peer_id.clone(),
            estimated_latency_ms: latency,
            is_local: peer.site_id == ctx.topology.local_site,
            reason: "round-robin selection".to_string(),
        }
    })
}
