//! Spike S4 — deux nœuds QUIC (porte G4), sur la pile existante
//! `ainonymous-quic` : mTLS Ed25519 avec épinglage des deux clés, token de
//! session à usage unique, transfert d'activations `ActivationTransfer`.
//!
//! Le plan de contrôle (Holochain) est remplacé ici par un échange hors bande de
//! l'offre de session ([`SessionOffer`]) : en boucle locale dans les tests, par
//! fichier JSON entre deux machines (voir `README.md`).
//!
//! Protocole applicatif du spike, au-dessus d'une session authentifiée :
//! - flux bidirectionnel : écho (mesure de RTT) ;
//! - flux unidirectionnel coordinateur → worker : activations
//!   (`ActivationTransfer::send`) ; le worker répond par un flux
//!   unidirectionnel contenant `sha256 (32 o) || taille (u64 LE)` du contenu
//!   reçu, ce qui prouve l'intégrité et date la fin de réception.

use ainonymous_quic::{
    ActivationTransfer, NodeIdentity, QuicListener, QuicSession, SessionConfig, SessionOffer,
    SessionRegistry,
};
use ainonymous_types::inference::{ActivationHeader, DType};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Événements observés côté worker (pour les tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    /// Session authentifiée (mTLS + token) et remise au handler.
    Authenticated {
        client_key: [u8; 32],
        layer_range: Option<(u32, u32)>,
    },
    /// Bloc d'activations reçu.
    Activations {
        bytes: u64,
        sha256: [u8; 32],
        layer_start: u32,
        layer_end: u32,
    },
}

pub struct Worker {
    pub addr: SocketAddr,
    pub public_key: [u8; 32],
    registry: SessionRegistry,
    pub events: mpsc::UnboundedReceiver<WorkerEvent>,
    _task: tokio::task::JoinHandle<()>,
}

impl Worker {
    /// Démarre un worker QUIC (listener mTLS) sur `bind`.
    pub async fn start(bind: SocketAddr, identity: NodeIdentity) -> Result<Self> {
        let listener = Arc::new(QuicListener::new(bind, &identity).await?);
        let addr = listener.local_addr()?;
        let registry = listener.registry();
        let (tx, events) = mpsc::unbounded_channel();
        let l = listener.clone();
        let task = tokio::spawn(async move {
            l.run(move |conn, offer| {
                let tx = tx.clone();
                async move { serve(conn, offer, tx).await }
            })
            .await;
        });
        Ok(Self {
            addr,
            public_key: identity.public_key_bytes(),
            registry,
            events,
            _task: task,
        })
    }

    /// Plan de contrôle simulé : crée et enregistre une offre pour un client
    /// dont la clé publique est connue. Le coordinateur reçoit l'offre.
    pub fn offer_for(
        &self,
        client_key: [u8; 32],
        layer_range: Option<(u32, u32)>,
        advertised: SocketAddr,
    ) -> SessionOffer {
        let mut offer = SessionOffer::new(advertised, layer_range);
        offer.peer_pubkey = Some(self.public_key);
        offer.client_pubkey = Some(client_key);
        self.registry.register(offer.clone());
        offer
    }
}

async fn serve(
    conn: quinn::Connection,
    offer: SessionOffer,
    tx: mpsc::UnboundedSender<WorkerEvent>,
) {
    let _ = tx.send(WorkerEvent::Authenticated {
        client_key: offer.client_pubkey.unwrap_or_default(),
        layer_range: offer.layer_range,
    });
    // Écho sur les flux bidirectionnels.
    let c2 = conn.clone();
    tokio::spawn(async move {
        while let Ok((mut send, mut recv)) = c2.accept_bi().await {
            tokio::spawn(async move {
                if let Ok(buf) = recv.read_to_end(1 << 20).await {
                    let _ = send.write_all(&buf).await;
                    let _ = send.finish();
                }
            });
        }
    });
    // Activations sur les flux unidirectionnels.
    let session = QuicSession {
        connection: conn.clone(),
        offer,
        config: SessionConfig::default(),
        established_at: Instant::now(),
    };
    while let Ok((header, data)) = ActivationTransfer::receive(&session).await {
        let sha: [u8; 32] = Sha256::digest(&data).into();
        let _ = tx.send(WorkerEvent::Activations {
            bytes: data.len() as u64,
            sha256: sha,
            layer_start: header.layer_start,
            layer_end: header.layer_end,
        });
        if let Ok(mut s) = conn.open_uni().await {
            let mut ack = sha.to_vec();
            ack.extend_from_slice(&(data.len() as u64).to_le_bytes());
            let _ = s.write_all(&ack).await;
            let _ = s.finish();
        }
    }
}

/// Connexion coordinateur → worker à partir d'une offre.
pub async fn connect(
    identity: &NodeIdentity,
    offer: SessionOffer,
) -> Result<(quinn::Endpoint, QuicSession)> {
    let bind: SocketAddr = match offer.quic_endpoint {
        Some(SocketAddr::V6(_)) => "[::]:0".parse()?,
        _ => "0.0.0.0:0".parse()?,
    };
    let ep = ainonymous_quic::create_endpoint(Some(bind), identity).await?;
    let s = QuicSession::connect(&ep, offer, SessionConfig::default(), identity).await?;
    Ok((ep, s))
}

/// Un aller-retour d'écho de `payload` octets ; renvoie la durée.
pub async fn echo_once(session: &QuicSession, payload: usize) -> Result<Duration> {
    let data = vec![0x5Au8; payload];
    let t = Instant::now();
    let (mut send, mut recv) = session.connection.open_bi().await?;
    send.write_all(&data).await?;
    send.finish()?;
    let back = recv.read_to_end(1 << 20).await?;
    let d = t.elapsed();
    if back != data {
        bail!("écho corrompu");
    }
    Ok(d)
}

/// Envoie un bloc d'activations float32 pseudo-aléatoires et attend l'accusé
/// de réception du worker. Renvoie la durée bout en bout.
pub async fn send_activations(
    session: &QuicSession,
    bytes: usize,
    layer_start: u32,
    layer_end: u32,
) -> Result<Duration> {
    use rand::RngCore;
    let mut data = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut data);
    let expected: [u8; 32] = Sha256::digest(&data).into();
    let mut rid = [b'0'; 36];
    rid[..8].copy_from_slice(b"spike-s4");
    let header = ActivationHeader {
        request_id: rid,
        layer_start,
        layer_end,
        seq_len: 1,
        hidden_size: (bytes / 4) as u32,
        dtype: DType::F32,
        compressed: false,
        speculative_k: 0,
    };
    let t = Instant::now();
    ActivationTransfer::send(session, header, &data).await?;
    let mut ack = session
        .connection
        .accept_uni()
        .await
        .context("accusé de réception")?;
    let buf = ack.read_to_end(64).await?;
    let d = t.elapsed();
    if buf.len() != 40
        || buf[..32] != expected
        || u64::from_le_bytes(buf[32..40].try_into()?) != bytes as u64
    {
        bail!("accusé de réception incohérent (intégrité)");
    }
    Ok(d)
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub mode: String,
    pub peer: String,
    pub session_setup_ms: f64,
    pub rtt_samples: usize,
    pub rtt_p50_ms: f64,
    pub rtt_p95_ms: f64,
    pub transfer_bytes: u64,
    pub transfer_ms: f64,
    pub throughput_mbit_s: f64,
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i]
}

/// Mesures §6.3 du plan : setup de session, RTT, débit (transfert de `bytes`).
pub async fn measure(
    identity: &NodeIdentity,
    offer: SessionOffer,
    mode: &str,
    rtt_n: usize,
    bytes: usize,
) -> Result<Report> {
    let peer = offer
        .quic_endpoint
        .map(|a| a.to_string())
        .unwrap_or_default();
    let t = Instant::now();
    let (_ep, session) = connect(identity, offer).await?;
    // Le serveur ne valide le token qu'après le handshake : la session n'est
    // considérée établie qu'au premier écho (confirmé côté worker).
    echo_once(&session, 64).await?;
    let setup = t.elapsed();
    let mut rtts = Vec::with_capacity(rtt_n);
    for _ in 0..rtt_n {
        rtts.push(echo_once(&session, 64).await?.as_secs_f64() * 1e3);
    }
    rtts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let d = send_activations(&session, bytes, 0, 1).await?;
    let report = Report {
        mode: mode.into(),
        peer,
        session_setup_ms: setup.as_secs_f64() * 1e3,
        rtt_samples: rtt_n,
        rtt_p50_ms: pct(&rtts, 0.5),
        rtt_p95_ms: pct(&rtts, 0.95),
        transfer_bytes: bytes as u64,
        transfer_ms: d.as_secs_f64() * 1e3,
        throughput_mbit_s: bytes as f64 * 8.0 / d.as_secs_f64() / 1e6,
    };
    session.close();
    Ok(report)
}
