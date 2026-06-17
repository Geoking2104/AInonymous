use hdk::prelude::*;
use hybridnode_integrity::*;

// ---------------------------------------------------------------------------
// Node Attestation
// ---------------------------------------------------------------------------

#[hdk_extern]
pub fn publish_attestation(attestation: NodeAttestation) -> ExternResult<ActionHash> {
    let hash = create_entry(EntryTypes::NodeAttestation(attestation.clone()))?;
    create_link(
        attestation.agent.clone(),
        hash.clone(),
        LinkTypes::AgentToAttestation,
        (),
    )?;
    Ok(hash)
}

#[hdk_extern]
pub fn get_node_attestation(agent: AgentPubKey) -> ExternResult<Option<NodeAttestation>> {
    let links = get_links(
        GetLinksInputBuilder::try_new(agent, LinkTypes::AgentToAttestation)?.build(),
    )?;
    let latest = links.into_iter()
        .max_by_key(|l| l.timestamp);
    if let Some(link) = latest {
        let hash = ActionHash::try_from(link.target).map_err(|_| {
            wasm_error!(WasmErrorInner::Guest("invalid link target".to_string()))
        })?;
        let record = get(hash, GetOptions::default())?;
        if let Some(r) = record {
            let attestation: NodeAttestation = r.entry().to_app_option()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
                .ok_or(wasm_error!(WasmErrorInner::Guest("missing entry".to_string())))?;
            return Ok(Some(attestation));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Model Manifest
// ---------------------------------------------------------------------------

#[hdk_extern]
pub fn publish_model_manifest(manifest: ModelManifest) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::ModelManifest(manifest))
}

// ---------------------------------------------------------------------------
// Model Claim
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct ClaimModelInput {
    pub manifest_hash: ActionHash,
    pub layer_range: (u32, u32),
}

#[hdk_extern]
pub fn claim_model(input: ClaimModelInput) -> ExternResult<ActionHash> {
    let agent = agent_info()?.agent_initial_pubkey;
    let claim = ModelClaim {
        agent: agent.clone(),
        model_hash: input.manifest_hash.clone(),
        layer_range: input.layer_range,
        claimed_at: sys_time()?,
    };
    let claim_hash = create_entry(EntryTypes::ModelClaim(claim))?;
    create_link(
        input.manifest_hash,
        claim_hash.clone(),
        LinkTypes::ManifestToClaim,
        (),
    )?;
    Ok(claim_hash)
}

// ---------------------------------------------------------------------------
// Warrants
// ---------------------------------------------------------------------------

#[hdk_extern]
pub fn publish_warrant(warrant: Warrant) -> ExternResult<ActionHash> {
    let hash = create_entry(EntryTypes::Warrant(warrant.clone()))?;
    create_link(
        warrant.accused.clone(),
        hash.clone(),
        LinkTypes::AgentToWarrants,
        (),
    )?;
    Ok(hash)
}

#[hdk_extern]
pub fn get_active_warrants(agent: AgentPubKey) -> ExternResult<Vec<Warrant>> {
    let links = get_links(
        GetLinksInputBuilder::try_new(agent, LinkTypes::AgentToWarrants)?.build(),
    )?;
    let mut warrants = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target).map_err(|_| {
            wasm_error!(WasmErrorInner::Guest("invalid link target".to_string()))
        })?;
        if let Some(record) = get(hash, GetOptions::default())? {
            let warrant: Warrant = record.entry().to_app_option()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
                .ok_or(wasm_error!(WasmErrorInner::Guest("missing entry".to_string())))?;
            warrants.push(warrant);
        }
    }
    Ok(warrants)
}

#[hdk_extern]
pub fn refute_warrant(input: WarrantRefutation) -> ExternResult<ActionHash> {
    let refutation_hash = create_entry(EntryTypes::WarrantRefutation(input.clone()))?;
    create_link(
        input.warrant_hash,
        refutation_hash.clone(),
        LinkTypes::WarrantToRefutation,
        (),
    )?;
    Ok(refutation_hash)
}

// ---------------------------------------------------------------------------
// Attestation status
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub enum AttestationStatus {
    Valid,
    Expired,
    Missing,
    Sanctioned { warrant_count: usize },
}

#[hdk_extern]
pub fn verify_node_attestation(agent: AgentPubKey) -> ExternResult<AttestationStatus> {
    let attestation = get_node_attestation(agent.clone())?;
    if attestation.is_none() {
        return Ok(AttestationStatus::Missing);
    }

    let warrants = get_active_warrants(agent)?;
    if !warrants.is_empty() {
        return Ok(AttestationStatus::Sanctioned { warrant_count: warrants.len() });
    }

    Ok(AttestationStatus::Valid)
}
