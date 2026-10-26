//! Spike S5 — distribution directe signée (décision D8), porte G5.
//!
//! Ce crate prouve, sans dépendre d'un OS particulier, la chaîne de confiance
//! applicative d'une mise à jour AInonymous Relay :
//!
//! 1. un **manifeste de release** décrit une version, une plateforme et les
//!    artefacts (installeur Windows, APK) avec leur SHA-256 et leur taille ;
//! 2. le manifeste est **signé Ed25519** par une clé d'éditeur, sur une forme
//!    canonique (JSON à clés triées) préfixée par un contexte de domaine ;
//! 3. le client vérifie : clé autorisée (sinon *non autorisé*), signature,
//!    schéma, produit/plateforme, fenêtre de validité, **numéro de séquence
//!    strictement croissant** (anti-rejeu / anti-gel) et **absence de
//!    rétrogradation** de version ;
//! 4. l'artefact téléchargé est refusé si son **hash** ou sa **taille** ne
//!    correspondent pas ;
//! 5. l'installation est atomique (pointeur `current` réécrit par renommage) et
//!    conserve la version précédente : un **rollback** local est possible,
//!    explicite ou automatique après un échec de contrôle de santé, après
//!    re-vérification de l'intégrité de la version restaurée.
//!
//! Hors périmètre du spike (nécessite Windows/Android) : la signature
//! Authenticode (`signtool verify`) et la signature APK (`apksigner verify`),
//! qui s'ajoutent à cette vérification applicative et ne la remplacent pas.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Contexte de domaine préfixé au message signé : une signature de manifeste
/// ne peut pas être réutilisée pour un autre type d'objet signé par la même clé.
pub const SIGNING_CONTEXT: &[u8] = b"ainonymous-relay/release-manifest/v1\0";
/// Version de schéma acceptée.
pub const MANIFEST_SCHEMA: u32 = 1;
/// Tolérance d'horloge pour `issued_at_unix` (secondes).
pub const CLOCK_SKEW_SECS: u64 = 300;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReleaseError {
    #[error("clé d'éditeur non autorisée: {0}")]
    UnauthorizedKey(String),
    #[error("signature du manifeste invalide")]
    BadSignature,
    #[error("format invalide: {0}")]
    Malformed(String),
    #[error("schéma de manifeste non supporté: {0}")]
    UnsupportedSchema(u32),
    #[error("produit/canal/plateforme inattendu: {0}")]
    TargetMismatch(String),
    #[error("manifeste expiré")]
    Expired,
    #[error("manifeste émis dans le futur")]
    NotYetValid,
    #[error("rejeu refusé: séquence {got} <= dernière séquence acceptée {last}")]
    Replay { got: u64, last: u64 },
    #[error("rétrogradation refusée: {got} < version installée {installed}")]
    Downgrade { got: String, installed: String },
    #[error("artefact absent du manifeste: {0}")]
    UnknownArtifact(String),
    #[error("taille d'artefact incorrecte: attendu {expected}, obtenu {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("hash d'artefact incorrect: attendu {expected}, obtenu {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("aucune version précédente pour le rollback")]
    NoPreviousVersion,
    #[error("contrôle de santé échoué pour {0}, rollback effectué")]
    HealthCheckFailed(String),
    #[error("E/S: {0}")]
    Io(String),
}

impl From<std::io::Error> for ReleaseError {
    fn from(e: std::io::Error) -> Self {
        ReleaseError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ReleaseError>;

/// Artefact distribuable (installeur per-user Windows, APK Android…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Nom de fichier, sans séparateur de chemin.
    pub name: String,
    /// `nsis`, `msi`, `apk`, `tar` …
    pub kind: String,
    /// SHA-256 hexadécimal minuscule.
    pub sha256: String,
    pub size: u64,
    pub url: String,
}

/// Cible d'une mise à jour : un client n'accepte que sa propre cible.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub product: String,
    pub channel: String,
    pub platform: String,
}

impl Target {
    fn key(&self) -> String {
        format!("{}/{}/{}", self.product, self.channel, self.platform)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema: u32,
    pub target: Target,
    /// Version `MAJEUR.MINEUR.CORRECTIF`.
    pub version: String,
    /// Compteur monotone par cible, incrémenté à chaque manifeste publié.
    pub sequence: u64,
    pub issued_at_unix: u64,
    pub expires_at_unix: u64,
    pub artifacts: Vec<Artifact>,
}

impl ReleaseManifest {
    /// Forme canonique signée : JSON compact, clés d'objets triées
    /// (`serde_json::Map` est un `BTreeMap` sans la feature `preserve_order`).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let value = serde_json::to_value(self).expect("manifeste sérialisable");
        serde_json::to_vec(&value).expect("valeur sérialisable")
    }

    fn signing_message(&self) -> Vec<u8> {
        let mut m = SIGNING_CONTEXT.to_vec();
        m.extend_from_slice(&self.canonical_bytes());
        m
    }

    pub fn artifact(&self, name: &str) -> Result<&Artifact> {
        self.artifacts
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| ReleaseError::UnknownArtifact(name.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    pub manifest: ReleaseManifest,
    /// Clé publique Ed25519 de l'éditeur, en hexadécimal.
    pub key_id: String,
    /// Signature Ed25519 en hexadécimal.
    pub signature: String,
}

pub fn sign_manifest(manifest: ReleaseManifest, key: &SigningKey) -> SignedManifest {
    let sig = key.sign(&manifest.signing_message());
    SignedManifest {
        key_id: hex::encode(key.verifying_key().to_bytes()),
        signature: hex::encode(sig.to_bytes()),
        manifest,
    }
}

/// Ensemble des clés d'éditeur autorisées (épinglées dans le client).
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    keys: BTreeMap<String, VerifyingKey>,
}

impl TrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_hex(&mut self, pubkey_hex: &str) -> Result<()> {
        let bytes: [u8; 32] = hex::decode(pubkey_hex)
            .map_err(|e| ReleaseError::Malformed(format!("clé: {e}")))?
            .try_into()
            .map_err(|_| ReleaseError::Malformed("clé: 32 octets attendus".into()))?;
        let vk = VerifyingKey::from_bytes(&bytes)
            .map_err(|e| ReleaseError::Malformed(format!("clé: {e}")))?;
        self.keys.insert(hex::encode(bytes), vk);
        Ok(())
    }

    pub fn add(&mut self, vk: VerifyingKey) {
        self.keys.insert(hex::encode(vk.to_bytes()), vk);
    }
}

/// État persistant côté client, nécessaire à l'anti-rejeu.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Dernière séquence acceptée par cible.
    pub last_sequence: BTreeMap<String, u64>,
    /// Version actuellement installée par cible.
    pub installed_version: BTreeMap<String, String>,
}

fn parse_version(v: &str) -> Result<(u64, u64, u64)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return Err(ReleaseError::Malformed(format!("version {v}")));
    }
    let p = |s: &str| {
        s.parse::<u64>()
            .map_err(|_| ReleaseError::Malformed(format!("version {v}")))
    };
    Ok((p(parts[0])?, p(parts[1])?, p(parts[2])?))
}

fn is_hex_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Vérifie un manifeste signé pour une cible donnée.
///
/// N'altère pas `state` : l'appelant n'enregistre la séquence qu'après une
/// installation réussie (voir [`Installer::install`]).
pub fn verify_manifest(
    signed: &SignedManifest,
    trust: &TrustStore,
    expected: &Target,
    state: &ClientState,
    now_unix: u64,
) -> Result<()> {
    // 1. Clé autorisée ?
    let vk = trust
        .keys
        .get(&signed.key_id.to_ascii_lowercase())
        .ok_or_else(|| ReleaseError::UnauthorizedKey(signed.key_id.clone()))?;

    // 2. Signature (vérification stricte : refuse les clés faibles / malléabilité)
    let sig_bytes: [u8; 64] = hex::decode(&signed.signature)
        .map_err(|_| ReleaseError::BadSignature)?
        .try_into()
        .map_err(|_| ReleaseError::BadSignature)?;
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify_strict(&signed.manifest.signing_message(), &sig)
        .map_err(|_| ReleaseError::BadSignature)?;

    let m = &signed.manifest;

    // 3. Schéma et contenu
    if m.schema != MANIFEST_SCHEMA {
        return Err(ReleaseError::UnsupportedSchema(m.schema));
    }
    if &m.target != expected {
        return Err(ReleaseError::TargetMismatch(m.target.key()));
    }
    parse_version(&m.version)?;
    if m.artifacts.is_empty() {
        return Err(ReleaseError::Malformed("aucun artefact".into()));
    }
    let mut names = BTreeSet::new();
    for a in &m.artifacts {
        if a.name.is_empty()
            || a.name.contains('/')
            || a.name.contains('\\')
            || a.name == "."
            || a.name == ".."
        {
            return Err(ReleaseError::Malformed(format!(
                "nom d'artefact {}",
                a.name
            )));
        }
        if !is_hex_sha256(&a.sha256) {
            return Err(ReleaseError::Malformed(format!("sha256 de {}", a.name)));
        }
        if !names.insert(a.name.clone()) {
            return Err(ReleaseError::Malformed(format!(
                "artefact dupliqué {}",
                a.name
            )));
        }
    }

    // 4. Fenêtre de validité
    if m.issued_at_unix > now_unix + CLOCK_SKEW_SECS {
        return Err(ReleaseError::NotYetValid);
    }
    if now_unix >= m.expires_at_unix {
        return Err(ReleaseError::Expired);
    }

    // 5. Anti-rejeu : séquence strictement croissante
    let key = m.target.key();
    if let Some(&last) = state.last_sequence.get(&key) {
        if m.sequence <= last {
            return Err(ReleaseError::Replay {
                got: m.sequence,
                last,
            });
        }
    }

    // 6. Pas de rétrogradation par manifeste (le rollback est une opération locale)
    if let Some(installed) = state.installed_version.get(&key) {
        if parse_version(&m.version)? < parse_version(installed)? {
            return Err(ReleaseError::Downgrade {
                got: m.version.clone(),
                installed: installed.clone(),
            });
        }
    }
    Ok(())
}

/// SHA-256 et taille d'un fichier, en flux.
pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((hex::encode(h.finalize()), size))
}

/// Vérifie un fichier téléchargé contre son entrée de manifeste.
pub fn verify_artifact(path: &Path, expected: &Artifact) -> Result<()> {
    let (sha, size) = hash_file(path)?;
    if size != expected.size {
        return Err(ReleaseError::SizeMismatch {
            expected: expected.size,
            actual: size,
        });
    }
    if sha != expected.sha256 {
        return Err(ReleaseError::HashMismatch {
            expected: expected.sha256.clone(),
            actual: sha,
        });
    }
    Ok(())
}

/// Enregistrement local d'une version installée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledRecord {
    pub version: String,
    pub sequence: u64,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallerState {
    pub client: ClientState,
    pub current: Option<InstalledRecord>,
    pub previous: Option<InstalledRecord>,
    /// Versions retirées après un échec de contrôle de santé.
    pub quarantined: Vec<String>,
}

/// Installeur de démonstration, indépendant de l'OS : il range les artefacts
/// vérifiés dans `root/versions/<version>/` et maintient l'état dans
/// `root/state.json` (écrit de façon atomique par renommage).
pub struct Installer {
    root: PathBuf,
    target: Target,
}

impl Installer {
    pub fn open(root: impl Into<PathBuf>, target: Target) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("versions"))?;
        Ok(Self { root, target })
    }

    pub fn state(&self) -> Result<InstallerState> {
        let p = self.root.join("state.json");
        if !p.exists() {
            return Ok(InstallerState::default());
        }
        serde_json::from_slice(&fs::read(p)?)
            .map_err(|e| ReleaseError::Malformed(format!("state.json: {e}")))
    }

    fn save_state(&self, st: &InstallerState) -> Result<()> {
        let tmp = self.root.join("state.json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(st).expect("état sérialisable"))?;
            f.sync_all()?;
        }
        fs::rename(tmp, self.root.join("state.json"))?;
        Ok(())
    }

    pub fn artifact_path(&self, rec: &InstalledRecord) -> PathBuf {
        self.root
            .join("versions")
            .join(&rec.version)
            .join(&rec.artifact.name)
    }

    /// Vérifie le manifeste et l'artefact, copie l'artefact, bascule `current`
    /// et conserve l'ancienne version comme `previous`. Rien n'est modifié si
    /// une vérification échoue.
    pub fn install(
        &self,
        signed: &SignedManifest,
        artifact_name: &str,
        downloaded: &Path,
        trust: &TrustStore,
        now_unix: u64,
    ) -> Result<InstalledRecord> {
        let mut st = self.state()?;
        verify_manifest(signed, trust, &self.target, &st.client, now_unix)?;
        let art = signed.manifest.artifact(artifact_name)?.clone();
        verify_artifact(downloaded, &art)?;

        // Copier dans un répertoire temporaire, re-vérifier, puis renommer.
        let version = signed.manifest.version.clone();
        let staging = self
            .root
            .join("versions")
            .join(format!(".staging-{version}"));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        let staged = staging.join(&art.name);
        fs::copy(downloaded, &staged)?;
        verify_artifact(&staged, &art)?;
        let final_dir = self.root.join("versions").join(&version);
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir)?;
        }
        fs::rename(&staging, &final_dir)?;

        let rec = InstalledRecord {
            version: version.clone(),
            sequence: signed.manifest.sequence,
            artifact: art,
        };
        let key = self.target.key();
        st.client
            .last_sequence
            .insert(key.clone(), signed.manifest.sequence);
        st.client.installed_version.insert(key, version);
        st.previous = st.current.take();
        st.current = Some(rec.clone());
        self.save_state(&st)?;
        Ok(rec)
    }

    /// Installe puis exécute un contrôle de santé ; en cas d'échec, revient
    /// automatiquement à la version précédente.
    pub fn install_with_health_check(
        &self,
        signed: &SignedManifest,
        artifact_name: &str,
        downloaded: &Path,
        trust: &TrustStore,
        now_unix: u64,
        health: impl Fn(&Path) -> bool,
    ) -> Result<InstalledRecord> {
        let rec = self.install(signed, artifact_name, downloaded, trust, now_unix)?;
        if health(&self.artifact_path(&rec)) {
            return Ok(rec);
        }
        self.rollback()?;
        let mut st = self.state()?;
        st.quarantined.push(rec.version.clone());
        self.save_state(&st)?;
        Err(ReleaseError::HealthCheckFailed(rec.version))
    }

    /// Restaure la version précédente après avoir re-vérifié son intégrité sur
    /// disque. La séquence acceptée n'est pas diminuée : un ancien manifeste
    /// reste refusé (le rollback est local, pas un rejeu).
    pub fn rollback(&self) -> Result<InstalledRecord> {
        let mut st = self.state()?;
        let prev = st.previous.clone().ok_or(ReleaseError::NoPreviousVersion)?;
        verify_artifact(&self.artifact_path(&prev), &prev.artifact)?;
        let key = self.target.key();
        st.client
            .installed_version
            .insert(key, prev.version.clone());
        st.current = Some(prev.clone());
        st.previous = None;
        self.save_state(&st)?;
        Ok(prev)
    }
}

/// Génère une clé d'éditeur (usage test/spike uniquement).
pub fn generate_key() -> SigningKey {
    SigningKey::generate(&mut rand::rngs::OsRng)
}
