use hdi::prelude::*;

/// Entry types for the hybridnode-core integrity zome.
#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeAttestation {
    pub agent: AgentPubKey,
    pub site_id: String,
    pub hardware_fingerprint: HardwareFingerprint,
    pub benchmark: BenchmarkResults,
    pub timestamp: Timestamp,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct HardwareFingerprint {
    pub cpu_model: String,
    pub gpu_model: Option<String>,
    pub vram_mb: u64,
    pub ram_mb: u64,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct BenchmarkResults {
    /// Tokens per second on standard benchmark prompt.
    pub tokens_per_second: f64,
    /// Memory bandwidth in GB/s.
    pub memory_bandwidth_gbps: f64,
    pub benchmark_timestamp: Timestamp,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct ModelManifest {
    pub model_name: String,
    pub version: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub architecture: String,
    pub quant_format: String,
    pub num_layers: u32,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct ModelClaim {
    pub agent: AgentPubKey,
    pub model_hash: ActionHash,
    /// Layer range this agent handles [start, end).
    pub layer_range: (u32, u32),
    pub claimed_at: Timestamp,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct Warrant {
    pub accused: AgentPubKey,
    pub accuser: AgentPubKey,
    pub reason: WarrantReason,
    pub issued_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub enum WarrantReason {
    HashMismatch { model_hash: String },
    FalseAttestation,
    Timeout { count: u32 },
    SybilSuspicion,
}

#[hdk_entry_helper]
#[derive(Clone)]
pub struct WarrantRefutation {
    pub warrant_hash: ActionHash,
    pub refuted_by: AgentPubKey,
    pub evidence: String,
}

/// Entry types enum required by HDI.
#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    NodeAttestation(NodeAttestation),
    ModelManifest(ModelManifest),
    ModelClaim(ModelClaim),
    Warrant(Warrant),
    WarrantRefutation(WarrantRefutation),
}

/// Link types enum required by HDI.
#[hdk_link_types]
pub enum LinkTypes {
    AgentToAttestation,
    ManifestToClaim,
    AgentToWarrants,
    WarrantToRefutation,
    ModelToManifest,
}

/// Validation callback for all entries.
#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::CreateEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::NodeAttestation(a) => validate_node_attestation(&a, &action),
                EntryTypes::ModelManifest(m) => validate_model_manifest(&m),
                EntryTypes::ModelClaim(c) => validate_model_claim(&c, &action),
                EntryTypes::Warrant(w) => validate_warrant(&w, &action),
                EntryTypes::WarrantRefutation(r) => validate_refutation(&r, &action),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::Link(OpLink::CreateLink { link_type, action }) => {
            validate_create_link(link_type, &action)
        }
        FlatOp::Link(OpLink::DeleteLink {
            original_action,
            action,
            ..
        }) => {
            if action.author() != original_action.author() {
                Ok(ValidateCallbackResult::Invalid(
                    "only the link author may delete a HybridNode link".into(),
                ))
            } else {
                Ok(ValidateCallbackResult::Valid)
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_node_attestation(
    attestation: &NodeAttestation,
    action: &TypedAction<CreateData>,
) -> ExternResult<ValidateCallbackResult> {
    if &attestation.agent != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "NodeAttestation.agent must equal action author".to_string(),
        ));
    }
    if attestation.hardware_fingerprint.ram_mb == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "NodeAttestation: ram_mb must be > 0".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_model_manifest(manifest: &ModelManifest) -> ExternResult<ValidateCallbackResult> {
    if manifest.size_bytes == 0 || manifest.num_layers == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "ModelManifest size_bytes and num_layers must be greater than zero".into(),
        ));
    }
    if manifest.sha256.len() != 64 || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(ValidateCallbackResult::Invalid(
            "ModelManifest sha256 must contain 64 hexadecimal characters".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_model_claim(
    claim: &ModelClaim,
    action: &TypedAction<CreateData>,
) -> ExternResult<ValidateCallbackResult> {
    if &claim.agent != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "ModelClaim.agent must equal action author".into(),
        ));
    }
    let (start, end) = claim.layer_range;
    if start >= end {
        return Ok(ValidateCallbackResult::Invalid(
            "ModelClaim: layer_range start must be < end".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_refutation(
    refutation: &WarrantRefutation,
    action: &TypedAction<CreateData>,
) -> ExternResult<ValidateCallbackResult> {
    if &refutation.refuted_by != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "WarrantRefutation.refuted_by must equal action author".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_create_link(
    link_type: LinkTypes,
    action: &TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let Some(target_hash) = action.target_address.clone().into_action_hash() else {
        return Ok(ValidateCallbackResult::Invalid(
            "HybridNode links must target an action".into(),
        ));
    };
    let record = must_get_valid_record(target_hash)?;
    if record.action().author() != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "HybridNode links must target an entry authored by the link author".into(),
        ));
    }

    let expected_base = match link_type {
        LinkTypes::AgentToAttestation => {
            let Some(entry) = record
                .entry()
                .to_app_option::<NodeAttestation>()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
            else {
                return Ok(ValidateCallbackResult::Invalid(
                    "AgentToAttestation must target a NodeAttestation".into(),
                ));
            };
            AnyLinkableHash::from(entry.agent)
        }
        LinkTypes::ManifestToClaim => {
            let Some(entry) = record
                .entry()
                .to_app_option::<ModelClaim>()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
            else {
                return Ok(ValidateCallbackResult::Invalid(
                    "ManifestToClaim must target a ModelClaim".into(),
                ));
            };
            AnyLinkableHash::from(entry.model_hash)
        }
        LinkTypes::AgentToWarrants => {
            let Some(entry) = record
                .entry()
                .to_app_option::<Warrant>()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
            else {
                return Ok(ValidateCallbackResult::Invalid(
                    "AgentToWarrants must target a Warrant".into(),
                ));
            };
            AnyLinkableHash::from(entry.accused)
        }
        LinkTypes::WarrantToRefutation => {
            let Some(entry) = record
                .entry()
                .to_app_option::<WarrantRefutation>()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
            else {
                return Ok(ValidateCallbackResult::Invalid(
                    "WarrantToRefutation must target a WarrantRefutation".into(),
                ));
            };
            AnyLinkableHash::from(entry.warrant_hash)
        }
        LinkTypes::ModelToManifest => {
            return Ok(ValidateCallbackResult::Invalid(
                "ModelToManifest is reserved and cannot be created yet".into(),
            ));
        }
    };

    if action.base_address != expected_base {
        Ok(ValidateCallbackResult::Invalid(
            "HybridNode link base does not match the target entry".into(),
        ))
    } else {
        Ok(ValidateCallbackResult::Valid)
    }
}

fn validate_warrant(
    warrant: &Warrant,
    action: &TypedAction<CreateData>,
) -> ExternResult<ValidateCallbackResult> {
    if &warrant.accuser != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "Warrant.accuser must equal action author".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Properties baked into the DNA hash at packaging time (see `workdir/dna.yaml`,
/// `integrity.properties`). Read via `dna_info()` inside `genesis_self_check`,
/// which runs locally on the joining agent with no DHT/network access — DNA
/// properties are the only trust configuration available at that point, and
/// they are identical for every honest peer because they're part of the hash
/// that defines this network.
#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct HybridNodeDnaProperties {
    #[serde(default)]
    pub private_network: bool,
    /// Human-readable network identifier included in every signed admission
    /// proof. It prevents a valid proof from being replayed across private DNAs.
    #[serde(default)]
    pub network_id: String,
    /// Raw 36-byte ed25519 public key (32-byte key + 4-byte DHT location, no
    /// hash-type prefix — see `holo_hash::AgentPubKey::from_raw_36`) of the
    /// network administrator trusted to sign `PrivateNetworkProof`s.
    /// Required when `private_network` is `true`.
    #[serde(default)]
    pub network_admin_pubkey: Option<Vec<u8>>,
}

/// Membrane proof for HybridNode's private-network mode: the network admin
/// signs admission for one specific joining agent. Verified in
/// `genesis_self_check` against the admin key baked into DNA properties
/// (`HybridNodeDnaProperties::network_admin_pubkey`).
///
/// Note: this only proves *who signed off on this agent joining*. It does not
/// yet carry an expiration or replay-protection mechanism — `genesis_self_check`
/// has no verified-safe access to a trusted clock in this HDI version, so
/// proof expiry is left as a known, disclosed gap rather than guessed at.
#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct PrivateNetworkProof {
    pub network_id: String,
    pub issued_at: Timestamp,
    pub issued_to: AgentPubKey,
    /// ed25519 signature over [`PrivateNetworkProofPayload`] (this struct
    /// minus this field), produced by the network admin's private key.
    pub signature: Vec<u8>,
}

/// The fields of [`PrivateNetworkProof`] that are actually signed. Kept as a
/// separate type so the signed payload can never accidentally include the
/// signature itself.
#[derive(Debug, Serialize)]
struct PrivateNetworkProofPayload {
    network_id: String,
    issued_at: Timestamp,
    issued_to: AgentPubKey,
}

impl PrivateNetworkProof {
    fn signed_payload(&self) -> PrivateNetworkProofPayload {
        PrivateNetworkProofPayload {
            network_id: self.network_id.clone(),
            issued_at: self.issued_at,
            issued_to: self.issued_to.clone(),
        }
    }
}

/// Genesis self-check — validates the membrane proof for private networks.
///
/// Runs locally on the joining agent before it has any DHT/network access, so
/// the only trust anchor available is the DNA's own properties, which are
/// baked into the DNA hash at packaging time.
#[hdk_extern]
pub fn genesis_self_check(data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    let props: HybridNodeDnaProperties =
        dna_info()?.modifiers.properties.try_into().map_err(|e| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "Failed to parse HybridNode DNA properties: {e:?}"
            )))
        })?;

    if !props.private_network {
        return Ok(ValidateCallbackResult::Valid);
    }
    if props.network_id.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "private_network is true but network_id is empty".to_string(),
        ));
    }

    let admin_pubkey_bytes = match props.network_admin_pubkey {
        Some(bytes) => bytes,
        None => {
            return Ok(ValidateCallbackResult::Invalid(
                "private_network is true but no network_admin_pubkey is configured in DNA properties".to_string(),
            ));
        }
    };
    let admin_pubkey = AgentPubKey::from_raw_36(admin_pubkey_bytes);

    let proof_bytes = match data.membrane_proof.as_ref() {
        Some(p) => p,
        None => {
            return Ok(ValidateCallbackResult::Invalid(
                "Private network requires a membrane proof".to_string(),
            ));
        }
    };

    let proof: PrivateNetworkProof = match (**proof_bytes).clone().try_into() {
        Ok(p) => p,
        Err(e) => {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "Failed to deserialize PrivateNetworkProof: {e:?}"
            )));
        }
    };

    if proof.issued_to != data.agent_key {
        return Ok(ValidateCallbackResult::Invalid(
            "PrivateNetworkProof was not issued to this agent".to_string(),
        ));
    }
    if proof.network_id != props.network_id {
        return Ok(ValidateCallbackResult::Invalid(
            "PrivateNetworkProof was issued for a different network".to_string(),
        ));
    }

    let sig_bytes: [u8; 64] = match proof.signature.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => {
            return Ok(ValidateCallbackResult::Invalid(
                "PrivateNetworkProof.signature must be exactly 64 bytes".to_string(),
            ));
        }
    };
    let signature = Signature::from(sig_bytes);

    let valid = verify_signature(admin_pubkey, signature, proof.signed_payload())?;
    if !valid {
        return Ok(ValidateCallbackResult::Invalid(
            "PrivateNetworkProof signature does not match the configured network admin key"
                .to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
