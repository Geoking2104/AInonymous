use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::llama::{LlamaClient, LlamaManager};
use crate::pipeline_client::PipelineClient;
use crate::DaemonConfig;

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

fn private_config(runtime: SocketAddr) -> DaemonConfig {
    DaemonConfig {
        conductor_only: true,
        llama_server_port: runtime.port(),
        pipeline_server_port: runtime.port(),
        ..Default::default()
    }
}

#[tokio::test]
async fn private_runtime_accepts_direct_responses() {
    let runtime = serve(
        Router::new()
            .route("/health", get(|| async { "ready" }))
            .route("/tokenize", post(|| async { Json(json!({"input_ids": [7]})) }))
            .route("/clear", post(|| async { "cleared" })),
    ).await;
    let config = private_config(runtime);
    assert!(LlamaClient::new(&config).unwrap().is_ready().await);
    let pipeline = PipelineClient::new(&config).unwrap();
    assert_eq!(pipeline.tokenize(json!([])).await.unwrap(), vec![7]);
    pipeline.clear("test-request").await.unwrap();
    assert!(LlamaManager::new(config).unwrap().is_running().await);
}

#[tokio::test]
async fn private_runtime_refuses_redirects_before_contacting_a_second_service() {
    let visits = Arc::new(AtomicUsize::new(0));
    let observed = visits.clone();
    let other = serve(Router::new().fallback(move || {
        observed.fetch_add(1, Ordering::SeqCst);
        async { "unexpected destination" }
    })).await;
    let target = format!("http://{other}/");
    let runtime = serve(Router::new().fallback(move || {
        let redirect = Redirect::temporary(&target);
        async move { redirect }
    })).await;
    let config = private_config(runtime);
    let llama = LlamaClient::new(&config).unwrap();
    let pipeline = PipelineClient::new(&config).unwrap();
    assert!(!llama.is_ready().await);
    assert!(llama.chat_completions("fixture", &json!([]), 1).await.is_err());
    assert!(pipeline.tokenize(json!([])).await.is_err());
    assert!(pipeline.clear("test-request").await.is_err());
    assert!(!LlamaManager::new(config).unwrap().is_running().await);
    assert_eq!(visits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn private_policy_clears_proxy_selection_while_development_keeps_it() {
    let local = serve(Router::new().fallback(|| async { "local" })).await;
    let proxy = serve(Router::new().fallback(|| async { "proxy" })).await;
    for (conductor_only, expected) in [(false, "proxy"), (true, "local")] {
        let config = DaemonConfig { conductor_only, ..Default::default() };
        let builder = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{proxy}")).unwrap());
        let response = config.build_runtime_http(builder).unwrap()
            .get(format!("http://{local}/health")).send().await.unwrap();
        assert_eq!(response.text().await.unwrap(), expected);
    }
}
