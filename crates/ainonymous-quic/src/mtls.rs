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
/// valide. La liaison à un agent précis est assurée en aval par le token de
/// session (enregistré avec le `requestor`).
#[derive(Debug)]
pub struct Ed25519ClientVerifier {
    algs: WebPkiSupportedAlgorithms,
}

impl Ed25519ClientVerifier {
    pub fn new() -> Self {
        Self { algs: provider_algs() }
    }
}

impl Default for Ed25519ClientVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientCertVerifier for Ed25519ClientVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        // Doit être un certificat ed25519 ; possession prouvée par la signature.
        let _ = ed25519_pubkey_from_cert(end_entity)?;
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
