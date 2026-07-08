use hdk::prelude::*;
use warrants_integrity::{Warrant, WarrantType};

#[hdk_extern]
pub fn emit_warrant(warrant: Warrant) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::Warrant(warrant))?;
    Ok(action_hash)
}

#[hdk_extern]
pub fn verify_warrant(warrant: Warrant) -> ExternResult<bool> {
    // TODO: Implémenter la vraie vérification cryptographique
    // Pour l'instant on fait une validation basique
    if warrant.signature.len() == 64 && !warrant.is_expired() {
        Ok(true)
    } else {
        Ok(false)
    }
}

#[hdk_extern]
pub fn get_warrants(agent_id: String) -> ExternResult<Vec<Warrant>> {
    // TODO: Filtrer par issuer (agent_id)
    // Pour l'instant on retourne tous les warrants (à affiner)
    let warrants: Vec<Warrant> = query(
        ChainQueryFilter::new()
            .entry_type(EntryType::App(AppEntryDef::new(
                EntryTypesUnit::Warrant.try_into().unwrap(),
                0,
                EntryVisibility::Public,
            )))
            .include_entries(true),
    )?
    .into_iter()
    .filter_map(|el| {
        if let RecordEntry::Present(entry) = el.entry {
            entry.app_entry().ok()
        } else {
            None
        }
    })
    .collect();

    Ok(warrants)
}

// Helper pour vérifier l'expiration (doit être dans l'integrity aussi)
impl Warrant {
    pub fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }
        let now = sys_time().unwrap().0.as_secs();
        now > self.issued_at + self.ttl_seconds
    }
}
