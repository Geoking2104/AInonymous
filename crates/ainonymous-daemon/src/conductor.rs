use std::sync::Arc;
use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use ainonymous_quic::{SessionOffer, QuicSession, ActivationTransfer, TokenStream};
use ainonymous_types::inference::{ActivationHeader, GeneratedToken};
use crate::{DaemonConfig, holochain::HolochainClient};
use crate::pipeline_client::{PipelineClient, PrefillRequest, DecodeRequest};

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

    // Vérifier si c'est le dernier nœud (le plan d'exécution est dans l'offre)
    let is_last_node = output.is_last_node;

    if is_last_node {
        // Décoder en streaming et envoyer les tokens au nœud coordinateur
        if let Some(first_token_id) = output.first_token_id {
            stream_tokens_to_coordinator(
                &session, &request_id, first_token_id,
                &header, pipeline,
            ).await?;
        }
    } else {
        // Transmettre les activations au nœud suivant
        let out_header = ActivationHeader {
            request_id: header.request_id,
            layer_start: layer_range.1,
            layer_end: layer_range.1,   // sera mis à jour par le prochain nœud
            seq_len: output.seq_len as u32,
            hidden_size: output.hidden_size as u32,
            dtype: header.dtype,
            compressed: false,
        };

        let out_activations = output.hidden_states_bytes
            .ok_or_else(|| anyhow::anyhow!("hidden_states manquants en sortie"))?;

        forward_activations_to_next(
            &out_header, &out_activations, &offer, holochain,
        ).await?;
    }

    // Nettoyage KV-cache si dernier nœud
    if is_last_node {
        let _ = pipeline.clear(&request_id).await;
    }

    Ok(())
}

// ── Exécution réelle des couches ─────────────────────────────────────────────

struct LayerOutput {
    hidden_states_bytes: Option<Vec<u8>>,
    seq_len: usize,
    hidden_size: usize,
    is_last_node: bool,
    first_token_id: Option<i32>,
}

/// Appel au pipeline_server.py pour exécuter la tranche de couches.
/// Gère le cas prefill (premier appel) et decode (passes suivantes).
async fn run_pipeline_layers_real(
    request_id: &str,
    header: &ActivationHeader,
    raw_activations: &[u8],
    pipeline: &PipelineClient,
) -> Result<LayerOutput> {
    // Déterminer si c'est un prefill ou un decode
    // Convention : layer_start == 0 && pas de KV-cache en mémoire → prefill
    let is_prefill = header.layer_start == 0 && header.seq_len > 1;

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
    }
}

// ── Streaming tokens ─────────────────────────────────────────────────────────

/// Dernier nœud : génère les tokens en boucle decode et les streame via QUIC.
async fn stream_tokens_to_coordinator(
    session: &QuicSession,
    request_id: &str,
    first_token_id: i32,
    header: &ActivationHeader,
    pipeline: &PipelineClient,
) -> Result<()> {
    let max_new_tokens = 512usize; // TODO: récupérer depuis la requête originale
    let eos_token_id = 1i32;       // Gemma4 EOS token (1 = <eos>)

    let mut token_stream = TokenStream::sender(session)
        .await
        .map_err(|e| anyhow::anyhow!("TokenStream: {}", e))?;

    // Streamer le premier token (issu du prefill)
    let first_tok = GeneratedToken {
        token_id: first_token_id as u32,
        text: String::new(), // le texte sera décodé par le coordinateur
        logprob: None,
        finish_reason: None,
    };
    token_stream.send_token(&first_tok)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if first_token_id == eos_token_id {
        token_stream.finish()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        return Ok(());
    }

    // Boucle de décodage
    let mut last_token_id = first_token_id;

    for _ in 0..max_new_tokens - 1 {
        // Decode pass : le premier nœud du pipeline reçoit le dernier token
        // et relance le forward pass complet via QUIC
        // Dans cette architecture single-node (dernier nœud = nœud courant),
        // on peut boucler localement
        let resp = pipeline.decode(&DecodeRequest {
            request_id: request_id.to_string(),
            input_ids: Some(vec![last_token_id]),
            hidden_states_b64: None,
            seq_len: 1,
            hidden_size: header.hidden_size as usize,
        }).await?;

        let token_id = resp.next_token_id
            .ok_or_else(|| anyhow::anyhow!("next_token_id manquant"))?;

        let tok = GeneratedToken {
            token_id: token_id as u32,
            text: resp.next_token_text.unwrap_or_default(),
            logprob: None,
            finish_reason: if token_id == eos_token_id {
                Some("stop".into())
            } else {
                None
            },
        };

        token_stream.send_token(&tok)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        last_token_id = token_id;

        if token_id == eos_token_id {
            break;
        }
    }

    token_stream.finish()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    info!("Stream tokens terminé pour {}", request_id);
    Ok(())
}

// ── Transmission au nœud suivant ─────────────────────────────────────────────

async fn forward_activations_to_next(
    header: &ActivationHeader,
    activations: &[u8],
    offer: &SessionOffer,
    holochain: &HolochainClient,
) -> Result<()> {
    // 1. Le plan d'exécution dans l'offre donne l'agent suivant
    let next_agent = offer.next_agent_id.as_deref()
        .ok_or_else(|| anyhow::anyhow!("Pas de nœud suivant dans l'offre de session"))?;

    // 2. Négocier une session QUIC avec le nœud suivant
    let next_offer = holochain.negotiate_quic_session(
        next_agent,
        offer.next_layer_range,
    ).await.context("Négociation QUIC nœud suivant")?;

    // 3. Connexion QUIC directe
    let endpoint = next_offer.quic_endpoint
        .ok_or_else(|| anyhow::anyhow!("endpoint QUIC absent dans l'offre"))?;

    let next_session = ainonymous_quic::QuicSession::connect(endpoint, &next_offer)
        .await
        .context("Connexion QUIC nœud suivant")?;

    // 4. Envoyer les activations
    ActivationTransfer::send(&next_session, header, activations)
        .await
        .context("Envoi activations nœud suivant")?;

    debug!(
        "Activations transmises → {} (couches [{}, {}[)",
        next_agent, header.layer_start, header.layer_end
    );

    Ok(())
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
