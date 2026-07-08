use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use std::path::PathBuf;
use anyhow::Result;
use keyring::Entry;
use std::fs;

#[derive(Clone, Debug)]
pub struct NodeIdentity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl NodeIdentity {
    /// Charge ou génère une identité ed25519.
    /// Priorité :
    /// 1. Keyring OS natif (si feature activée et disponible)
    /// 2. Fichier sur disque (identity_path)
    pub fn load_or_generate_keyring(
        service: &str,
        username: &str,
        identity_path: &PathBuf,
    ) -> Result<Self> {
        // Essayer le keyring OS d'abord
        if let Ok(entry) = Entry::new(service, username) {
            if let Ok(secret) = entry.get_secret() {
                if secret.len() == 32 {
                    let signing_key = SigningKey::from_bytes(&secret.try_into().unwrap());
                    return Ok(Self {
                        signing_key,
                        verifying_key: signing_key.verifying_key(),
                    });
                }
            }

            // Générer et stocker dans le keyring
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);
            if entry.set_secret(&signing_key.to_bytes()).is_ok() {
                return Ok(Self {
                    signing_key,
                    verifying_key: signing_key.verifying_key(),
                });
            }
        }

        // Fallback fichier
        if identity_path.exists() {
            let bytes = fs::read(identity_path)?;
            if bytes.len() == 32 {
                let signing_key = SigningKey::from_bytes(&bytes.try_into().unwrap());
                return Ok(Self {
                    signing_key,
                    verifying_key: signing_key.verifying_key(),
                });
            }
        }

        // Générer et sauvegarder dans le fichier
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        if let Some(parent) = identity_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(identity_path, signing_key.to_bytes())?;

        Ok(Self {
            signing_key,
            verifying_key: signing_key.verifying_key(),
        })
    }

    /// Rotation complète de la clé (keyring + fallback fichier)
    pub fn rotate(
        service: &str,
        username: &str,
        identity_path: &PathBuf,
    ) -> Result<(VerifyingKey, VerifyingKey)> {
        let old_identity = Self::load_or_generate_keyring(service, username, identity_path)?;
        let old_pubkey = old_identity.verifying_key;

        // Générer nouvelle clé
        let mut csprng = OsRng;
        let new_signing_key = SigningKey::generate(&mut csprng);
        let new_pubkey = new_signing_key.verifying_key();

        // Essayer keyring
        if let Ok(entry) = Entry::new(service, username) {
            if entry.set_secret(&new_signing_key.to_bytes()).is_ok() {
                return Ok((old_pubkey, new_pubkey));
            }
        }

        // Fallback fichier
        fs::write(identity_path, new_signing_key.to_bytes())?;

        Ok((old_pubkey, new_pubkey))
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    pub fn tls_cert(&self) -> Result<(rustls::pki_types::CertificateDer<'static>, rustls::pki_types::PrivateKeyDer<'static>)> {
        let cert = rcgen::Certificate::from_params(rcgen::CertificateParams::new(vec!["localhost".into()]))?;
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        Ok((cert_der.into(), key_der.into()))
    }
}