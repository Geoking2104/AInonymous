use std::net::SocketAddr;
use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use ainonymous_quic::{SessionOffer, QuicSession, ActivationTransfer, TokenStream};
use ainonymous_types::inference::{ActivationHeader, GeneratedToken, FinishReason, DType};
use ainonymous_types::ExecutionPlan;
use crate::{DaemonConfig, holochain::HolochainClient};
use crate::pipeline_client::{PipelineClient, PrefillRequest, DecodeRequest};

/// Orchestrateur central du daemon
pub struct Conductor {
    pub config: DaemonConfig,
    pub pipeline: PipelineClient,
    /// Token EOS du modèle local (lu depuis pipeline_server /status au démarrage).
    /// Défaut conservateur : 1 (Gemma family). Mis à jour si pipeline_server répond.
    pub eos_token_id: i32,
    /// Nombre de tokens brouillon pour le décodage spéculatif (T2.4).
    /// 0 = décodage classique. Valeur issue de config.inference.speculative_k.
    pub speculative_k: u8,
}

impl Conductor {
    pub async fn new(config: DaemonConfig) -> Result<Self> {
        let pipeline = PipelineClient::new(config.pipeline_server_port);
        let speculative_k = config.inference.speculative_k;

        // Vérifier que le pipeline_server.py tourne ; récupérer l'EOS token
        let eos_token_id = match pipeline.health_check().await {
            Ok(status) => {
                info!(
                    "Pipeline server actif — modèle: {} couches [{}, {}[ device: {} eos={} speculative_k={}",
                    status.model_id, status.layer_start, status.layer_end,
                    status.device, status.eos_token_id, speculative_k
                );
                status.eos_token_id as i32
            }
            Err(e) => {
                warn!("Pipeline server inaccessible ({}). Démarrage en mode solo uniquement.", e);
                1 // fallback Gemma
            }
        };

        Ok(Self { config, pipeline, eos_token_id, speculative_k })
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
    identity: &ainonymous_quic::NodeIdentity,
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

    // Connexion vers le nœud suivant, négociée UNE fois puis réutilisée pour
    // toutes les passes de la requête. L'endpoint client doit vivre aussi
    // longtemps que la connexion → on le garde en scope.
    let mut next_endpoint: Option<quinn::Endpoint> = None;
    let mut next_session: Option<QuicSession> = None;
    let mut last_request_id: Option<String> = None;

    // Boucle de passes sur la MÊME connexion (prefill puis chaque decode).
    // Chaque passe = un transfert d'activations entrant + un token renvoyé.
    // La fin de requête est signalée par la fermeture de la connexion amont.
    loop {
        let (header, raw_activations) = match ActivationTransfer::receive(&session).await {
            Ok(v) => v,
            Err(_) => break, // l'amont (coordinateur/nœud précédent) a fermé
        };

        debug!(
            "Activations reçues: couches {}-{} seq={} hidden={}",
            header.layer_start, header.layer_end, header.seq_len, header.hidden_size
        );

        let request_id = String::from_utf8_lossy(&header.request_id).to_string();
        last_request_id = Some(request_id.clone());

        let output = run_pipeline_layers_real(
            &request_id, &header, &raw_activations, pipeline,
        ).await?;

        // Nombre de tokens attendus : 1 (normal) ou K+1 (spéculatif)
        let token_count = if header.speculative_k > 0 {
            header.speculative_k as usize + 1
        } else {
            1
        };

        if output.is_last_node {
            // Dernier nœud : envoyer token_count tokens en amont.
            for tok in output.tokens.iter().take(token_count) {
                send_single_token(&session, tok).await?;
            }
        } else {
            // Nœud intermédiaire : établir (une fois) la connexion vers le suivant,
            // transmettre les activations, récupérer token_count tokens et les relayer.
            if next_session.is_none() {
                let next_agent = offer.next_agent_id.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Pas de nœud suivant dans l'offre"))?;
                let next_offer = holochain.negotiate_quic_session(
                    next_agent, offer.next_layer_range, None, None,
                    Some(identity.public_key_bytes()),
                ).await.context("négociation nœud suivant")?;
                let ep = ainonymous_quic::create_endpoint(None, identity)
                    .await.context("endpoint QUIC client")?;
                let s = QuicSession::connect(
                    &ep, next_offer, ainonymous_quic::SessionConfig::default(), identity,
                ).await.context("connexion nœud suivant")?;
                next_endpoint = Some(ep);
                next_session = Some(s);
            }
            let ns = next_session.as_ref().unwrap();

            let out_header = ActivationHeader {
                request_id: header.request_id,
                layer_start: layer_range.1,
                layer_end: layer_range.1,
                seq_len: output.seq_len as u32,
                hidden_size: output.hidden_size as u32,
                dtype: header.dtype,
                compressed: false,
                speculative_k: header.speculative_k,  // propager
            };
            let out_activations = output.hidden_states_bytes
                .ok_or_else(|| anyhow::anyhow!("hidden_states manquants en sortie"))?;

            let toks = forward_and_receive_many_on(ns, &out_header, &out_activations, token_count).await?;
            for tok in &toks {
                send_single_token(&session, tok).await?;
            }
        }
    }

    // Fin de requête : purger le KV-cache local…
    if let Some(rid) = &last_request_id {
        let _ = pipeline.clear(rid).await;
        debug!("KV-cache purgé pour {}", rid);
    }
    // …et fermer la connexion aval (drop) → la purge se propage en cascade.
    drop(next_session);
    drop(next_endpoint);

    Ok(())
}

// ── Côté COORDINATEUR : initiation de l'inférence pipeline (chaîne) ───────────

/// Résultat d'une inférence distribuée vue du coordinateur.
pub struct CoordinatorResult {
    pub text: String,
    pub token_count: u32,
    pub node_ids: Vec<String>,
    /// Taux d'acceptation spéculatif (None si spéculatif désactivé).
    pub speculative_acceptance_rate: Option<f32>,
}

/// Construit un plan d'exécution PipelineSplit à partir de la config statique
/// (testnet sans Holochain). Les endpoints QUIC sont résolus via `peers`.
/// Retourne None si aucun `pipeline_stages` n'est défini ou si un endpoint
/// manque/est invalide.
pub fn static_plan_from_config(config: &DaemonConfig) -> Option<ExecutionPlan> {
    if config.pipeline_stages.is_empty() {
        return None;
    }
    let n = config.pipeline_stages.len();
    let mut stages = Vec::with_capacity(n);
    for (i, st) in config.pipeline_stages.iter().enumerate() {
        let peer = config.peers.iter().find(|p| p.agent_id == st.agent_id)?;
        let ep: SocketAddr = peer.quic_endpoint.as_ref()?.parse().ok()?;
        stages.push(ainonymous_types::PipelineStage {
            node: st.agent_id.clone(),
            quic_endpoint: ep,
            layer_start: st.layer_start,
            layer_end: st.layer_end,
            is_last: i == n - 1,
        });
    }
    Some(ExecutionPlan::PipelineSplit { stages })
}

/// Coordinateur : lance une inférence pipeline-split (topologie chaîne).
///
/// Optimisé : UNE seule session QUIC vers le 1er étage est négociée puis
/// réutilisée pour toutes les passes (prefill + chaque decode). En fin de
/// génération, la fermeture de la session purge le KV-cache de toute la chaîne.
///
/// 1. Tokenise le prompt via le pipeline_server (héberge embed+tokenizer).
/// 2. Ouvre la session vers le 1er étage (chaînage A→B indiqué).
/// 3. Boucle prefill → decode (un token par passe, request_id stable).
/// 4. Ferme la session → purge KV-cache en cascade.
pub async fn run_pipeline_inference(
    holochain: &HolochainClient,
    pipeline: &PipelineClient,
    plan: &ExecutionPlan,
    messages: serde_json::Value,
    max_tokens: u32,
    identity: &ainonymous_quic::NodeIdentity,
    eos_token_id: i32,
    speculative_k: u8,
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

    // 2. Ouvrir UNE session vers le 1er étage, réutilisée pour toutes les passes.
    // (L'endpoint client doit vivre aussi longtemps que la connexion.)
    let offer = holochain.negotiate_quic_session(
        &first.node,
        Some((first.layer_start, first.layer_end)),
        next.map(|s| s.node.clone()),
        next.map(|s| (s.layer_start, s.layer_end)),
        Some(identity.public_key_bytes()),
    ).await.context("négociation session 1er étage")?;
    let _endpoint = ainonymous_quic::create_endpoint(None, identity)
        .await.context("endpoint QUIC coordinateur")?;
    let session = QuicSession::connect(
        &_endpoint, offer, ainonymous_quic::SessionConfig::default(), identity,
    ).await.context("connexion QUIC 1er étage")?;

    let budget = if max_tokens == 0 { 512 } else { max_tokens };
    let mut out_ids: Vec<i32> = Vec::new();
    let mut text = String::new();

    // 3. Passe de prefill (prompt complet), puis boucle de decode
    let mut tok = send_pass_and_recv(&session, &request_id, first.layer_end, &token_ids)
        .await.context("passe prefill")?;

    let mut spec_proposed: u32 = 0;
    let mut spec_accepted: u32 = 0;

    loop {
        out_ids.push(tok.token_id as i32);
        text.push_str(&tok.text);

        let is_eos = tok.token_id as i32 == eos_token_id || tok.finish_reason.is_some();
        if is_eos || out_ids.len() as u32 >= budget {
            break;
        }

        if speculative_k > 0 && out_ids.len() as u32 + speculative_k as u32 <= budget {
            // ── Décodage spéculatif ──────────────────────────────────────────
            // Draft : n-gram sur les tokens générés (ou répétition du dernier token)
            let draft = make_ngram_draft(&out_ids, speculative_k as usize);
            spec_proposed += draft.len() as u32;

            let mut input = vec![tok.token_id as i32];
            input.extend_from_slice(&draft);

            let verified = send_speculative_and_recv_many(
                &session, &request_id, first.layer_end, &input, speculative_k,
            ).await.context("passe spéculative")?;

            // Acceptation : prefix matching (greedy speculative decoding)
            let mut stop = false;
            for (i, v) in verified.iter().enumerate() {
                if i >= draft.len() {
                    // Token bonus (toute la séquence de brouillon acceptée)
                    out_ids.push(v.token_id as i32);
                    text.push_str(&v.text);
                    if v.token_id as i32 == eos_token_id { stop = true; }
                    break;
                }
                let d = draft[i];
                // Le token vérifié à la position i est la prédiction pour draft[i]
                if d == v.token_id as i32 {
                    // Draft correct — accepté
                    spec_accepted += 1;
                    out_ids.push(d);
                    // Le texte sera détokenisé en fin de génération
                    if d == eos_token_id { stop = true; break; }
                } else {
                    // Rejet — on utilise le token vérifié à la place
                    out_ids.push(v.token_id as i32);
                    text.push_str(&v.text);
                    if v.token_id as i32 == eos_token_id { stop = true; }
                    break;
                }
                if out_ids.len() as u32 >= budget { stop = true; break; }
            }

            // Dernier token accepté connu (pour la prochaine itération)
            tok = match out_ids.last() {
                Some(&id) => GeneratedToken {
                    token_id: id as u32,
                    text: String::new(),
                    logprob: None,
                    finish_reason: if id == eos_token_id { Some(FinishReason::Stop) } else { None },
                },
                None => break,
            };
            if stop || out_ids.len() as u32 >= budget { break; }
        } else {
            // ── Décodage classique (1 token / passe) ─────────────────────────
            let last = tok.token_id as i32;
            tok = send_pass_and_recv(&session, &request_id, first.layer_end, &[last])
                .await.context("passe decode")?;
        }
    }

    // Détokeniser les tokens dont le texte n'a pas été fourni (spéculatif)
    if text.len() < out_ids.len() {
        text = pipeline.detokenize(&out_ids).await.unwrap_or(text);
    } else if text.is_empty() && !out_ids.is_empty() {
        text = pipeline.detokenize(&out_ids).await.unwrap_or_default();
    }

    // 4. Fermer la session → purge KV-cache de toute la chaîne (cascade).
    session.close();

    let speculative_acceptance_rate = if spec_proposed > 0 {
        Some(spec_accepted as f32 / spec_proposed as f32)
    } else {
        None
    };

    info!(
        "Coordinateur : {} tokens{}", out_ids.len(),
        speculative_acceptance_rate
            .map(|r| format!(" | spéculatif acceptance={:.0}%", r * 100.0))
            .unwrap_or_default()
    );
    Ok(CoordinatorResult {
        text,
        token_count: out_ids.len() as u32,
        node_ids: stages.iter().map(|s| s.node.clone()).collect(),
        speculative_acceptance_rate,
    })
}

/// Passe spéculative : envoie K+1 tokens (last + K brouillons) via QUIC,
/// reçoit K+1 tokens vérifiés depuis le bout de chaîne.
async fn send_speculative_and_recv_many(
    session: &QuicSession,
    request_id: &str,
    layer_end: u32,
    input_ids: &[i32],   // [last_tok, draft_1, …, draft_K]
    speculative_k: u8,
) -> Result<Vec<GeneratedToken>> {
    let mut rid = [0u8; 36];
    let rb = request_id.as_bytes();
    rid[..rb.len().min(36)].copy_from_slice(&rb[..rb.len().min(36)]);

    let header = ActivationHeader {
        request_id: rid,
        layer_start: 0,
        layer_end,
        seq_len: input_ids.len() as u32,
        hidden_size: 0,
        dtype: DType::F16,
        compressed: false,
        speculative_k,
    };
    ActivationTransfer::send(session, header, &token_ids_to_bytes(input_ids))
        .await
        .map_err(|e| anyhow::anyhow!("envoi spéculatif: {}", e))?;

    let mut ts = TokenStream::receiver(session)
        .await
        .map_err(|e| anyhow::anyhow!("ouverture token stream (spéculatif): {}", e))?;

    let mut tokens = Vec::with_capacity(speculative_k as usize + 1);
    for _ in 0..=speculative_k {
        match ts.recv_token().await.map_err(|e| anyhow::anyhow!("réception tok spéculatif: {}", e))? {
            Some(tok) => tokens.push(tok),
            None => break,
        }
    }
    Ok(tokens)
}

/// Draft n-gram : prédit les K prochains tokens depuis l'historique de génération.
/// Stratégie : bigram basé sur les dernières occurrences dans `history`.
/// Repli : répétition du dernier token si aucun bigram connu.
fn make_ngram_draft(history: &[i32], k: usize) -> Vec<i32> {
    if history.is_empty() || k == 0 {
        return vec![];
    }
    let mut draft = Vec::with_capacity(k);
    let mut last = *history.last().unwrap();

    for _ in 0..k {
        // Chercher le successeur le plus récent de `last` dans l'historique
        let successor = history
            .windows(2)
            .rev()
            .find(|w| w[0] == last)
            .map(|w| w[1])
            .unwrap_or(last); // repli : répéter
        draft.push(successor);
        last = successor;
    }
    draft
}

/// Une passe (prefill ou decode) sur une session DÉJÀ établie : envoie les
/// token_ids, reçoit UN token relayé en bout de chaîne. Le KV-cache persiste
/// côté pipeline_server entre les passes (même request_id).
async fn send_pass_and_recv(
    session: &QuicSession,
    request_id: &str,
    layer_end: u32,
    input_ids: &[i32],
) -> Result<GeneratedToken> {
    let mut rid = [0u8; 36];
    let rb = request_id.as_bytes();
    let n = rb.len().min(36);
    rid[..n].copy_from_slice(&rb[..n]);

    let header = ActivationHeader {
        request_id: rid,
        layer_start: 0,
        layer_end,
        seq_len: input_ids.len() as u32,
        hidden_size: 0,
        dtype: DType::F16,
        compressed: false,
        speculative_k: 0,
    };
    ActivationTransfer::send(session, header, &token_ids_to_bytes(input_ids))
        .await
        .map_err(|e| anyhow::anyhow!("envoi token_ids: {}", e))?;

    let mut ts = TokenStream::receiver(session)
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
    /// Tokens produits par ce nœud (vide si pas le dernier nœud).
    /// 1 token en décodage normal ; K+1 tokens en passe spéculative.
    tokens: Vec<GeneratedToken>,
}

/// Appel au pipeline_server.py pour exécuter la tranche de couches.
/// Gère prefill (seq_len > 1 ET pas de speculative_k), decode (seq_len = 1),
/// et passe spéculative (seq_len = K+1, speculative_k = K).
async fn run_pipeline_layers_real(
    request_id: &str,
    header: &ActivationHeader,
    raw_activations: &[u8],
    pipeline: &PipelineClient,
) -> Result<LayerOutput> {
    let is_speculative = header.speculative_k > 0;
    // Prefill = prompt complet SANS flag spéculatif (seq_len > 1, speculative_k = 0)
    let is_prefill = header.seq_len > 1 && !is_speculative;

    if is_prefill && header.layer_start == 0 {
        // Premier nœud : raw_activations = token_ids encodés u32 LE
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
        // Nœud intermédiaire / dernier : hidden states entrants
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
        // Phase decode (1 token normal ou K+1 tokens spéculatifs)
        let is_first_decode_node = header.layer_start == 0;
        let req = if is_first_decode_node {
            // Premier nœud : input_ids (1 token normal, ou K+1 tokens spéculatifs)
            let token_ids = parse_token_ids_from_activations(raw_activations);
            DecodeRequest {
                request_id: request_id.to_string(),
                input_ids: Some(token_ids),
                hidden_states_b64: None,
                seq_len: header.seq_len as usize,
                hidden_size: 0,
            }
        } else {
            // Nœud suivant : hidden states (seq_len positions)
            let b64 = crate::pipeline_client::bytes_to_b64(raw_activations);
            DecodeRequest {
                request_id: request_id.to_string(),
                input_ids: None,
                hidden_states_b64: Some(b64),
                seq_len: header.seq_len as usize,
                hidden_size: header.hidden_size as usize,
            }
        };
        let resp = pipeline.decode(&req).await.context("pipeline_server /decode")?;

        // Construire la liste de tokens pour le nœud final
        let tokens = if resp.is_last_node {
            build_token_vec(&resp)
        } else {
            Vec::new()
        };

        Ok(LayerOutput {
            hidden_states_bytes: resp.hidden_states_b64
                .as_deref()
                .map(crate::pipeline_client::b64_to_bytes)
                .transpose()?,
            seq_len: resp.seq_len,
            hidden_size: resp.hidden_size,
            is_last_node: resp.is_last_node,
            tokens,
        })
    }
}

fn build_layer_output_from_prefill(
    resp: crate::pipeline_client::PrefillResponse
) -> LayerOutput {
    let tokens = if resp.is_last_node {
        if let Some(id) = resp.next_token_id {
            vec![GeneratedToken {
                token_id: id as u32,
                text: resp.next_token_text.clone().unwrap_or_default(),
                logprob: None,
                finish_reason: None,
            }]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    LayerOutput {
        hidden_states_bytes: resp.hidden_states_b64
            .as_deref()
            .and_then(|b64| crate::pipeline_client::b64_to_bytes(b64).ok()),
        seq_len: resp.seq_len,
        hidden_size: resp.hidden_size,
        is_last_node: resp.is_last_node,
        tokens,
    }
}

/// Construit un Vec<GeneratedToken> depuis une DecodeResponse du nœud final.
/// Prend les K+1 tokens de `next_token_ids` si disponible (spéculatif),
/// sinon le token unique de `next_token_id`.
fn build_token_vec(resp: &crate::pipeline_client::DecodeResponse) -> Vec<GeneratedToken> {
    if let Some(ids) = &resp.next_token_ids {
        // Passe spéculative : K+1 tokens
        ids.iter().map(|&id| GeneratedToken {
            token_id: id as u32,
            text: String::new(), // le texte n'est décodé que pour le token final (perf)
            logprob: None,
            finish_reason: None,
        }).collect()
    } else if let Some(id) = resp.next_token_id {
        vec![GeneratedToken {
            token_id: id as u32,
            text: resp.next_token_text.clone().unwrap_or_default(),
            logprob: None,
            finish_reason: None,
        }]
    } else {
        Vec::new()
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

/// Nœud intermédiaire : transmet les activations au nœud suivant et reçoit
/// `token_count` tokens produits en bout de chaîne (1 normal, K+1 spéculatif).
async fn forward_and_receive_many_on(
    next_session: &QuicSession,
    header: &ActivationHeader,
    activations: &[u8],
    token_count: usize,
) -> Result<Vec<GeneratedToken>> {
    ActivationTransfer::send(next_session, header.clone(), activations)
        .await
        .context("Envoi activations nœud suivant")?;

    debug!(
        "Activations transmises (couches [{}, {}[) — {} token(s) attendu(s)",
        header.layer_start, header.layer_end, token_count
    );

    let mut ts = TokenStream::receiver(next_session)
        .await
        .map_err(|e| anyhow::anyhow!("réception tokens du suivant: {}", e))?;

    let mut tokens = Vec::with_capacity(token_count);
    for _ in 0..token_count {
        match ts.recv_token().await.map_err(|e| anyhow::anyhow!("lecture tok suivant: {}", e))? {
            Some(tok) => tokens.push(tok),
            None => break,
        }
    }
    if tokens.is_empty() {
        anyhow::bail!("aucun token reçu du nœud suivant");
    }
    Ok(tokens)
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
