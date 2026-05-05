pub mod start;
pub mod control;
pub mod status;
pub mod goose;
pub mod mcp;
pub mod model;
pub mod blackboard;
pub mod nodes;

use anyhow::Result;
use reqwest::Client;

pub fn api_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap()
}

pub async fn check_daemon_running(api_url: &str) -> bool {
    api_client()
        .get(format!("{}/health", api_url.trim_end_matches("/v1")))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
