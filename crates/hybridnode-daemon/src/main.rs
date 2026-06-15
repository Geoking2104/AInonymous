use anyhow::Result;
use clap::Parser;
use hybridnode_core::HybridNode;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(
    name = "hybridnode",
    version = "0.1.0",
    about = "HybridNode daemon — SD-WAN + Holochain + QUIC/mTLS for AInonymous"
)]
struct Cli {
    /// Path to hybridnode configuration file (YAML)
    #[arg(short, long, default_value = "ainonymous.hybridnode.yaml")]
    config: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Output logs as JSON (for log aggregators)
    #[arg(long, default_value_t = false)]
    json_logs: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cli.log_level));

    if cli.json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();
    }

    tracing::info!("Starting HybridNode daemon — config={}", cli.config);

    let node = HybridNode::from_config(&cli.config).await?;
    node.run().await?;

    Ok(())
}
