//! Generate and verify HybridNode private-network admission proofs.
//!
//! The `hybridnode-core` integrity zome validates a `PrivateNetworkProof` in
//! `genesis_self_check`: an ed25519 signature by the network administrator over
//! the Holochain serialisation of `{ network_id, issued_at, issued_to }`. The
//! proof is supplied as role settings at install time, never through a zome
//! call.
//!
//! This tool produces exactly those bytes, so an operator can admit an agent
//! without hand-rolling serialisation:
//!
//! ```text
//! # 1. Admin key pair (32-byte hex seed) -> raw 36-byte key for dna.yaml
//! private-network-proof admin-pubkey --seed-hex <64 hex chars>
//!
//! # 2. Proof for one agent (base64, for a roles-settings YAML)
//! private-network-proof proof \
//!     --network-id ainonymous-hybridnode-hc07-v4 \
//!     --agent-key <uhCAk... base64 agent key> \
//!     --seed-hex <64 hex chars> \
//!     --out proof.b64
//!
//! # 3. Offline check of a proof before shipping it
//! private-network-proof verify \
//!     --network-id ... --agent-key uhCAk... \
//!     --admin-pubkey-hex <raw 36-byte hex> --proof proof.b64
//! ```
//!
//! The zome verifies with the HDK `verify_signature` host function, which checks
//! a plain ed25519 signature over the serialised payload; this tool signs and
//! verifies with `ed25519-dalek` over the same bytes produced by
//! `holochain_serialized_bytes::encode`.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use ed25519_dalek::{Signature as DalekSignature, Signer as _, Verifier as _};
use holochain_types::prelude::{AgentPubKey, Timestamp};
use serde::{Deserialize, Serialize};

/// Fields signed by the network administrator. Must stay field-for-field
/// identical to `PrivateNetworkProofPayload` in the integrity zome.
#[derive(Debug, Serialize)]
struct ProofPayload {
    network_id: String,
    issued_at: Timestamp,
    issued_to: AgentPubKey,
}

/// Wire shape of the proof, matching `PrivateNetworkProof` in the zome.
#[derive(Debug, Serialize, Deserialize)]
struct PrivateNetworkProof {
    network_id: String,
    issued_at: Timestamp,
    issued_to: AgentPubKey,
    signature: Vec<u8>,
}

fn encode<T: Serialize + std::fmt::Debug>(value: &T) -> Result<Vec<u8>> {
    holochain_serialized_bytes::encode(value).map_err(|e| anyhow::anyhow!("serialize: {e:?}"))
}

fn signing_key(seed_hex: &str) -> Result<ed25519_dalek::SigningKey> {
    let seed = hex_decode(seed_hex).context("--seed-hex must be 32 bytes of hex")?;
    let seed: [u8; 32] = seed
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("--seed-hex must decode to exactly 32 bytes"))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

fn hex_decode(input: &str) -> Result<Vec<u8>> {
    let cleaned: String = input.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        bail!("hex string has an odd number of characters");
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).context("invalid hex digit")? as u8;
        let lo = (pair[1] as char).to_digit(16).context("invalid hex digit")? as u8;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn parse_agent_key(value: &str) -> Result<AgentPubKey> {
    let value = value.trim();
    // Holochain displays agent keys as a 5-character type prefix ("uhCAk")
    // followed by base64url of the 36 raw bytes (32-byte key + 4-byte location).
    if value.len() == 53 && value.starts_with("uhC") {
        let body = URL_SAFE_NO_PAD
            .decode(&value[5..])
            .or_else(|_| STANDARD.decode(&value[5..]))
            .context("agent key body is not base64")?;
        if body.len() != 36 {
            bail!("agent key body must decode to 36 bytes, got {}", body.len());
        }
        return Ok(AgentPubKey::from_raw_36(body));
    }

    let raw = URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| STANDARD.decode(value))
        .context("agent key must be base64 (uhCAk...)")?;
    if raw.len() != 39 {
        bail!("agent key must decode to 39 bytes, got {}", raw.len());
    }
    Ok(AgentPubKey::from_raw_39(raw))
}

fn admin_agent_key(signing: &ed25519_dalek::SigningKey) -> AgentPubKey {
    AgentPubKey::from_raw_32(signing.verifying_key().to_bytes().to_vec())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("admin-pubkey") => admin_pubkey(&args[1..]),
        Some("proof") => make_proof(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  private-network-proof admin-pubkey --seed-hex <hex>\n  \
                 private-network-proof proof --network-id <id> --agent-key <uhCAk...> \
                 --seed-hex <hex> [--issued-at-micros <n>] [--out <path>]\n  \
                 private-network-proof verify --network-id <id> --agent-key <uhCAk...> \
                 --admin-pubkey-hex <raw36 hex> --proof <path>"
            );
            std::process::exit(2);
        }
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn admin_pubkey(args: &[String]) -> Result<()> {
    let seed = flag(args, "--seed-hex").context("--seed-hex is required")?;
    let signing = signing_key(seed)?;
    let agent = admin_agent_key(&signing);
    // DNA properties take the raw 36-byte form (32-byte key + 4-byte DHT location).
    println!("{}", hex_encode(agent.get_raw_36()));
    Ok(())
}

fn proof_bytes(
    network_id: &str,
    issued_to: &AgentPubKey,
    issued_at: Timestamp,
    signing: &ed25519_dalek::SigningKey,
) -> Result<Vec<u8>> {
    let payload = ProofPayload {
        network_id: network_id.to_string(),
        issued_at,
        issued_to: issued_to.clone(),
    };
    let payload_bytes = encode(&payload)?;
    let signature = signing.sign(&payload_bytes);

    // Fail loudly if our own bytes do not verify the way the zome will check them.
    signing
        .verifying_key()
        .verify(
            &payload_bytes,
            &DalekSignature::from_bytes(&signature.to_bytes()),
        )
        .map_err(|e| anyhow::anyhow!("internal error: generated signature does not verify: {e}"))?;

    let proof = PrivateNetworkProof {
        network_id: network_id.to_string(),
        issued_at,
        issued_to: issued_to.clone(),
        signature: signature.to_bytes().to_vec(),
    };
    encode(&proof)
}

fn make_proof(args: &[String]) -> Result<()> {
    let network_id = flag(args, "--network-id").context("--network-id is required")?;
    let agent_key = flag(args, "--agent-key").context("--agent-key is required")?;
    let seed = flag(args, "--seed-hex").context("--seed-hex is required")?;
    let issued_at = match flag(args, "--issued-at-micros") {
        Some(v) => Timestamp::from_micros(
            v.parse::<i64>()
                .context("--issued-at-micros must be an integer")?,
        ),
        None => Timestamp::now(),
    };

    let signing = signing_key(seed)?;
    let issued_to = parse_agent_key(agent_key)?;
    let bytes = proof_bytes(network_id, &issued_to, issued_at, &signing)?;
    let encoded = STANDARD.encode(&bytes);
    println!("{encoded}");

    if let Some(out) = flag(args, "--out") {
        std::fs::write(out, &encoded).with_context(|| format!("write {out}"))?;
    }
    Ok(())
}

fn verify(args: &[String]) -> Result<()> {
    let network_id = flag(args, "--network-id").context("--network-id is required")?;
    let agent_key = flag(args, "--agent-key").context("--agent-key is required")?;
    let admin_hex = flag(args, "--admin-pubkey-hex").context("--admin-pubkey-hex is required")?;
    let proof_path = flag(args, "--proof").context("--proof is required")?;

    let admin_raw = hex_decode(admin_hex)?;
    let admin = if admin_raw.len() == 36 {
        AgentPubKey::from_raw_36(admin_raw)
    } else if admin_raw.len() == 32 {
        AgentPubKey::from_raw_32(admin_raw)
    } else {
        bail!("--admin-pubkey-hex must be 36 bytes (raw) or 32 bytes");
    };
    let issued_to = parse_agent_key(agent_key)?;

    let encoded = std::fs::read_to_string(proof_path)
        .with_context(|| format!("read {proof_path}"))?
        .trim()
        .to_string();
    let raw = STANDARD
        .decode(&encoded)
        .or_else(|_| URL_SAFE_NO_PAD.decode(&encoded))
        .context("proof file must contain base64")?;
    let proof: PrivateNetworkProof =
        holochain_serialized_bytes::decode(&raw).map_err(|e| anyhow::anyhow!("deserialize proof: {e:?}"))?;

    if proof.network_id != network_id {
        bail!("proof was issued for network '{}'", proof.network_id);
    }
    if proof.issued_to != issued_to {
        bail!("proof was issued to a different agent");
    }
    let signature_bytes: [u8; 64] = proof
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be exactly 64 bytes"))?;

    let payload = ProofPayload {
        network_id: proof.network_id.clone(),
        issued_at: proof.issued_at,
        issued_to: proof.issued_to.clone(),
    };
    let payload_bytes = encode(&payload)?;
    let admin_key_bytes: [u8; 32] = admin
        .get_raw_32()
        .try_into()
        .map_err(|_| anyhow::anyhow!("admin key must be 32 bytes"))?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&admin_key_bytes)
        .context("admin key is not a valid ed25519 public key")?;
    verifying_key
        .verify(&payload_bytes, &DalekSignature::from_bytes(&signature_bytes))
        .map_err(|e| anyhow::anyhow!("signature does not match the admin key: {e}"))?;

    println!("OK: proof valid for network '{network_id}' and the given agent");
    Ok(())
}
