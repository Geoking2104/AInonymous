#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_warrant_sign_and_verify() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        let claim = ModelClaim {
            model_id: "gemma4-e4b".to_string(),
            model_hash: "abc123".to_string(),
            vram_required_gb: 24.0,
            max_context: 8192,
            supported_backends: vec!["cuda".to_string()],
        };

        let warrant = Warrant::new_signed(
            &signing_key,
            WarrantType::ModelClaim,
            serde_json::to_value(claim).unwrap(),
            3600,
        ).unwrap();

        let pubkey = signing_key.verifying_key();
        assert!(warrant.verify(&pubkey));
    }

    #[test]
    fn test_warrant_expired() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        let warrant = Warrant::new_signed(
            &signing_key,
            WarrantType::NodeCapabilities,
            serde_json::json!({}),
            1, // expire dans 1 seconde
        ).unwrap();

        // On attend un peu
        std::thread::sleep(std::time::Duration::from_millis(1100));

        assert!(warrant.is_expired());
    }
}
