use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use hex;
use serde::Deserialize;
use tower_http::trace::TraceLayer;

use ainonymous_quic::{NodeIdentity, SessionOffer, SessionRegistry};
use crate::{conductor::Conductor, holochain::HolochainClient, DaemonConfig};

#[derive(Clone)]
struct DaemonState {
    conductor: Arc<Conductor>,
    holochain: HolochainClient,
    /// Registre des sessions QUIC en attente (plan de contrôle)
    registry: SessionRegistry,
    /// Endpoint QUIC public annoncé aux pairs
    quic_endpoint: SocketAddr,
    /// Identité ed25519 du nœud (mTLS + AgentPubKey annoncé)
    identity: NodeIdentity,
    /// Config du daemon (pour re-announce après rotation de clé)
    config: DaemonConfig,
    /// Chemin du fichier de seed ed25519 (pour rotation de clé)
    identity_path: PathBuf,
}

pub fn build(
    conductor: Arc<Conductor>,
    holochain: HolochainClient,
    registry: SessionRegistry,
    quic_endpoint: SocketAddr,
    identity: NodeIdentity,
    config: DaemonConfig,
    identity_path: PathBuf,
) -> Router {
    let state = DaemonState { conductor, holochain, registry, quic_endpoint, identity, config, identity_path };

    Router::new()
        // Endpoints pour le proxy ainonymous-proxy
        .route("/mesh/status", get(mesh_status))
        .route("/mesh/nodes", get(mesh_nodes))
        .route("/mesh/plan", post(mesh_plan))
        .route("/mesh/metrics", post(mesh_metrics))
        .route("/mesh/blackboard/post", post(blackboard_post))
        .route("/mesh/blackboard/search", post(blackboard_search))

        // Plan de contrôle : négociation de session QUIC entre pairs
        .route("/mesh/session/negotiate", post(session_negotiate))

        // Coordinateur : inférence distribuée (pipeline-split)
        .route("/mesh/infer", post(mesh_infer))

        // Endpoints internes (zome calls via daemon)
        .route("/zome/:dna/:zome/:function", post(zome_call))

        // Administration du nœud (palier E)
        .route("/daemon/rotate-identity", post(rotate_identity))

        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Deserialize)]
struct NegotiateBody {
    #[serde(default)]
    layer_range: Option<(u32, u32)>,
    #[serde(default)]
    next_agent_id: Option<String>,
    #[serde(default)]
    next_layer_range: Option<(u32, u32)>,
}

/// POST /mesh/session/negotiate
/// Un pair demande à ouvrir une session QUIC entrante sur ce nœud.
/// On génère une offre (token + endpoint), on l'enregistre dans le listener,
/// puis on la retourne. Le pair se connectera ensuite en QUIC avec ce token.
async fn session_negotiate(
    State(s): State<DaemonState>,
    Json(body): Json<NegotiateBody>,
) -> impl IntoResponse {
    let mut offer = SessionOffer::new(s.quic_endpoint, body.layer_range);
    offer.next_agent_id = body.next_agent_id;
    offer.next_layer_range = body.next_layer_range;
    // Annonce notre clé ed25519 pour que le pair vérifie le certificat mTLS.
    offer.peer_pubkey = Some(s.identity.public_key_bytes());

    s.registry.register(offer.clone());
    Json(offer)
}

#[derive(Deserialize)]
struct InferBody {
    model_id: String,
    /// Messages chat (format OpenAI) à tokeniser puis exécuter
    messages: serde_json::Value,
    #[serde(default)]
    max_tokens: u32,
}

/// POST /mesh/infer
/// Coordinateur : calcule le plan d'exécution puis lance l'inférence distribuée.
async fn mesh_infer(
    State(s): State<DaemonState>,
    Json(body): Json<InferBody>,
) -> impl IntoResponse {
    // Plan statique (testnet) prioritaire, sinon plan calculé par Holochain.
    let plan = match crate::conductor::static_plan_from_config(&s.conductor.config) {
        Some(p) => p,
        None => match s.holochain.get_execution_plan(&body.model_id).await {
            Ok(p) => p,
            Err(e) => return (StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": format!("plan indisponible: {}", e)}))).into_response(),
        },
    };

    match crate::conductor::run_pipeline_inference(
        &s.holochain, &s.conductor.pipeline, &plan, body.messages, body.max_tokens,
        &s.identity, s.conductor.eos_token_id,
    ).await {
        Ok(r) => Json(serde_json::json!({
            "content": r.text,
            "token_count": r.token_count,
            "node_ids": r.node_ids,
            "execution_mode": "pipeline_split",
        })).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn mesh_status(State(s): State<DaemonState>) -> impl IntoResponse {
    match s.holochain.get_execution_plan("").await {
        Ok(_) => Json(serde_json::json!({
            "local_node": { "status": "active" },
            "mesh": { "status": "connected" }
        })).into_response(),
        Err(_) => Json(serde_json::json!({
            "local_node": { "status": "degraded" },
            "mesh": { "status": "connecting" }
        })).into_response(),
    }
}

#[derive(Deserialize)]
struct ModelQuery { model_id: Option<String> }

async fn mesh_nodes(
    State(s): State<DaemonState>,
    Query(q): Query<ModelQuery>,
) -> impl IntoResponse {
    let model_id = q.model_id.as_deref().unwrap_or("");
    match s.holochain.get_available_nodes(model_id).await {
        Ok(nodes) => Json(nodes).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn mesh_plan(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Plan statique (testnet) prioritaire, sinon plan calculé par Holochain.
    if let Some(plan) = crate::conductor::static_plan_from_config(&s.conductor.config) {
        return Json(plan).into_response();
    }
    let model_id = body["model_id"].as_str().unwrap_or("");
    match s.holochain.get_execution_plan(model_id).await {
        Ok(plan) => Json(plan).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn mesh_metrics(
    State(_s): State<DaemonState>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // TODO: publier les métriques via zome call Holochain
    StatusCode::OK
}

async fn blackboard_post(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let prefix = body["prefix"].as_str().unwrap_or("STATUS");
    let content = body["content"].as_str().unwrap_or("");
    let tags = body["tags"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    match s.holochain.blackboard_post(prefix, content, tags).await {
        Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({"status": "posted"}))).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn blackboard_search(
    State(s): State<DaemonState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let terms = body["terms"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let prefix_filter = body["prefix_filter"].as_str().map(String::from);

    match s.holochain.blackboard_search(terms, prefix_filter).await {
        Ok(results) => Json(results).into_response(),
        Err(_) => Json(serde_json::json!({"posts": [], "total": 0})).into_response(),
    }
}

/// Route générique pour les zome calls (délégation vers Holochain)
async fn zome_call(
    State(s): State<DaemonState>,
    Path((dna, zome, function)): Path<(String, String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match s.holochain.zome_call(&dna, &zome, &function, payload).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

/// POST /daemon/rotate-identity
///
/// Palier E — Rotation de clé ed25519 du nœud.
/// Génère une nouvelle clé, l'écrit dans `identity_path`, et re-annonce la
/// nouvelle clé publique dans le DHT. La connexion mTLS QUIC actuelle reste
/// active avec l'ancienne clé jusqu'au prochain redémarrage du daemon.
///
/// Réponse :
/// ```json
/// {
///   "old_pubkey": "<hex>",
///   "new_pubkey": "<hex>",
///   "restart_required": true,
///   "dht_updated": true | false
/// }
/// ```
async fn rotate_identity(State(s): State<DaemonState>) -> impl IntoResponse {
    // Générer la nouvelle clé et écraser le fichier
    let (new_identity, old_pubkey_bytes) = match NodeIdentity::rotate_file(&s.identity_path) {
        Ok(pair) => pair,
        Err(e) => return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("rotation échouée: {e}")})),
        ).into_response(),
    };

    let old_pubkey_hex = hex::encode(old_pubkey_bytes);
    let new_pubkey_hex = new_identity.public_key_hex();

    // Re-annoncer dans le DHT (non-fatal)
    let dht_updated = s.holochain
        .reannounce_pubkey(&new_pubkey_hex, &s.config)
        .await
        .is_ok();

    Json(serde_json::json!({
        "old_pubkey": old_pubkey_hex,
        "new_pubkey": new_pubkey_hex,
        "restart_required": true,
        "dht_updated": dht_updated,
    })).into_response()
}
