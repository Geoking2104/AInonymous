//! Serveur MCP (Model Context Protocol) pour AInonymous.
//! Expose les capacités du mesh Holochain comme outils pour Goose.
//!
//! Protocole MCP stdio : lecture de requêtes JSON-RPC sur stdin, écriture sur stdout.
//! https://modelcontextprotocol.io/spec

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use tracing::debug;

#[derive(Parser)]
#[command(name = "ainonymous-mcp")]
struct Args {
    /// Limiter aux outils d'une DNA spécifique
    #[arg(long)]
    dna: Option<String>,

    /// URL du proxy ainonymous
    #[arg(long, default_value = "http://127.0.0.1:9337/v1")]
    api_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // MCP utilise stdio — les logs doivent aller sur stderr uniquement
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let server = McpServer::new(args.api_url, args.dna);
    server.run().await
}

struct McpServer {
    api_url: String,
    dna_filter: Option<String>,
    client: reqwest::Client,
}

impl McpServer {
    fn new(api_url: String, dna_filter: Option<String>) -> Self {
        Self {
            api_url,
            dna_filter,
            client: reqwest::Client::new(),
        }
    }

    async fn run(self) -> Result<()> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        for line in stdin.lock().lines() {
            let line = line?;
            if line.is_empty() { continue; }

            debug!("MCP ← {}", line);

            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let err = json_rpc_error(None, -32700, &format!("Parse error: {}", e));
                    writeln!(stdout, "{}", serde_json::to_string(&err)?)?;
                    stdout.flush()?;
                    continue;
                }
            };

            let id = request.get("id").cloned();
            let method = request["method"].as_str().unwrap_or("");
            let params = request.get("params").cloned().unwrap_or(json!({}));

            let response = self.handle(id.clone(), method, params).await;
            let response_str = serde_json::to_string(&response)?;
            debug!("MCP → {}", response_str);

            writeln!(stdout, "{}", response_str)?;
            stdout.flush()?;
        }

        Ok(())
    }

    async fn handle(&self, id: Option<Value>, method: &str, params: Value) -> Value {
        match method {
            "initialize" => self.handle_initialize(id),
            "tools/list"  => self.handle_tools_list(id),
            "tools/call"  => self.handle_tool_call(id, params).await,
            "ping"        => json_rpc_ok(id, json!({})),
            _ => json_rpc_error(id, -32601, &format!("Method not found: {}", method)),
        }
    }

    fn handle_initialize(&self, id: Option<Value>) -> Value {
        json_rpc_ok(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "ainonymous-mesh",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }))
    }

    fn handle_tools_list(&self, id: Option<Value>) -> Value {
        let mut tools = vec![
            tool_def("mesh_query_nodes",
                "Lister les nœuds disponibles dans le mesh AInonymous pour un modèle donné",
                json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string", "description": "ex: gemma4-31b, gemma4-26b-moe"},
                        "min_vram_gb": {"type": "number"},
                        "region": {"type": "string"}
                    },
                    "required": ["model_id"]
                })
            ),
            tool_def("mesh_run_inference",
                "Exécuter une inférence LLM sur le mesh distribué AInonymous",
                json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string"},
                        "prompt": {"type": "string"},
                        "max_tokens": {"type": "integer", "default": 2048},
                        "temperature": {"type": "number", "default": 0.7}
                    },
                    "required": ["model_id", "prompt"]
                })
            ),
            tool_def("mesh_get_status",
                "Obtenir le statut actuel du mesh (pairs actifs, latence, VRAM totale)",
                json!({"type": "object", "properties": {}})
            ),
        ];

        // Outils Blackboard (disponibles toujours ou si dna=blackboard)
        if self.dna_filter.is_none() || self.dna_filter.as_deref() == Some("blackboard") {
            tools.push(tool_def("blackboard_post",
                "Publier un message sur le blackboard partagé des agents AInonymous",
                json!({
                    "type": "object",
                    "properties": {
                        "prefix": {
                            "type": "string",
                            "enum": ["STATUS", "FINDING", "QUESTION", "TIP", "DONE"],
                            "description": "Préfixe du message"
                        },
                        "content": {"type": "string", "maxLength": 4096},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "ttl_hours": {"type": "integer", "default": 48}
                    },
                    "required": ["prefix", "content"]
                })
            ));
            tools.push(tool_def("blackboard_search",
                "Rechercher dans le blackboard partagé des agents. Utilise une recherche OR multi-termes.",
                json!({
                    "type": "object",
                    "properties": {
                        "terms": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Termes de recherche (OR logique)"
                        },
                        "prefix_filter": {
                            "type": "string",
                            "enum": ["STATUS", "FINDING", "QUESTION", "TIP", "DONE"]
                        },
                        "limit": {"type": "integer", "default": 20}
                    },
                    "required": ["terms"]
                })
            ));
        }

        json_rpc_ok(id, json!({ "tools": tools }))
    }

    async fn handle_tool_call(&self, id: Option<Value>, params: Value) -> Value {
        let tool_name = match params["name"].as_str() {
            Some(n) => n.to_string(),
            None => return json_rpc_error(id, -32602, "Paramètre 'name' manquant"),
        };
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        let result = match tool_name.as_str() {
            "mesh_query_nodes"   => self.tool_query_nodes(args).await,
            "mesh_run_inference" => self.tool_run_inference(args).await,
            "mesh_get_status"    => self.tool_get_status().await,
            "blackboard_post"    => self.tool_blackboard_post(args).await,
            "blackboard_search"  => self.tool_blackboard_search(args).await,
            other => Err(anyhow::anyhow!("Outil inconnu: {}", other)),
        };

        match result {
            Ok(text) => json_rpc_ok(id, json!({
                "content": [{"type": "text", "text": text}]
            })),
            Err(e) => json_rpc_ok(id, json!({
                "content": [{"type": "text", "text": format!("Erreur: {}", e)}],
                "isError": true
            })),
        }
    }

    async fn tool_query_nodes(&self, args: Value) -> Result<String> {
        let model_id = args["model_id"].as_str().unwrap_or("gemma4-31b");
        let resp = self.client
            .get(format!("{}/ainonymous/mesh/nodes", self.api_url))
            .query(&[("model_id", model_id)])
            .send().await?;
        let nodes: Value = resp.json().await?;
        Ok(serde_json::to_string_pretty(&nodes)?)
    }

    async fn tool_run_inference(&self, args: Value) -> Result<String> {
        let model_id = args["model_id"].as_str().unwrap_or("gemma4-31b");
        let prompt = args["prompt"].as_str().unwrap_or("");
        let max_tokens = args["max_tokens"].as_u64().unwrap_or(2048);
        let temperature = args["temperature"].as_f64().unwrap_or(0.7);

        let resp = self.client
            .post(format!("{}/chat/completions", self.api_url))
            .json(&json!({
                "model": model_id,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": max_tokens,
                "temperature": temperature,
            }))
            .send().await?;

        let result: Value = resp.json().await?;
        let content = result["choices"][0]["message"]["content"]
            .as_str().unwrap_or("").to_string();
        Ok(content)
    }

    async fn tool_get_status(&self) -> Result<String> {
        let resp = self.client
            .get(format!("{}/ainonymous/mesh/status", self.api_url))
            .send().await?;
        let status: Value = resp.json().await?;
        Ok(serde_json::to_string_pretty(&status)?)
    }

    async fn tool_blackboard_post(&self, args: Value) -> Result<String> {
        let resp = self.client
            .post(format!("{}/ainonymous/blackboard/post", self.api_url))
            .json(&args)
            .send().await?;
        if resp.status().is_success() {
            Ok(format!("Publié: {} — {}",
                args["prefix"].as_str().unwrap_or("?"),
                args["content"].as_str().unwrap_or("?")))
        } else {
            Err(anyhow::anyhow!("Erreur publication: {}", resp.text().await?))
        }
    }

    async fn tool_blackboard_search(&self, args: Value) -> Result<String> {
        let terms = args["terms"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("+"))
            .unwrap_or_default();

        let mut req = self.client
            .get(format!("{}/ainonymous/blackboard/search", self.api_url))
            .query(&[("q", &terms)]);

        if let Some(prefix) = args["prefix_filter"].as_str() {
            req = req.query(&[("prefix", prefix)]);
        }

        let resp = req.send().await?;
        let results: Value = resp.json().await?;
        Ok(serde_json::to_string_pretty(&results)?)
    }
}

// ─── Helpers JSON-RPC ─────────────────────────────────────────────────────────

fn json_rpc_ok(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn tool_def(name: &str, description: &str, schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": schema,
    })
}
