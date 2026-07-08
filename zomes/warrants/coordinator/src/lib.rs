#[hdk_extern]
pub fn get_warrants_by_type(
    agent_id: String,
    warrant_type: WarrantType,
) -> ExternResult<Vec<Warrant>> {
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
