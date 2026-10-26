//! CLI du spike S4. Voir README.md.
use ainonymous_quic::{NodeIdentity, SessionOffer};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use relay_quic_spike::{measure, Worker};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "quic-spike",
    about = "Spike S4 : mesures QUIC mTLS entre deux nœuds"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Affiche (et crée si besoin) la clé publique Ed25519 du nœud.
    Pubkey {
        #[arg(long)]
        key_file: PathBuf,
    },
    /// Deux nœuds dans le même processus, en boucle locale (127.0.0.1).
    Loopback {
        #[arg(long, default_value_t = 200)]
        rtt_n: usize,
        #[arg(long, default_value_t = 100 * 1024 * 1024)]
        bytes: usize,
    },
    /// Nœud worker : écoute et publie des offres de session (fichier JSON,
    /// régénéré toutes les 20 s car le token expire au bout de 30 s).
    Serve {
        #[arg(long, default_value = "0.0.0.0:7700")]
        bind: SocketAddr,
        /// Adresse joignable par le client (IP LAN ou Tailscale).
        #[arg(long)]
        advertise: SocketAddr,
        #[arg(long)]
        key_file: PathBuf,
        /// Clé publique (hex) du client autorisé.
        #[arg(long)]
        client_key: String,
        #[arg(long, default_value = "offer.json")]
        offer_out: PathBuf,
    },
    /// Nœud coordinateur : se connecte avec une offre et mesure.
    Client {
        #[arg(long)]
        offer: PathBuf,
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long, default_value = "lan")]
        mode: String,
        #[arg(long, default_value_t = 200)]
        rtt_n: usize,
        #[arg(long, default_value_t = 100 * 1024 * 1024)]
        bytes: usize,
    },
}

fn parse_key(hex_str: &str) -> Result<[u8; 32]> {
    let v = hex::decode(hex_str.trim()).context("clé hex invalide")?;
    v.try_into()
        .map_err(|_| anyhow::anyhow!("clé : 32 octets attendus"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match Cli::parse().cmd {
        Cmd::Pubkey { key_file } => {
            println!(
                "{}",
                NodeIdentity::load_or_generate(&key_file)?.public_key_hex()
            );
        }
        Cmd::Loopback { rtt_n, bytes } => {
            let worker_id = NodeIdentity::generate();
            let client_id = NodeIdentity::generate();
            let worker = Worker::start("127.0.0.1:0".parse()?, worker_id).await?;
            let offer = worker.offer_for(client_id.public_key_bytes(), Some((0, 1)), worker.addr);
            let r = measure(
                &client_id,
                offer,
                "loopback (127.0.0.1, non représentatif d'un LAN)",
                rtt_n,
                bytes,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        Cmd::Serve {
            bind,
            advertise,
            key_file,
            client_key,
            offer_out,
        } => {
            let id = NodeIdentity::load_or_generate(&key_file)?;
            eprintln!("worker pubkey = {}", id.public_key_hex());
            let client = parse_key(&client_key)?;
            let mut worker = Worker::start(bind, id).await?;
            eprintln!("écoute sur {} (annoncé {advertise})", worker.addr);
            loop {
                let offer = worker.offer_for(client, Some((0, 1)), advertise);
                let tmp = offer_out.with_extension("tmp");
                std::fs::write(&tmp, serde_json::to_vec_pretty(&offer)?)?;
                std::fs::rename(&tmp, &offer_out)?;
                let deadline = tokio::time::sleep(Duration::from_secs(20));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        ev = worker.events.recv() => match ev {
                            Some(e) => eprintln!("événement : {e:?}"),
                            None => return Ok(()),
                        },
                    }
                }
            }
        }
        Cmd::Client {
            offer,
            key_file,
            mode,
            rtt_n,
            bytes,
        } => {
            let id = NodeIdentity::load_or_generate(&key_file)?;
            let offer: SessionOffer = serde_json::from_slice(&std::fs::read(&offer)?)?;
            let r = measure(&id, offer, &mode, rtt_n, bytes).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
    }
    Ok(())
}
