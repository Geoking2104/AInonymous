//! SD-WAN integration layer.
//!
//! Abstracts over multiple SD-WAN providers (Cisco vManage, VMware VeloCloud,
//! generic REST) via the `SdwanProvider` trait. The `mock` provider is used
//! in development and CI.

use crate::config::SdwanConfig;
use crate::error::HybridNodeError;
use crate::topology::{LinkSla, SiteId};
use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

/// Abstract interface for SD-WAN controllers.
#[async_trait]
pub trait SdwanProvider: Send + Sync {
    /// Return the site ID of this node (from env var or controller).
    async fn local_site_id(&self) -> Result<SiteId>;

    /// Return all sites known to the controller.
    async fn list_sites(&self) -> Result<Vec<SiteId>>;

    /// Return SLA metrics for the link from `from` to `to`.
    async fn get_link_sla(&self, from: &str, to: &str) -> Result<LinkSla>;

    /// Apply a traffic policy (DSCP marking, QoS) for inference traffic.
    async fn set_traffic_policy(&self, traffic_class: &str, dscp: u8) -> Result<()>;

    /// Check if a peer at `addr` is reachable from this site.
    async fn is_peer_reachable(&self, addr: &str) -> Result<bool>;
}

/// Connect to the SD-WAN provider specified in config.
pub async fn connect(config: &SdwanConfig) -> Result<Box<dyn SdwanProvider>> {
    match config.provider.as_str() {
        "mock" => {
            info!("SD-WAN: using mock provider");
            Ok(Box::new(MockSdwan::new()))
        }
        "rest" | "vmanage" | "velocloud" => {
            #[cfg(feature = "vmanage")]
            {
                info!("SD-WAN: connecting to {}", config.provider);
                // TODO: implement REST client using reqwest
                Err(HybridNodeError::Sdwan(format!(
                    "Provider '{}' not yet implemented — build with 'mock-sdwan' for dev",
                    config.provider
                )).into())
            }
            #[cfg(not(feature = "vmanage"))]
            {
                Err(HybridNodeError::Sdwan(format!(
                    "Provider '{}' requires feature 'vmanage'. Enable it in Cargo.toml or use 'mock'.",
                    config.provider
                )).into())
            }
        }
        other => Err(HybridNodeError::Sdwan(format!("Unknown SD-WAN provider: {other}")).into()),
    }
}

// ---------------------------------------------------------------------------
// Mock implementation for development / CI
// ---------------------------------------------------------------------------

pub struct MockSdwan {
    local_site: SiteId,
}

impl MockSdwan {
    pub fn new() -> Self {
        Self { local_site: "site-local".to_string() }
    }
}

impl Default for MockSdwan {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl SdwanProvider for MockSdwan {
    async fn local_site_id(&self) -> Result<SiteId> {
        Ok(self.local_site.clone())
    }

    async fn list_sites(&self) -> Result<Vec<SiteId>> {
        Ok(vec!["site-local".to_string(), "site-remote-a".to_string()])
    }

    async fn get_link_sla(&self, from: &str, to: &str) -> Result<LinkSla> {
        let (latency_ms, bandwidth_mbps) = if from == to {
            (1.0, 25_000.0)
        } else {
            (15.0, 2_000.0)
        };
        Ok(LinkSla {
            from_site: from.to_string(),
            to_site: to.to_string(),
            latency_ms,
            bandwidth_mbps,
            packet_loss: 0.0,
            is_private_wan: true,
        })
    }

    async fn set_traffic_policy(&self, traffic_class: &str, dscp: u8) -> Result<()> {
        info!("MockSdwan: set_traffic_policy class={traffic_class} dscp={dscp}");
        Ok(())
    }

    async fn is_peer_reachable(&self, addr: &str) -> Result<bool> {
        info!("MockSdwan: is_peer_reachable addr={addr} → true (mock)");
        Ok(true)
    }
}
