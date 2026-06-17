//! # hybridnode-core
//!
//! Reusable architecture layer combining:
//! - SD-WAN (underlay topology and QoS)
//! - Holochain (overlay: DHT, identity, coordination)
//! - QUIC/mTLS (data plane: tensor activations, token streams)
//!
//! This crate provides a `HybridNode` struct that integrates these three layers
//! into a locality-aware inference scheduler for distributed LLM workloads.

pub mod config;
pub mod error;
pub mod identity;
pub mod model;
pub mod observability;
pub mod scheduler;
pub mod sdwan;
pub mod topology;

pub use config::HybridNodeConfig;
pub use error::HybridNodeError;
pub use scheduler::{SchedulingContext, SchedulingDecision, SchedulingStrategy};
pub use topology::{LinkSla, NodeTopology, SiteId};

use anyhow::Result;
use tracing::{info, warn};

/// Entry point for the HybridNode runtime.
pub struct HybridNode {
    pub config: HybridNodeConfig,
}

impl HybridNode {
    /// Initialize a HybridNode from a configuration file path.
    pub async fn from_config(path: &str) -> Result<Self> {
        let config = config::load_config(path)?;
        info!("HybridNode initialized from {path}");
        Ok(Self { config })
    }

    /// Run the HybridNode daemon (blocks until shutdown signal).
    pub async fn run(&self) -> Result<()> {
        info!("HybridNode starting — mode={:?}", self.config.mode);

        // Start observability first so we can track startup failures
        observability::start_prometheus(&self.config.observability).await?;

        // Connect to Holochain conductor
        let _identity = identity::load_from_conductor(&self.config).await?;

        // Poll SD-WAN topology
        let _topology = sdwan::connect(&self.config.sdwan).await?;

        info!("HybridNode ready");

        // Keep alive until Ctrl-C
        tokio::signal::ctrl_c().await?;
        warn!("Shutdown signal received");
        Ok(())
    }
}
