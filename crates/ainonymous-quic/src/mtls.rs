// mtls.rs - Palier E en cours de finalisation
// NodeIdentity avec support keyring OS natif + rotation ed25519

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use std::path::PathBuf;
use anyhow::Result;
use keyring::Entry;

#[derive(Clone)]
pub struct NodeIdentity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl NodeIdentity {
    /// Charge depuis le keyring OS (macOS Keychain / Windows Credential Manager / libsecret)
    /// ou génère une nouvelle clé et la stocke.
    pub fn load_or_generate_keyring(service: &str, username: &str) -> Result<Self> {
        let entry = Entry::new(service, username)?;

        if let Ok(secret) = entry.get_secret() {
            if secret.len() == 32 {
                let signing_key = SigningKey::from_bytes(&secret.try_into().unwrap());
                let verifying_key = signing_key.verifying_key();
                return Ok(Self { signing_key, verifying_key });
            }
        }

        // Génération nouvelle clé
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        // Stockage sécurisé
        entry.set_secret(&signing_key.to_bytes())?;

        Ok(Self { signing_key, verifying_key })
    }

    /// Rotation de clé : génère une nouvelle clé, la stocke, et retourne l'ancienne pubkey
    pub fn rotate_keyring(service: &str, username: &str) -> Result<(VerifyingKey, VerifyingKey)> {
        let entry = Entry::new(service, username)?;

        let old_pubkey = if let Ok(secret) = entry.get_secret() {
            if secret.len() == 32 {
                let old_key = SigningKey::from_bytes(&secret.try_into().unwrap());
                old_key.verifying_key()
            } else {
                return Err(anyhow::anyhow!("Clé invalide dans le keyring"));
            }
        } else {
            return Err(anyhow::anyhow!("Aucune clé existante"));
        };

        // Nouvelle clé
        let mut csprng = OsRng;
        let new_signing_key = SigningKey::generate(&mut csprng);
        let new_verifying_key = new_signing_key.verifying_key();

        entry.set_secret(&new_signing_key.to_bytes())?;

        Ok((old_pubkey, new_verifying_key))
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    pub fn tls_cert(&self) -> Result<(rustls::pki_types::CertificateDer<'static>, rustls::pki_types::PrivateKeyDer<'static>)> {
        // Génération certificat auto-signé ed25519 pour mTLS
        let cert = rcgen::Certificate::from_params(rcgen::CertificateParams::new(vec!["localhost".into()]))?;
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        Ok((cert_der.into(), key_der.into()))
    }
}