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
    pub fn load_or_generate_keyring(
        service: &str,
        username: &str,
        identity_path: &PathBuf,
    ) -> Result<Self> {
        if let Ok(entry) = Entry::new(service, username) {
            if let Ok(secret) = entry.get_secret() {
                if secret.len() == 32 {
                    let signing_key = SigningKey::from_bytes(&secret.try_into().unwrap());
                    return Ok(Self { signing_key, verifying_key: signing_key.verifying_key() });
                }
            }
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);
            if entry.set_secret(&signing_key.to_bytes()).is_ok() {
                return Ok(Self { signing_key, verifying_key: signing_key.verifying_key() });
            }
        }

        if identity_path.exists() {
            let bytes = fs::read(identity_path)?;
            if bytes.len() == 32 {
                let signing_key = SigningKey::from_bytes(&bytes.try_into().unwrap());
                return Ok(Self { signing_key, verifying_key: signing_key.verifying_key() });
            }
        }

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        if let Some(parent) = identity_path.parent() { let _ = fs::create_dir_all(parent); }
        fs::write(identity_path, signing_key.to_bytes())?;

        Ok(Self { signing_key, verifying_key: signing_key.verifying_key() })
    }

    pub fn rotate(
        service: &str,
        username: &str,
        identity_path: &PathBuf,
    ) -> Result<(VerifyingKey, VerifyingKey)> {
        let old = Self::load_or_generate_keyring(service, username, identity_path)?;
        let old_pub = old.verifying_key;

        let mut csprng = OsRng;
        let new_signing = SigningKey::generate(&mut csprng);
        let new_pub = new_signing.verifying_key();

        if let Ok(entry) = Entry::new(service, username) {
            if entry.set_secret(&new_signing.to_bytes()).is_ok() {
                return Ok((old_pub, new_pub));
            }
        }
        fs::write(identity_path, new_signing.to_bytes())?;
        Ok((old_pub, new_pub))
    }

    pub fn rotate_file(identity_path: &PathBuf) -> Result<(Self, [u8; 32])> {
        let old_bytes = if identity_path.exists() {
            fs::read(identity_path)?.try_into().map_err(|_| anyhow::anyhow!("invalid key length"))?
        } else {
            [0u8; 32]
        };

        let mut csprng = OsRng;
        let new_signing_key = SigningKey::generate(&mut csprng);
        if let Some(parent) = identity_path.parent() { let _ = fs::create_dir_all(parent); }
        fs::write(identity_path, new_signing_key.to_bytes())?;

        let new_identity = Self {
            signing_key: new_signing_key,
            verifying_key: new_signing_key.verifying_key(),
        };

        Ok((new_identity, old_bytes))
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    pub fn tls_cert(&self) -> Result<(rustls::pki_types::CertificateDer<'static>, rustls::pki_types::PrivateKeyDer<'static>)> {
        let cert = rcgen::Certificate::from_params(rcgen::CertificateParams::new(vec!["localhost".into()]))?;
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        Ok((cert_der.into(), key_der.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_or_generate_creates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_identity.key");

        let id1 = NodeIdentity::load_or_generate_keyring("test-service", "test-user", &path).unwrap();
        assert!(path.exists());

        let id2 = NodeIdentity::load_or_generate_keyring("test-service", "test-user", &path).unwrap();
        assert_eq!(id1.public_key_hex(), id2.public_key_hex());
    }

    #[test]
    fn test_rotate_file_changes_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rotate_test.key");

        let id1 = NodeIdentity::load_or_generate_keyring("test-service", "test-user", &path).unwrap();
        let old_pub = id1.public_key_bytes();

        let (id2, returned_old) = NodeIdentity::rotate_file(&path).unwrap();

        assert_ne!(old_pub, id2.public_key_bytes());
        assert_eq!(old_pub, returned_old);
    }

    #[test]
    fn test_rotate_returns_different_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rotate2.key");

        let _ = NodeIdentity::load_or_generate_keyring("test-service", "test-user", &path).unwrap();
        let (old_pub, new_pub) = NodeIdentity::rotate("test-service", "test-user", &path).unwrap();

        assert_ne!(old_pub.to_bytes(), new_pub.to_bytes());
    }
}