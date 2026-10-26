//! CLI du spike S5 : `keygen`, `manifest`, `sign`, `verify`, `install`, `rollback`, `status`.
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ed25519_dalek::SigningKey;
use relay_release::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(
    name = "relay-release",
    about = "Spike S5 — manifeste de release signé AInonymous Relay"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args, Clone)]
struct TargetArgs {
    #[arg(long, default_value = "ainonymous-relay")]
    product: String,
    #[arg(long, default_value = "stable")]
    channel: String,
    /// ex. windows-x86_64, android-arm64
    #[arg(long)]
    platform: String,
}

impl From<TargetArgs> for Target {
    fn from(t: TargetArgs) -> Self {
        Target {
            product: t.product,
            channel: t.channel,
            platform: t.platform,
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Génère une clé d'éditeur de TEST (seed hex 32 octets) et affiche la clé publique.
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    /// Construit un manifeste non signé à partir d'un artefact local.
    Manifest {
        #[command(flatten)]
        target: TargetArgs,
        #[arg(long)]
        version: String,
        #[arg(long)]
        sequence: u64,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        url: String,
        /// Durée de validité en jours.
        #[arg(long, default_value_t = 30)]
        valid_days: u64,
        #[arg(long)]
        out: PathBuf,
    },
    /// Signe un manifeste.
    Sign {
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Vérifie un manifeste signé (et optionnellement un artefact).
    Verify {
        #[command(flatten)]
        target: TargetArgs,
        /// Clé publique d'éditeur autorisée (hex) ; répétable.
        #[arg(long = "trust", required = true)]
        trust: Vec<String>,
        #[arg(long)]
        signed: PathBuf,
        #[arg(long)]
        artifact: Option<PathBuf>,
    },
    /// Vérifie et installe un artefact (installeur de démonstration).
    Install {
        #[command(flatten)]
        target: TargetArgs,
        #[arg(long)]
        root: PathBuf,
        #[arg(long = "trust", required = true)]
        trust: Vec<String>,
        #[arg(long)]
        signed: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
    },
    /// Revient à la version précédente.
    Rollback {
        #[command(flatten)]
        target: TargetArgs,
        #[arg(long)]
        root: PathBuf,
    },
    /// Affiche l'état de l'installeur.
    Status {
        #[command(flatten)]
        target: TargetArgs,
        #[arg(long)]
        root: PathBuf,
    },
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn load_key(p: &Path) -> Result<SigningKey> {
    let s = std::fs::read_to_string(p)?;
    let b: [u8; 32] = hex::decode(s.trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("seed: 32 octets attendus"))?;
    Ok(SigningKey::from_bytes(&b))
}

fn trust_store(keys: &[String]) -> Result<TrustStore> {
    let mut t = TrustStore::new();
    for k in keys {
        t.add_hex(k)?;
    }
    Ok(t)
}

fn read_signed(p: &Path) -> Result<SignedManifest> {
    serde_json::from_slice(&std::fs::read(p)?).context("manifeste signé illisible")
}

fn file_name(p: &Path) -> Result<String> {
    Ok(p.file_name()
        .context("nom de fichier")?
        .to_string_lossy()
        .into_owned())
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { out } => {
            let k = generate_key();
            std::fs::write(&out, hex::encode(k.to_bytes()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o600))?;
            }
            println!("{}", hex::encode(k.verifying_key().to_bytes()));
        }
        Cmd::Manifest {
            target,
            version,
            sequence,
            artifact,
            kind,
            url,
            valid_days,
            out,
        } => {
            let (sha256, size) = hash_file(&artifact)?;
            let issued = now();
            let m = ReleaseManifest {
                schema: MANIFEST_SCHEMA,
                target: target.into(),
                version,
                sequence,
                issued_at_unix: issued,
                expires_at_unix: issued + valid_days * 86_400,
                artifacts: vec![Artifact {
                    name: file_name(&artifact)?,
                    kind,
                    sha256,
                    size,
                    url,
                }],
            };
            std::fs::write(out, serde_json::to_vec_pretty(&m)?)?;
        }
        Cmd::Sign { key, manifest, out } => {
            let m: ReleaseManifest = serde_json::from_slice(&std::fs::read(manifest)?)?;
            let s = sign_manifest(m, &load_key(&key)?);
            std::fs::write(out, serde_json::to_vec_pretty(&s)?)?;
        }
        Cmd::Verify {
            target,
            trust,
            signed,
            artifact,
        } => {
            let s = read_signed(&signed)?;
            verify_manifest(
                &s,
                &trust_store(&trust)?,
                &target.into(),
                &ClientState::default(),
                now(),
            )?;
            if let Some(a) = artifact {
                verify_artifact(&a, s.manifest.artifact(&file_name(&a)?)?)?;
            }
            println!("OK {} séquence {}", s.manifest.version, s.manifest.sequence);
        }
        Cmd::Install {
            target,
            root,
            trust,
            signed,
            artifact,
        } => {
            let inst = Installer::open(root, target.into())?;
            let s = read_signed(&signed)?;
            let rec = inst.install(
                &s,
                &file_name(&artifact)?,
                &artifact,
                &trust_store(&trust)?,
                now(),
            )?;
            println!("installé {}", rec.version);
        }
        Cmd::Rollback { target, root } => {
            let rec = Installer::open(root, target.into())?.rollback()?;
            println!("rollback vers {}", rec.version);
        }
        Cmd::Status { target, root } => {
            let st = Installer::open(root, target.into())?.state()?;
            println!("{}", serde_json::to_string_pretty(&st)?);
        }
    }
    Ok(())
}
