//! mTLS ed25519 pour le data plane QUIC.
//!
//! Chaque nœud possède une identité ed25519 (= AgentPubKey). Le certificat TLS
//! est auto-signé par cette clé, donc la clé publique du certificat **est**
//! l'AgentPubKey. Vérification mutuelle :
//!   - le client vérifie que la clé publique du certificat serveur == la clé
//!     attendue du pair (fournie par le plan de contrôle, dans l'offre) ;
//!   - le serveur exige un certificat client ed25519 valide.
//!
//! La **preuve de possession** repose sur la vérification réelle de la signature
//! du handshake (déléguée au provider `ring`), pas sur un `assertion()` qui
//! l'aurait court-circuitée (faille de l'ancien `SkipVerification`).

use anyhow::{Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};

/// Identité cryptographique d'un nœud (clé ed25519 = AgentPubKey).
#[derive(Clone)]
pub struct NodeIdentity {
    signing: ed25519_dalek::SigningKey,
}

impl NodeIdentity {
    /// Génère une identité aléatoire.
    pub fn generate() -> Self {
        let mut rng = rand::rngs::OsRng;
        Self { signing: ed25519_dalek::SigningKey::generate(&mut rng) }
    }

    /// Identité déterministe à partir d'une graine de 32 octets.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self { signing: ed25519_dalek::SigningKey::from_bytes(seed) }
    }

    /// Clé publique (AgentPubKey), 32 octets.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// Clé publique en hexadécimal.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    /// Graine privée (32 octets) — à persister de façon sûre.
    pub fn seed_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// Charge la seed ed25519 depuis le keyring natif de l'OS (macOS Keychain,
    /// Windows Credential Manager, Linux libsecret) et retourne l'identité.
    ///
    /// Si la seed est absente du keyring, en génère une nouvelle, la stocke dans
    /// le keyring **et** dans `fallback_path` (protection double). En cas d'erreur
    /// keyring (daemon absent, permissions), repli sur `load_or_generate(fallback_path)`.
    ///
    /// `service` : nom de l'application dans le keyring (ex: `"ainonymous-daemon"`).
    /// `account` : identifiant de la clé (ex: `"quic-node-identity"`).
    #[cfg(feature = "secure-keyring")]
    pub fn load_or_generate_keyring(
        service: &str,
        account: &str,
        fallback_path: &std::path::Path,
    ) -> Result<Self> {
        use keyring::Entry;

        let entry = match Entry::new(service, account) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Keyring non disponible ({e}) — repli sur fichier");
                return Self::load_or_generate(fallback_path);
            }
        };

        // Essaie de charger la seed depuis le keyring
        match entry.get_password() {
            Ok(hex_seed) => {
                let bytes = hex::decode(&hex_seed)
                    .context("seed keyring invalide (hex decode)")?;
                let seed: [u8; 32] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("seed keyring corrompue (len={})", bytes.len()))?;
                tracing::info!("Identité ed25519 chargée depuis le keyring OS ({}/{})", service, account);
                Ok(Self::from_seed(&seed))
            }
            Err(keyring::Error::NoEntry) => {
                // Pas encore de seed → générer, stocker dans keyring ET fichier
                let identity = Self::generate();
                let hex_seed = hex::encode(identity.seed_bytes());
                if let Err(e) = entry.set_password(&hex_seed) {
                    tracing::warn!("Impossible de sauvegarder dans le keyring ({e}) — seed en clair uniquement");
                }
                // Sauvegarder aussi dans le fichier de secours
                if let Some(parent) = fallback_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(fallback_path, identity.seed_bytes());
                tracing::info!(
                    "Nouvelle identité ed25519 générée et stockée dans le keyring OS + {:?}",
                    fallback_path
                );
                Ok(identity)
            }
            Err(e) => {
                tracing::warn!("Erreur keyring ({e}) — repli sur fichier");
                Self::load_or_generate(fallback_path)
            }
        }
    }

    /// Génère une nouvelle identité, écrase le fichier `path`, et retourne
    /// `(nouvelle_identité, ancienne_pubkey_32_bytes)`.
    ///
    /// Utile pour la rotation de clé : l'appelant re-annonce la nouvelle pubkey
    /// dans le DHT puis redémarre le daemon (la session mTLS actuelle reste active
    /// jusqu'au prochain redémarrage).
    pub fn rotate_file(path: &std::path::Path) -> Result<(Self, [u8; 32])> {
        // Charger l'ancienne clé (pour retourner l'ancienne pubkey)
        let old_pubkey: [u8; 32] = if path.exists() {
            let bytes = std::fs::read(path).context("lecture ancienne seed")?;
            let seed: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("seed existante corrompue"))?;
            Self::from_seed(&seed).public_key_bytes()
        } else {
            [0u8; 32]
        };

        // Générer et persister la nouvelle clé
        let new_identity = Self::generate();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("création répertoire rotation")?;
        }
        std::fs::write(path, new_identity.seed_bytes())
            .context("sauvegarde nouvelle seed après rotation")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::info!(
            "Rotation identité ed25519 : {} → {}",
            hex::encode(old_pubkey),
            new_identity.public_key_hex()
        );
        Ok((new_identity, old_pubkey))
    }

    /// Charge l'identité depuis `path` (seed de 32 octets) ou en génère une
    /// nouvelle et la persiste sur disque. Crée les répertoires parents si absent.
    ///
    /// Le fichier contient exactement 32 octets (la seed ed25519 brute). Aucun
    /// chiffrement supplémentaire : la protection repose sur les permissions FS
    /// (chmod 600 recommandé).
    pub fn load_or_generate(path: &std::path::Path) -> Result<Self> {
        if path.exists() {
            let bytes = std::fs::read(path).context("lecture seed identité ed25519")?;
            let seed: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("seed corrompue : attendu 32 octets, obtenu {}", bytes.len()))?;
            tracing::info!("Identité ed25519 chargée depuis {:?}", path);
            Ok(Self::from_seed(&seed))
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .context("création répertoire identité")?;
            }
            let identity = Self::generate();
            std::fs::write(path, identity.seed_bytes())
                .context("sauvegarde seed identité ed25519")?;
            // Restreindre les permissions sur Unix (lecture seule par le propriétaire)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .context("chmod 600 seed identité")?;
            }
            tracing::info!("Nouvelle identité ed25519 générée et sauvegardée dans {:?}", path);
            Ok(identity)
        }
    }

    /// Certificat TLS auto-signé porté par cette clé ed25519.
    pub fn tls_cert(&self) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let pkcs8 = self.signing.to_pkcs8_der().context("ed25519 → pkcs8")?;
        let key_pair = rcgen::KeyPair::try_from(pkcs8.as_bytes())
            .map_err(|e| anyhow::anyhow!("rcgen KeyPair: {}", e))?;
        let params = rcgen::CertificateParams::new(vec!["ainonymous.local".to_string()])
            .context("CertificateParams")?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| anyhow::anyhow!("self_signed: {}", e))?;
        let cert_der = cert.der().clone();
        let key_der = PrivateKeyDer::try_from(pkcs8.as_bytes().to_vec())
            .map_err(|e| anyhow::anyhow!("pkcs8 → PrivateKeyDer: {}", e))?;
        Ok((cert_der, key_der))
    }
}

/// Extrait la clé publique ed25519 (32 octets) du SPKI d'un certificat DER.
pub fn ed25519_pubkey_from_cert(cert: &CertificateDer<'_>) -> Result<[u8; 32], rustls::Error> {
    use x509_parser::prelude::FromDer;
    let (_, parsed) = x509_parser::certificate::X509Certificate::from_der(cert.as_ref())
        .map_err(|_| rustls::Error::General("certificat X.509 illisible".into()))?;
    let data = parsed.public_key().subject_public_key.data.as_ref();
    if data.len() != 32 {
        return Err(rustls::Error::General(format!(
            "clé publique non ed25519 (len={})",
            data.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(data);
    Ok(out)
}

fn provider_algs() -> WebPkiSupportedAlgorithms {
    rustls::crypto::ring::default_provider().signature_verification_algorithms
}

/// Vérificateur côté CLIENT : le certificat serveur doit porter la clé ed25519
/// attendue (issue du plan de contrôle) ; la signature du handshake est vérifiée
/// réellement (preuve de possession).
#[derive(Debug)]
pub struct PeerKeyVerifier {
    expected: Option<[u8; 32]>,
    algs: WebPkiSupportedAlgorithms,
}

impl PeerKeyVerifier {
    /// `expected = Some(key)` : exige cette clé précise. `None` : accepte tout
    /// certificat ed25519 auto-signé valide (possession prouvée) sans liaison
    /// d'identité — repli quand le plan de contrôle ne fournit pas la clé.
    pub fn new(expected: Option<[u8; 32]>) -> Self {
        Self { expected, algs: provider_algs() }
    }
}

impl ServerCertVerifier for PeerKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let key = ed25519_pubkey_from_cert(end_entity)?;
        if let Some(expected) = self.expected {
            if key != expected {
                return Err(rustls::Error::General(
                    "clé publique du pair != clé attendue".into(),
                ));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// Vérificateur côté SERVEUR : exige un certificat client ed25519 auto-signé
/// valide. La liaison à un agent précis est gérée par le token de session
/// (contrôle applicatif, cf. `SessionRegistry`).
#[derive(Debug)]
pub struct Ed25519ClientVerifier {
    algs: WebPkiSupportedAlgorithms,
}

impl Ed25519ClientVerifier {
    pub fn new() -> Self {
        Self { algs: provider_algs() }
    }
}

impl ClientCertVerifier for Ed25519ClientVerifier {
    fn offer_client_auth(&self) -> bool { true }
    fn client_auth_mandatory(&self) -> bool { true }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        // Vérifier la présence d'une clé ed25519 valide ; le binding d'identité
        // est délégué au token de session (`SessionRegistry`).
        ed25519_pubkey_from_cert(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}