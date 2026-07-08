use hdk::prelude::*;
use warrants_integrity::{Warrant, WarrantType, LinkTypes};

#[hdk_extern]
pub fn emit_warrant(warrant: Warrant) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::Warrant(warrant.clone()))?;

    // Créer un lien Agent -> Warrant
    let agent_pubkey = agent_info()?.agent_initial_pubkey;
    create_link(
        agent_pubkey,
        action_hash.clone(),
        LinkTypes::AgentToWarrants,
        LinkTag::new(warrant.warrant_type.to_string().as_bytes()),
    )?;

    Ok(action_hash)
}

#[hdk_extern]
pub fn verify_warrant(warrant: Warrant) -> ExternResult<bool> {
    // Utilise la vraie vérification ed25519 depuis ainonymous-types
    // (on reconstruit la pubkey)
    if let Ok(pubkey) = VerifyingKey::from_bytes(&warrant.issuer) {
        return Ok(warrant.verify(&pubkey));
    }
    Ok(false)
}

#[hdk_extern]
pub fn get_warrants(agent_id: String) -> ExternResult<Vec<Warrant>> {
    // Récupère les warrants via les liens
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
