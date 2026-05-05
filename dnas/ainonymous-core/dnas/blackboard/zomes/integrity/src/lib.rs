use hdi::prelude::*;

#[hdk_entry_helper]
#[derive(Clone)]
pub struct BlackboardPost {
    pub prefix: String,               // "STATUS"|"FINDING"|"QUESTION"|"TIP"|"DONE"
    pub content: String,              // max 4096 chars
    pub tags: Vec<String>,            // max 10 tags, 64 chars chacun
    pub ttl_hours: u32,               // 1-168 (max 7 jours)
    pub reply_to: Option<ActionHash>, // threading
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    BlackboardPost(BlackboardPost),
}

#[hdk_link_types]
pub enum LinkTypes {
    TimelineToPost,    // anchor "timeline/all" → posts
    TagToPost,         // anchor "tags/{tag}" → posts
    AgentToPosts,      // agent → ses posts
    PostToReplies,     // post → réponses
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(OpEntry::CreateEntry { app_entry, .. }) => {
            match app_entry {
                EntryTypes::BlackboardPost(post) => validate_post(&post),
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_post(post: &BlackboardPost) -> ExternResult<ValidateCallbackResult> {
    // Contenu non vide et dans la limite
    if post.content.is_empty() || post.content.len() > 4096 {
        return Ok(ValidateCallbackResult::Invalid("Contenu entre 1 et 4096 caractères".into()));
    }

    // Préfixe valide
    let valid_prefixes = ["STATUS", "FINDING", "QUESTION", "TIP", "DONE"];
    let prefix_upper = post.prefix.to_uppercase();
    if !valid_prefixes.contains(&prefix_upper.as_str()) {
        return Ok(ValidateCallbackResult::Invalid(
            format!("Préfixe '{}' invalide. Valeurs acceptées: {:?}", post.prefix, valid_prefixes)
        ));
    }

    // TTL dans les limites
    if post.ttl_hours == 0 || post.ttl_hours > 168 {
        return Ok(ValidateCallbackResult::Invalid("ttl_hours doit être entre 1 et 168".into()));
    }

    // Tags
    if post.tags.len() > 10 {
        return Ok(ValidateCallbackResult::Invalid("Maximum 10 tags par post".into()));
    }
    for tag in &post.tags {
        if tag.is_empty() || tag.len() > 64 {
            return Ok(ValidateCallbackResult::Invalid("Tags entre 1 et 64 caractères".into()));
        }
    }

    // Détection PII basique
    let pii_patterns = ["/home/", "C:\\Users\\", "api_key=", "secret=",
                        "password", "token=", "Bearer ", "ssh-rsa"];
    let content_lower = post.content.to_lowercase();
    for pattern in &pii_patterns {
        if content_lower.contains(&pattern.to_lowercase()) {
            return Ok(ValidateCallbackResult::Invalid(
                format!("Contenu potentiellement sensible (pattern: '{}')", pattern)
            ));
        }
    }

    Ok(ValidateCallbackResult::Valid)
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
