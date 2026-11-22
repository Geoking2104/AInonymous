use std::{convert::Infallible, sync::Arc, time::Duration};

use ainonymous_types::{
    api::{ChatCompletionChunk, ChatCompletionRequest},
    ExecutionPlan,
};
use axum::response::{
    sse::{Event, KeepAlive},
    Sse,
};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;

use super::chat::build_llama_request;
use crate::AppState;

pub(super) fn response(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
    id: String,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = async_stream::stream! {
        let first = ChatCompletionChunk::first(&id, &req.model);
        yield Ok(Event::default().json_data(first).expect("serializable chat chunk"));

        let chunks = generate(&state, &req, &id);
        futures::pin_mut!(chunks);
        while let Some(result) = chunks.next().await {
            match result {
                Ok(chunk) => yield Ok(Event::default().json_data(chunk).expect("serializable chat chunk")),
                Err(reason) => {
                    tracing::warn!(request_id = %id, reason, "Chat stream interrupted");
                    let error = json!({
                        "id": id,
                        "error": {
                            "type": "upstream_error",
                            "code": "STREAM_INTERRUPTED",
                            "message": reason,
                        },
                    });
                    yield Ok(Event::default().json_data(error).expect("serializable stream error"));
                    return;
                }
            }
        }
        yield Ok(Event::default().data("[DONE]"));
    };
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn generate<'a>(
    state: &'a AppState,
    req: &'a ChatCompletionRequest,
    id: &'a str,
) -> impl Stream<Item = Result<ChatCompletionChunk, &'static str>> + 'a {
    async_stream::try_stream! {
        let model = &req.model;
        let plan = state.mesh.get_execution_plan(model).await;
        let mut finish_reason = None;
        if matches!(plan, Ok(ExecutionPlan::PipelineSplit { .. })) {
            let messages = serde_json::to_value(&req.messages).map_err(|_| "Invalid chat messages")?;
            let result = state.mesh.run_mesh_inference(model, &messages, req.max_tokens)
                .await.map_err(|_| "Pipeline inference failed")?;
            let content = result["content"].as_str().ok_or("Invalid pipeline response")?;
            for word in content.split_inclusive(' ') {
                yield ChatCompletionChunk::token(id, model, word);
            }
        } else {
            let body = build_llama_request(req, true);
            // A long generation may outlive the blocking mesh client's total timeout.
            let response = reqwest::Client::new()
                .post(format!("{}/v1/chat/completions", state.config.llama_server_url))
                .json(&body).send().await.map_err(|_| "Model server unavailable")?;
            if !response.status().is_success() {
                Err("Model server rejected the request")?;
            }
            let mut bytes = response.bytes_stream();
            let mut decoder = EventDecoder::default();
            let mut saw_choice = false;
            'events: loop {
                let part = bytes.next().await.ok_or("Model stream ended before completion")?
                    .map_err(|_| "Model stream transport failed")?;
                for byte in part {
                    let Some(data) = decoder.push(byte)? else { continue };
                    if data == "[DONE]" {
                        if !saw_choice {
                            Err("Model stream completed without a choice")?;
                        }
                        break 'events;
                    }
                    let chunk: UpstreamChunk = serde_json::from_str(&data)
                        .map_err(|_| "Invalid model stream event")?;
                    if chunk.error.is_some() {
                        Err("Model server interrupted generation")?;
                    }
                    let choices = chunk.choices.ok_or("Model stream event has no choices")?;
                    if let Some(choice) = choices.first() {
                        saw_choice = true;
                        if let Some(content) = &choice.delta.content {
                            yield ChatCompletionChunk::token(id, model, content);
                        }
                        if let Some(reason) = &choice.finish_reason {
                            finish_reason = Some(reason.clone());
                        }
                    }
                }
            }
        }
        let mut done = ChatCompletionChunk::done(id, model);
        if let Some(reason) = finish_reason {
            done.choices[0].finish_reason = Some(reason);
        }
        yield done;
    }
}

#[derive(Deserialize)]
struct UpstreamChunk {
    choices: Option<Vec<UpstreamChoice>>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct UpstreamChoice {
    delta: UpstreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct UpstreamDelta {
    content: Option<String>,
}

// HTTP body boundaries may occur anywhere, including inside a UTF-8 character.
// Dispatch SSE data only at a blank line. Bound both a partial line and joined data.
#[derive(Default)]
struct EventDecoder {
    line: Vec<u8>,
    data: String,
    after_cr: bool,
}

impl EventDecoder {
    const LIMIT: usize = 1024 * 1024;

    fn push(&mut self, byte: u8) -> Result<Option<String>, &'static str> {
        let after_cr = self.after_cr;
        self.after_cr = byte == b'\r';
        if after_cr && byte == b'\n' {
            return Ok(None);
        }
        if !matches!(byte, b'\r' | b'\n') {
            if self.line.len() >= Self::LIMIT {
                return Err("Model stream line exceeds limit");
            }
            self.line.push(byte);
            return Ok(None);
        }
        if self.line.is_empty() {
            if self.data.is_empty() {
                return Ok(None);
            }
            self.data.pop(); // remove the final SSE data-line separator
            return Ok(Some(std::mem::take(&mut self.data)));
        }
        let line = std::str::from_utf8(&self.line).map_err(|_| "Invalid UTF-8 in model stream")?;
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            let value = value.strip_prefix(' ').unwrap_or(value);
            if self.data.len() + value.len() + 1 > Self::LIMIT {
                return Err("Model stream event exceeds limit");
            }
            self.data.push_str(value);
            self.data.push('\n');
        }
        self.line.clear();
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, routing::post, Json, Router};
    use serde_json::Value;

    #[test]
    fn split_bytes_and_line_endings_preserve_data() {
        for newline in ["\r", "\n", "\r\n"] {
            let mut decoder = EventDecoder::default();
            let text = [": ping", "data: café", "data:avec lait", "", ""].join(newline);
            let mut events = Vec::new();
            for byte in text.bytes() {
                if let Some(data) = decoder.push(byte).unwrap() {
                    events.push(data);
                }
            }
            assert_eq!(events, ["café\navec lait"]);
        }
    }

    #[test]
    fn bounds_partial_lines_and_multiline_events() {
        let mut decoder = EventDecoder::default();
        for _ in 0..EventDecoder::LIMIT {
            decoder.push(b'x').unwrap();
        }
        assert!(decoder.push(b'x').is_err());

        let mut decoder = EventDecoder::default();
        let line = format!("data:{}\n", "x".repeat(EventDecoder::LIMIT / 2));
        assert!(line
            .bytes()
            .chain(line.bytes())
            .any(|b| decoder.push(b).is_err()));
    }

    async fn listen(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        url
    }

    async fn chat(proxy: &str, model: &str) -> (Vec<Value>, bool) {
        let raw = reqwest::Client::new()
            .post(format!("{proxy}/v1/chat/completions"))
            .json(&json!({"model":model, "messages":[{"role":"user","content":"synthetic"}], "stream":true}))
            .send().await.unwrap().error_for_status().unwrap().text().await.unwrap();
        assert!(!raw.contains("internal fixture details"));
        let mut data: Vec<_> = raw
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .collect();
        let done = data.last() == Some(&"[DONE]");
        if done {
            data.pop();
        }
        (
            data.into_iter()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect(),
            done,
        )
    }

    #[tokio::test]
    async fn http_chat_reports_one_terminal_outcome() {
        let llama = listen(Router::new().route("/v1/chat/completions", post(|Json(req): Json<Value>| async move {
            let token = "data: {\"choices\":[{\"delta\":{\"content\":\"café\"}}]}\n\n";
            let end = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n";
            let (status, body) = match req["model"].as_str().unwrap() {
                "ok" => (StatusCode::OK, format!("{token}{end}")),
                "cut" => (StatusCode::OK, token.to_string()),
                "bad-json" => (StatusCode::OK, format!("{token}data: {{oops\n\ndata: [DONE]\n\n")),
                "error-event" => (StatusCode::OK, format!("{token}data: {{\"error\":\"internal fixture details\"}}\n\n")),
                _ => (StatusCode::SERVICE_UNAVAILABLE, "internal fixture details".into()),
            };
            (status, [("content-type", "text/event-stream")], body)
        }))).await;
        let state = AppState::new(crate::ProxyConfig {
            host: "127.0.0.1".into(),
            port: 0,
            llama_server_url: llama,
            holochain_ws_url: String::new(),
            daemon_url: crate::mesh_client::tests::fake_daemon_url(),
        })
        .await
        .unwrap();
        let proxy = listen(crate::router::build(Arc::new(state))).await;

        for (model, content, finish) in [("ok", "café", "length"), ("mesh-ok", "two words", "stop")]
        {
            let (chunks, done) = chat(&proxy, model).await;
            assert!(done);
            let output: String = chunks
                .iter()
                .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
                .collect();
            assert_eq!(output, content);
            let reasons: Vec<_> = chunks
                .iter()
                .filter_map(|c| c["choices"][0]["finish_reason"].as_str())
                .collect();
            assert_eq!(reasons, [finish]);
        }
        for model in [
            "cut",
            "bad-json",
            "error-event",
            "http-error",
            "mesh-down",
            "mesh-malformed",
        ] {
            let (chunks, done) = chat(&proxy, model).await;
            assert!(!done, "{model}");
            let (last, preceding) = chunks.split_last().unwrap();
            assert_eq!(last["error"]["code"], "STREAM_INTERRUPTED");
            assert_eq!(last["id"], preceding[0]["id"]);
            assert!(preceding
                .iter()
                .all(|c| c["error"].is_null() && c["choices"][0]["finish_reason"].is_null()));
            if model == "cut" {
                assert_eq!(
                    preceding.last().unwrap()["choices"][0]["delta"]["content"],
                    "café"
                );
            }
        }
    }
}
