use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use ainonymous_quic::{SessionOffer, QuicSession, ActivationTransfer, TokenStream};
use ainonymous_types::inference::{ActivationHeader, GeneratedToken, FinishReason, DType};
use ainonymous_types::ExecutionPlan;
use crate::{DaemonConfig, holochain::HolochainClient};
use crate::pipeline_client::{PipelineClient, PrefillRequest, DecodeRequest};

/// Token de fin de séquence (Gemma 4 : <eos> = 1)
const EOS_TOKEN_ID: i32 = 1;

/// Orchestrateur central du daemon
pub struct Conductor {
    pub config: DaemonConfig,
    pub pipeline: PipelineClient,
}

impl Conductor {
    pub async fn new(config: DaemonConfig) -> Result<Self> {
        let pipeline = PipelineClient::new(config.pipeline_server_port);

        // Vérifier que le pipeline_server.py tourne
        match pipeline.health_check().await {
            Ok(status) => {
                info!(
                    "Pipeline server actif — modèle: {} couches [{}, {}[ device: {}",
                    status.model_id, status.layer_start, status.layer_end, status.device
                );
            }
            Err(e) => {
                warn!("Pipeline server inaccessible ({}). Démarrage en mode solo uniquement.", e);
            }
        }

        Ok(Self { config, pipeline })
    }
}

// ── Handler session QUIC entrante (nœud worker) ──────────────────────────────

/// Ce nœud est un worker dans un pipeline : il reçoit des activations via QUIC,
/// les fait passer par ses couches via le pipeline_server.py, puis :
///   - Transmet les activations au nœud suivant (si pas dernier), ou
///   - Streame les tokens générés vers le nœud coordinateur (si dernier)
pub async fn handle_pipeline_session(
    conn: quinn::Connection,
    offer: SessionOffer,
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
) -> Result<()> {
    let layer_range = offer.layer_range
        .unwrap_or((0, 0));
    info!(
        "Session pipeline entrante — couches [{}, {}[",
        layer_range.0, layer_range.1
    );

    let session = QuicSession {
        connection: conn,
        offer: offer.clone(),
        config: ainonymous_quic::SessionConfig::default(),
        established_at: std::time::Instant::now(),
    };

    // Recevoir les activations du nœud précédent (ou les input_ids pour le premier nœud)
    let (header, raw_activations) = ActivationTransfer::receive(&session)
        .await
        .context("Réception activations QUIC")?;

    debug!(
        "Activations reçues: couches {}-{} seq={} hidden={}",
        header.layer_start, header.layer_end, header.seq_len, header.hidden_size
    );

    // Déléguer au pipeline_server.py
    let request_id = String::from_utf8_lossy(&header.request_id).to_string();
    let output = run_pipeline_layers_real(
        &request_id, &header, &raw_activations, pipeline,
    ).await?;

    // Une passe = un token. La boucle de génération est pilotée par le
    // coordinateur (cf. ADR-001) ; le KV-cache persiste par request_id côté
    // pipeline_server entre les passes (pas de clear ici).
    if output.is_last_node {
        // Dernier nœud : produire UN token et le renvoyer en amont
        // (vers le nœud précédent ou le coordinateur) sur la session entrante.
        let token_id = output.first_token_id
            .ok_or_else(|| anyhow::anyhow!("dernier nœud sans token généré"))?;
        let tok = GeneratedToken {
            token_id: token_id as u32,
            text: output.next_token_text.clone().unwrap_or_default(),
            logprob: None,
            finish_reason: if token_id == EOS_TOKEN_ID {
                Some(FinishReason::Stop)
            } else {
                None
            },
        };
        send_single_token(&session, &tok).await?;
    } else {
        // Nœud intermédiaire : transmettre les activations au nœud suivant,
        // récupérer le token produit en bout de chaîne, et le relayer en amont.
        let out_header = ActivationHeader {
            request_id: header.request_id,
            layer_start: layer_range.1,
            layer_end: layer_range.1,
            seq_len: output.seq_len as u32,
            hidden_size: output.hidden_size as u32,
            dtype: header.dtype,
            compressed: false,
        };

        let out_activations = output.hidden_states_bytes
            .ok_or_else(|| anyhow::anyhow!("hidden_states manquants en sortie"))?;

        let tok = forward_and_receive_token(
            &out_header, &out_activations, &offer, holochain,
        ).await?;
        send_single_token(&session, &tok).await?;
    }

    Ok(())
}

// ── Côté COORDINATEUR : initiation de l'inférence pipeline (chaîne) ───────────

/// Résultat d'une inférence distribuée vue du coordinateur.
pub struct CoordinatorResult {
    pub text: String,
    pub token_count: u32,
    pub node_ids: Vec<String>,
}

/// Coordinateur : lance une inférence pipeline-split (topologie chaîne).
///
/// 1. Tokenise le prompt via le pipeline_server local (héberge embed+tokenizer).
/// 2. Négocie une session QUIC avec le 1er étage, en lui indiquant l'étage
///    suivant (chaînage A→B).
/// 3. Envoie les token_ids (u32 LE) en activations au 1er étage.
/// 4. Reçoit le flux de tokens relayé en bout de chaîne, détokenise au besoin.
///
/// NB (Phase 2, tranche sûre) : la boucle de décodage multi-token complète et le
/// relais B→A→C côté worker restent à finaliser (cf. ADR-001) ; cette fonction
/// fournit l'initiation côté coordinateur, compilée et prête à l'emploi.
pub async fn run_pipeline_inference(
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
    plan: &ExecutionPlan,
    messages: serde_json::Value,
    max_tokens: u32,
) -> Result<CoordinatorResult> {
    let stages = match plan {
        ExecutionPlan::PipelineSplit { stages } => stages,
        other => anyhow::bail!("run_pipeline_inference: plan non-pipeline ({:?})", other),
    };
    let first = stages.first()
        .ok_or_else(|| anyhow::anyhow!("plan pipeline vide"))?;
    let next = stages.get(1);

    // 1. Tokenisation (déléguée au tokenizer du modèle, 1er nœud)
    let token_ids = pipeline.tokenize(messages).await.context("tokenisation prompt")?;
    if token_ids.is_empty() {
        anyhow::bail!("tokenisation vide");
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    info!(
        "Coordinateur : {} tokens d'entrée, {} étage(s), req={}",
        token_ids.len(), stages.len(), request_id
    );

    let budget = if max_tokens == 0 { 512 } else { max_tokens };
    let mut out_ids: Vec<i32> = Vec::new();
    let mut text = String::new();

    // 2. Passe de prefill : envoie tout le prompt, reçoit le 1er token
    let mut tok = run_chain_pass(holochain, first, next, &request_id, &token_ids)
        .await.context("passe prefill")?;

    // 3. Boucle de décodage pilotée par le coordinateur (un token par passe)
    loop {
        out_ids.push(tok.token_id as i32);
        text.push_str(&tok.text);

        let is_eos = tok.token_id as i32 == EOS_TOKEN_ID || tok.finish_reason.is_some();
        if is_eos || out_ids.len() as u32 >= budget {
            break;
        }

        // Passe de décodage : renvoie le dernier token (seq_len = 1)
        let last = tok.token_id as i32;
        tok = run_chain_pass(holochain, first, next, &request_id, &[last])
            .await.context("passe decode")?;
    }

    // Repli : si le bout de chaîne n'a pas fourni le texte, détokeniser localement
    if text.is_empty() && !out_ids.is_empty() {
        text = pipeline.detokenize(&out_ids).await.unwrap_or_default();
    }

    info!("Coordinateur : génération terminée — {} tokens", out_ids.len());
    Ok(CoordinatorResult {
        text,
        token_count: out_ids.len() as u32,
        node_ids: stages.iter().map(|s| s.node.clone()).collect(),
    })
}

/// Une passe complète du pipeline (prefill ou decode) : ouvre une session vers
/// le 1er étage, envoie les token_ids, reçoit UN token relayé en bout de chaîne.
/// Le KV-cache persiste côté pipeline_server entre les passes (même request_id).
async fn run_chain_pass(
    holochain: &HolochainClient,
    first: &ainonymous_types::PipelineStage,
    next: Option<&ainonymous_types::PipelineStage>,
    request_id: &str,
    input_ids: &[i32],
) -> Result<GeneratedToken> {
    let offer = holochain.negotiate_quic_session(
        &first.node,
        Some((first.layer_start, first.layer_end)),
        next.map(|s| s.node.clone()),
        next.map(|s| (s.layer_start, s.layer_end)),
    ).await.context("négociation session 1er étage")?;

    let endpoint = ainonymous_quic::create_endpoint(None)
        .await.context("endpoint QUIC coordinateur")?;
    let session = QuicSession::connect(
        &endpoint, offer, ainonymous_quic::SessionConfig::default(),
    ).await.context("connexion QUIC 1er étage")?;

    let mut rid = [0u8; 36];
    let rb = request_id.as_bytes();
    let n = rb.len().min(36);
    rid[..n].copy_from_slice(&rb[..n]);

    let header = ActivationHeader {
        request_id: rid,
        layer_start: 0,
        layer_end: first.layer_end,
        seq_len: input_ids.len() as u32,
        hidden_size: 0,
        dtype: DType::F16,
        compressed: false,
    };
    ActivationTransfer::send(&session, header, &token_ids_to_bytes(input_ids))
        .await
        .map_err(|e| anyhow::anyhow!("envoi token_ids: {}", e))?;

    let mut ts = TokenStream::receiver(&session)
        .await
        .map_err(|e| anyhow::anyhow!("ouverture token stream: {}", e))?;
    let tok = ts.recv_token()
        .await
        .map_err(|e| anyhow::anyhow!("réception token: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("aucun token reçu"))?;
    Ok(tok)
}

/// Encode des token_ids en u32 LE (convention AInonymous, lue par
/// `parse_token_ids_from_activations`).
fn token_ids_to_bytes(ids: &[i32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(ids.len() * 4);
    for &id in ids {
        v.extend_from_slice(&id.to_le_bytes());
    }
    v
}

// ── Exécution réelle des couches ─────────────────────────────────────────────

struct LayerOutput {
    hidden_states_bytes: Option<Vec<u8>>,
    seq_len: usize,
    hidden_size: usize,
    is_last_node: bool,
    first_token_id: Option<i32>,
    next_token_text: Option<String>,
}

/// Appel au pipeline_server.py pour exécuter la tranche de couches.
/// Gère le cas prefill (premier appel) et decode (passes suivantes).
async fn run_pipeline_layers_real(
    request_id: &str,
    header: &ActivationHeader,
    raw_activations: &[u8],
    pipeline: &PipelineClient,
) -> Result<LayerOutput> {
    // Prefill = traitement du prompt complet (seq_len > 1) ; decode = 1 token.
    // (Indépendant de layer_start : un nœud intermédiaire/dernier reçoit aussi
    // un prefill avec seq_len > 1 sous forme de hidden states.)
    let is_prefill = header.seq_len > 1;

    if is_prefill && header.layer_start == 0 {
        // Cas premier nœud : les raw_activations contiennent les token_ids
        // encodés comme u32 LE (convention AInonymous)
        let input_ids = parse_token_ids_from_activations(raw_activations);

        let resp = pipeline.prefill(&PrefillRequest {
            request_id: request_id.to_string(),
            input_ids: Some(input_ids),
            hidden_states_b64: None,
            seq_len: header.seq_len as usize,
            hidden_size: 0,
        }).await.context("pipeline_server /prefill (premier nœud)")?;

        Ok(build_layer_output_from_prefill(resp))

    } else if is_prefill {
        // Nœud intermédiaire : passer les hidden states
        let b64 = crate::pipeline_client::bytes_to_b64(raw_activations);

        let resp = pipeline.prefill(&PrefillRequest {
            request_id: request_id.to_string(),
            input_ids: None,
            hidden_states_b64: Some(b64),
            seq_len: header.seq_len as usize,
            hidden_size: header.hidden_size as usize,
        }).await.context("pipeline_server /prefill (nœud intermédiaire)")?;

        Ok(build_layer_output_from_prefill(resp))

    } else {
        // Phase decode : 1 token, KV-cache déjà en mémoire sur le serveur
        let is_first_decode_node = header.layer_start == 0;

        let req = if is_first_decode_node {
            let token_ids = parse_token_ids_from_activations(raw_activations);
            DecodeRequest {
                request_id: request_id.to_string(),
                input_ids: Some(token_ids),
                hidden_states_b64: None,
                seq_len: 1,
                hidden_size: 0,
            }
        } else {
            let b64 = crate::pipeline_client::bytes_to_b64(raw_activations);
            DecodeRequest {
                request_id: request_id.to_string(),
                input_ids: None,
                hidden_states_b64: Some(b64),
                seq_len: 1,
                hidden_size: header.hidden_size as usize,
            }
        };

        let resp = pipeline.decode(&req)
            .await.context("pipeline_server /decode")?;

        Ok(LayerOutput {
            hidden_states_bytes: resp.hidden_states_b64
                .as_deref()
                .map(crate::pipeline_client::b64_to_bytes)
                .transpose()?,
            seq_len: resp.seq_len,
            hidden_size: resp.hidden_size,
            is_last_node: resp.is_last_node,
            first_token_id: resp.next_token_id,
            next_token_text: resp.next_token_text,
        })
    }
}

fn build_layer_output_from_prefill(
    resp: crate::pipeline_client::PrefillResponse
) -> LayerOutput {
    LayerOutput {
        hidden_states_bytes: resp.hidden_states_b64
            .as_deref()
            .and_then(|b64| crate::pipeline_client::b64_to_bytes(b64).ok()),
        seq_len: resp.seq_len,
        hidden_size: resp.hidden_size,
        is_last_node: resp.is_last_node,
        first_token_id: resp.next_token_id,
        next_token_text: resp.next_token_text,
    }
}

// ── Renvoi d'un token en amont ───────────────────────────────────────────────

/// Envoyer un unique token en amont (vers le nœud précédent ou le coordinateur)
/// sur la session entrante, puis clôturer le stream. La génération multi-token
/// est pilotée par le coordinateur (cf. ADR-001).
async fn send_single_token(session: &QuicSession, tok: &GeneratedToken) -> Result<()> {
    let mut ts = TokenStream::sender(session)
        .await
        .map_err(|e| anyhow::anyhow!("ouverture TokenStream: {}", e))?;
    ts.send_token(tok)
        .await
        .map_err(|e| anyhow::anyhow!("envoi token: {}", e))?;
    ts.finish()
        .await
        .map_err(|e| anyhow::anyhow!("clôture TokenStream: {}", e))?;
    Ok(())
}

// ── Transmission au nœud suivant + relais du token ───────────────────────────

/// Nœud intermédiaire : transmet les activations au nœud suivant de la chaîne,
/// puis attend le token produit en bout de chaîne (un token par passe). Le
/// token est ensuite relayé en amont par l'appelant.
async fn forward_and_receive_token(
    header: &ActivationHeader,
    activations: &[u8],
    offer: &SessionOffer,
    holochain: &HolochainClient,
) -> Result<GeneratedToken> {
    // 1. L'offre de session indique l'agent suivant
    let next_agent = offer.next_agent_id.as_deref()
        .ok_or_else(|| anyhow::anyhow!("Pas de nœud suivant dans l'offre de session"))?;

    // 2. Négocier une session QUIC avec le nœud suivant.
    // (Chaîne 2 nœuds : le suivant est le dernier → pas de next-next.)
    let next_offer = holochain.negotiate_quic_session(
        next_agent,
        offer.next_layer_range,
        None,
        None,
    ).await.context("Négociation QUIC nœud suivant")?;

    // 3. Connexion QUIC directe (endpoint client éphémère local)
    let client_endpoint = ainonymous_quic::create_endpoint(None)
        .await
        .context("Création endpoint QUIC client")?;
    let next_session = QuicSession::connect(
        &client_endpoint,
        next_offer,
        ainonymous_quic::SessionConfig::default(),
    )
        .await
        .context("Connexion QUIC nœud suivant")?;

    // 4. Envoyer les activations
    ActivationTransfer::send(&next_session, header.clone(), activations)
        .await
        .context("Envoi activations nœud suivant")?;

    debug!(
        "Activations transmises → {} (couches [{}, {}[)",
        next_agent, header.layer_start, header.layer_end
    );

    // 5. Recevoir le token produit en bout de chaîne
    let mut ts = TokenStream::receiver(&next_session)
        .await
        .map_err(|e| anyhow::anyhow!("réception token du suivant: {}", e))?;
    let tok = ts.recv_token()
        .await
        .map_err(|e| anyhow::anyhow!("lecture token du suivant: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("aucun token reçu du nœud suivant"))?;
    Ok(tok)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Les token IDs sont transmis comme u32 LE dans le champ activations
/// quand layer_start == 0 (premier nœud du pipeline)
fn parse_token_ids_from_activations(raw: &[u8]) -> Vec<i32> {
    if raw.len() % 4 != 0 {
        return vec![];
    }
    raw.chunks_exact(4)
        .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Nombre de couches selon le model_id
pub fn model_total_layers(model_id: &str) -> u32 {
    match model_id {
        id if id.contains("31b")  => 48,
        id if id.contains("26b")  => 30,
        id if id.contains("e4b")  => 18,
        id if id.contains("e2b")  => 12,
        _                         => 18, // fallback conservateur
    }
}
