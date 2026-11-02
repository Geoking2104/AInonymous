//! Offline signer. Read a raw 32-byte ADMIN seed from a protected file.
use ainonymous_revocation::Revocation;
use ed25519_dalek::{Signer, SigningKey};
use std::{
    error::Error,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: sign_revocation ADMIN_SEED_FILE NETWORK_ID AGENT_ID REASON OUTPUT_JSON".into(),
        );
    }
    let seed: [u8; 32] = std::fs::read(&args[0])?
        .try_into()
        .map_err(|_| "admin seed file must contain exactly 32 raw bytes")?;
    let key = SigningKey::from_bytes(&seed);
    let mut revocation = Revocation {
        network_id: args[1].clone(),
        agent_id: args[2].clone(),
        scope: "mesh".into(),
        reason: args[3].clone(),
        issued_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()?,
        signature: vec![],
    };
    revocation.signature = key.sign(&revocation.signing_bytes()).to_bytes().to_vec();
    revocation.verify(&args[1], &key.verifying_key().to_bytes())?;
    std::fs::write(&args[4], serde_json::to_vec_pretty(&revocation)?)?;
    Ok(())
}
