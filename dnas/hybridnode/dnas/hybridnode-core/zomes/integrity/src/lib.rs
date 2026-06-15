use hdi::prelude::*;

/// Entry types for the hybridnode-core integrity zome.
/// These mirror the entries defined in docs/HOLOCHAIN_ZOMES.md §attestation DNA.
#[hdk_entry_helper]
#[derive(Clone)]
pub struct NodeAttestation {
    pub agent: AgentPubKey,
    pub site_id: String,
    pub hardware_fingerprint: HardwareFingerprint,
    pub benchmark: BenchmarkResults,
    /// ed25519 signature over (agent || site_id || hardware_fingerprint || benchmark)
    pub signature: Vec<u8>,
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
    /// ed25519 signature from accuser over (accused || reason || timestamp).
    pub signature: Vec<u8>,
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
    pub signature: Vec<u8>,
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
        FlatOp::StoreEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::NodeAttestation(a) => validate_node_attestation(&a, &action),
                EntryTypes::ModelClaim(c) => validate_model_claim(&c),
                EntryTypes::Warrant(w) => validate_warrant(&w, &action),
                _ => Ok(ValidateCallbackResult::Valid),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_node_attestation(
    attestation: &NodeAttestation,
    action: &Create,
) -> ExternResult<ValidateCallbackResult> {
    if attestation.agent != action.author {
        return Ok(ValidateCallbackResult::Invalid(
            "NodeAttestation.agent must equal action author".to_string()
        ));
    }
    if attestation.hardware_fingerprint.vram_mb == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "NodeAttestation: vram_mb must be > 0".to_string()
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_model_claim(claim: &ModelClaim) -> ExternResult<ValidateCallbackResult> {
    let (start, end) = claim.layer_range;
    if start >= end {
        return Ok(ValidateCallbackResult::Invalid(
            "ModelClaim: layer_range start must be < end".to_string()
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_warrant(
    warrant: &Warrant,
    action: &Create,
) -> ExternResult<ValidateCallbackResult> {
    if warrant.accuser != action.author {
        return Ok(ValidateCallbackResult::Invalid(
            "Warrant.accuser must equal action author".to_string()
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Genesis self-check — validates the membrane proof for private networks.
#[hdk_extern]
pub fn genesis_self_check(data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    #[cfg(feature = "private-network")]
    {
        let proof_bytes = data.membrane_proof
            .as_ref()
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "Private network requires membrane proof".to_string()
            )))?;
        // Verify PrivateNetworkProof signature
        let _bytes = proof_bytes.bytes();
        // TODO: deserialize PrivateNetworkProof and verify ed25519 sig against network_key
    }
    Ok(ValidateCallbackResult::Valid)
}
