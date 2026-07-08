use hdk::prelude::*;
use warrants_integrity::{Warrant, WarrantType, LinkTypes};

#[hdk_extern]
pub fn emit_warrant(warrant: Warrant) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::Warrant(warrant.clone()))?;

    let agent_pubkey = agent_info()?.agent_initial_pubkey;

    // On tag le lien avec le type de warrant pour éviter les conflits et permettre les requêtes ciblées
    let tag = LinkTag::new(warrant.warrant_type.to_string().as_bytes());

    create_link(
        agent_pubkey,
        action_hash.clone(),
        LinkTypes::AgentToWarrants,
        tag,
    )?;

    Ok(action_hash)
}

/// Émet un warrant en supprimant d'abord les anciens du même type (gestion des conflits)
pub fn emit_warrant_with_cleanup(warrant: Warrant) -> ExternResult<ActionHash> {
    let agent_pubkey = agent_info()?.agent_initial_pubkey;
    let tag = LinkTag::new(warrant.warrant_type.to_string().as_bytes());

    // Supprime les anciens liens du même type (rotation de warrant)
    let existing_links = get_links(
        agent_pubkey.clone(),
        LinkTypes::AgentToWarrants,
        Some(tag.clone()),
    )?;

    for link in existing_links {
        delete_link(link.create_link_hash)?;
    }

    // Crée le nouveau warrant + lien
    let action_hash = create_entry(EntryTypes::Warrant(warrant.clone()))?;
    create_link(agent_pubkey, action_hash.clone(), LinkTypes::AgentToWarrants, tag)?;

    Ok(action_hash)
}

#[hdk_extern]
pub fn verify_warrant(warrant: Warrant) -> ExternResult<bool> {
    if let Ok(pubkey) = VerifyingKey::from_bytes(&warrant.issuer) {
        return Ok(warrant.verify(&pubkey));
    }
    Ok(false)
}

#[hdk_extern]
pub fn get_warrants(agent_id: String) -> ExternResult<Vec<Warrant>> {
    let agent_pubkey: AgentPubKey = agent_id.try_into()?;

    let links = get_links(
        agent_pubkey,
        LinkTypes::AgentToWarrants,
        None,
    )?;

    let mut warrants = Vec::new();
    for link in links {
        if let Some(record) = get(link.target.into(), GetOptions::default())? {
            if let RecordEntry::Present(entry) = record.entry {
                if let Some(w) = entry.app_entry::<Warrant>() {
                    warrants.push(w);
                }
            }
        }
    }
    Ok(warrants)
}

#[hdk_extern]
pub fn get_warrants_by_type(agent_id: String, warrant_type: WarrantType) -> ExternResult<Vec<Warrant>> {
    let agent_pubkey: AgentPubKey = agent_id.try_into()?;
    let tag = LinkTag::new(warrant_type.to_string().as_bytes());

    let links = get_links(
        agent_pubkey,
        LinkTypes::AgentToWarrants,
        Some(tag),
    )?;

    let mut warrants = Vec::new();
    for link in links {
        if let Some(record) = get(link.target.into(), GetOptions::default())? {
            if let RecordEntry::Present(entry) = record.entry {
                if let Some(w) = entry.app_entry::<Warrant>() {
                    warrants.push(w);
                }
            }
        }
    }
    Ok(warrants)
}
