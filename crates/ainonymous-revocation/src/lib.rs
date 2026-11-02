//! Shared, deterministic CH6 wire format. No conductor or transport dependency.
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Revocation {
    pub network_id: String,
    /// Canonical Holochain AgentPubKey string (not the independent TLS key).
    pub agent_id: String,
    pub scope: String,
    pub reason: String,
    pub issued_at_ms: u64,
    pub signature: Vec<u8>,
}

impl Revocation {
    /// Versioned domain separator, then UTF-8 strings prefixed with u32 BE byte
    /// lengths, then u64 BE milliseconds. Signature is excluded.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = b"AInonymous/Revocation/v1\0".to_vec();
        for value in [&self.network_id, &self.agent_id, &self.scope, &self.reason] {
            bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes.extend_from_slice(&self.issued_at_ms.to_be_bytes());
        bytes
    }

    pub fn verify_author(
        &self,
        network: &str,
        admin: &[u8; 32],
        author: &[u8],
        author_id: &str,
    ) -> Result<(), String> {
        if author != admin || self.agent_id == author_id {
            return Err("only admin may revoke; admin self-revocation is forbidden".into());
        }
        self.verify(network, admin)
    }

    pub fn verify(&self, network: &str, admin: &[u8; 32]) -> Result<(), String> {
        if self.network_id != network || network.is_empty() || network.len() > 256 {
            return Err("revocation network mismatch".into());
        }
        if self.scope != "mesh"
            || self.reason.trim().is_empty()
            || self.reason.len() > 512
            || self.agent_id.is_empty()
            || self.agent_id.len() > 128
            || self.issued_at_ms == 0
        {
            return Err("invalid revocation scope, agent, reason or timestamp".into());
        }
        // A Holochain multibase AgentPubKey contains a 3-byte type prefix,
        // the 32-byte public key, then the 4-byte DHT location. Transport keys
        // are separate and must never be used to identify the mesh admin.
        use base64::Engine as _;
        if let Some(encoded) = self.agent_id.strip_prefix('u') {
            if let Ok(raw) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded) {
                if raw.len() == 39 && raw[3..35] == admin[..] {
                    return Err("admin self-revocation is forbidden".into());
                }
            }
        }
        let key = VerifyingKey::from_bytes(admin).map_err(|e| e.to_string())?;
        let signature = Signature::from_slice(&self.signature).map_err(|e| e.to_string())?;
        key.verify_strict(&self.signing_bytes(), &signature)
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransportBinding {
    pub agent_id: String,
    pub node_pubkey: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn signature_binds_every_field_and_network() {
        let admin = SigningKey::from_bytes(&[7; 32]);
        let mut r = Revocation {
            network_id: "test".into(),
            agent_id: "agent".into(),
            scope: "mesh".into(),
            reason: "removed".into(),
            issued_at_ms: 1,
            signature: vec![],
        };
        r.signature = admin.sign(&r.signing_bytes()).to_bytes().to_vec();
        let key = admin.verifying_key().to_bytes();
        assert!(r.verify("test", &key).is_ok());
        assert!(r.verify_author("test", &key, &key, "admin").is_ok());
        assert!(r.verify_author("test", &key, &[0; 32], "admin").is_err());
        assert!(r.verify_author("test", &key, &key, "agent").is_err());
        assert!(r
            .verify("other", &admin.verifying_key().to_bytes())
            .is_err());
        for field in 0..5 {
            let mut changed = r.clone();
            match field {
                0 => changed.network_id.push('x'),
                1 => changed.agent_id.push('x'),
                2 => changed.scope.push('x'),
                3 => changed.reason.push('x'),
                _ => changed.issued_at_ms += 1,
            }
            assert!(changed
                .verify("test", &admin.verifying_key().to_bytes())
                .is_err());
        }
        assert!(r
            .verify(
                "test",
                &SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes()
            )
            .is_err());
        r.signature.pop();
        assert!(r.verify("test", &admin.verifying_key().to_bytes()).is_err());
    }
}
