use hdk::prelude::*;
use blackboard_integrity::*;

const TIMELINE_ANCHOR: &str = "timeline/all";
const POSTS_PER_PAGE: usize = 50;

/// Publier un message sur le blackboard
#[hdk_extern]
pub fn post(input: PostInput) -> ExternResult<ActionHash> {
    // Strip PII avant stockage
    let clean_content = strip_pii(&input.content);
    let clean_tags: Vec<String> = input.tags.iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .take(10)
        .collect();

    let bp = BlackboardPost {
        prefix:   input.prefix.to_uppercase(),
        content:  clean_content,
        tags:     clean_tags.clone(),
        ttl_hours: input.ttl_hours.unwrap_or(24),
        reply_to: input.reply_to.clone(),
    };

    let hash = create_entry(EntryTypes::BlackboardPost(bp))?;

    // Lier au timeline global
    let timeline = anchor_for_timeline()?;
    create_link(timeline, hash.clone(), LinkTypes::TimelineToPost, ())?;

    // Lier chaque tag
    for tag in &clean_tags {
        let tag_anchor = anchor("tags", tag)?;
        create_link(tag_anchor, hash.clone(), LinkTypes::TagToPost, ())?;
    }

    // Lier l'agent à ses posts
    let agent = agent_info()?.agent_latest_pubkey;
    create_link(agent, hash.clone(), LinkTypes::AgentToPosts, ())?;

    // Lier le post parent à cette réponse
    if let Some(ref parent) = input.reply_to {
        create_link(parent.clone(), hash.clone(), LinkTypes::PostToReplies, ())?;
    }

    Ok(hash)
}

/// Récupérer les posts récents (filtrés par TTL)
#[hdk_extern]
pub fn get_recent_posts(_: ()) -> ExternResult<Vec<PostSummary>> {
    let timeline = anchor_for_timeline()?;
    let links = get_links(
        GetLinksInputBuilder::try_new(timeline, LinkTypes::TimelineToPost)?.build()
    )?;

    let now_ms = sys_time()?.as_millis() as i64;
    let mut posts = Vec::new();

    // Prendre les N derniers liens
    for link in links.iter().rev().take(POSTS_PER_PAGE * 3) {
        if let Some(hash) = link.target.clone().into_action_hash() {
            if let Some(record) = get(hash.clone(), GetOptions::default())? {
                if let Ok(Some(bp)) = record.entry().to_app_option::<BlackboardPost>() {
                    let created_ms = link.timestamp.as_millis() as i64;
                    let ttl_ms = bp.ttl_hours as i64 * 3_600_000;
                    if now_ms - created_ms < ttl_ms {
                        posts.push(PostSummary {
                            hash,
                            prefix: bp.prefix,
                            content: bp.content,
                            tags: bp.tags,
                            ttl_hours: bp.ttl_hours,
                            reply_to: bp.reply_to,
                            author: record.action().author().clone().to_string(),
                            created_at_ms: created_ms,
                        });
                        if posts.len() >= POSTS_PER_PAGE {
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(posts)
}

/// Chercher par tag exact
#[hdk_extern]
pub fn search_by_tag(tag: String) -> ExternResult<Vec<PostSummary>> {
    let tag_lower = tag.trim().to_lowercase();
    let tag_anchor = anchor("tags", &tag_lower)?;
    let links = get_links(
        GetLinksInputBuilder::try_new(tag_anchor, LinkTypes::TagToPost)?.build()
    )?;

    let now_ms = sys_time()?.as_millis() as i64;
    let mut posts = Vec::new();

    for link in links.iter().rev().take(POSTS_PER_PAGE) {
        if let Some(hash) = link.target.clone().into_action_hash() {
            if let Some(record) = get(hash.clone(), GetOptions::default())? {
                if let Ok(Some(bp)) = record.entry().to_app_option::<BlackboardPost>() {
                    let created_ms = link.timestamp.as_millis() as i64;
                    let ttl_ms = bp.ttl_hours as i64 * 3_600_000;
                    if now_ms - created_ms < ttl_ms {
                        posts.push(PostSummary {
                            hash,
                            prefix: bp.prefix,
                            content: bp.content,
                            tags: bp.tags,
                            ttl_hours: bp.ttl_hours,
                            reply_to: bp.reply_to,
                            author: record.action().author().clone().to_string(),
                            created_at_ms: created_ms,
                        });
                    }
                }
            }
        }
    }

    Ok(posts)
}

/// Récupérer les réponses à un post
#[hdk_extern]
pub fn get_replies(post_hash: ActionHash) -> ExternResult<Vec<PostSummary>> {
    let links = get_links(
        GetLinksInputBuilder::try_new(post_hash, LinkTypes::PostToReplies)?.build()
    )?;

    let now_ms = sys_time()?.as_millis() as i64;
    let mut replies = Vec::new();

    for link in &links {
        if let Some(hash) = link.target.clone().into_action_hash() {
            if let Some(record) = get(hash.clone(), GetOptions::default())? {
                if let Ok(Some(bp)) = record.entry().to_app_option::<BlackboardPost>() {
                    let created_ms = link.timestamp.as_millis() as i64;
                    let ttl_ms = bp.ttl_hours as i64 * 3_600_000;
                    if now_ms - created_ms < ttl_ms {
                        replies.push(PostSummary {
                            hash,
                            prefix: bp.prefix,
                            content: bp.content,
                            tags: bp.tags,
                            ttl_hours: bp.ttl_hours,
                            reply_to: bp.reply_to,
                            author: record.action().author().clone().to_string(),
                            created_at_ms: created_ms,
                        });
                    }
                }
            }
        }
    }

    Ok(replies)
}

/// Posts de l'agent courant
#[hdk_extern]
pub fn my_posts(_: ()) -> ExternResult<Vec<PostSummary>> {
    let agent = agent_info()?.agent_latest_pubkey;
    let links = get_links(
        GetLinksInputBuilder::try_new(agent, LinkTypes::AgentToPosts)?.build()
    )?;

    let now_ms = sys_time()?.as_millis() as i64;
    let mut posts = Vec::new();

    for link in links.iter().rev().take(POSTS_PER_PAGE) {
        if let Some(hash) = link.target.clone().into_action_hash() {
            if let Some(record) = get(hash.clone(), GetOptions::default())? {
                if let Ok(Some(bp)) = record.entry().to_app_option::<BlackboardPost>() {
                    let created_ms = link.timestamp.as_millis() as i64;
                    let ttl_ms = bp.ttl_hours as i64 * 3_600_000;
                    if now_ms - created_ms < ttl_ms {
                        posts.push(PostSummary {
                            hash,
                            prefix: bp.prefix,
                            content: bp.content,
                            tags: bp.tags,
                            ttl_hours: bp.ttl_hours,
                            reply_to: bp.reply_to,
                            author: record.action().author().clone().to_string(),
                            created_at_ms: created_ms,
                        });
                    }
                }
            }
        }
    }

    Ok(posts)
}

/// Recherche full-text + filtre préfixe (scan sur timeline récente)
#[hdk_extern]
pub fn search(input: SearchInput) -> ExternResult<Vec<PostSummary>> {
    let all = get_recent_posts(())?;
    let terms: Vec<String> = input.terms.iter()
        .map(|t| t.to_lowercase())
        .collect();

    let results = all.into_iter().filter(|p| {
        // Filtre préfixe
        if let Some(ref pf) = input.prefix_filter {
            if p.prefix.to_uppercase() != pf.to_uppercase() {
                return false;
            }
        }
        // Filtre termes (OR sur contenu + tags)
        if terms.is_empty() {
            return true;
        }
        let haystack = format!("{} {}", p.content.to_lowercase(),
                               p.tags.join(" ").to_lowercase());
        terms.iter().any(|t| haystack.contains(t.as_str()))
    })
    .take(input.limit.unwrap_or(20) as usize)
    .collect();

    Ok(results)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn anchor_for_timeline() -> ExternResult<EntryHash> {
    anchor(TIMELINE_ANCHOR, "")
}

/// Strip patterns PII du contenu (défensif, la validation integrity est la vraie gate)
fn strip_pii(content: &str) -> String {
    let mut out = content.to_string();
    let replacements: &[(&str, &str)] = &[
        ("api_key=",   "api_key=[REDACTED]"),
        ("secret=",    "secret=[REDACTED]"),
        ("token=",     "token=[REDACTED]"),
        ("Bearer ",    "Bearer [REDACTED]"),
        ("password",   "[REDACTED]"),
        ("ssh-rsa",    "[REDACTED_KEY]"),
        ("/home/",     "/[REDACTED]/"),
        ("C:\\Users\\","[REDACTED]\\"),
    ];
    for (pat, repl) in replacements {
        // Case-insensitive replacement
        let lower = out.to_lowercase();
        let pat_lower = pat.to_lowercase();
        if let Some(idx) = lower.find(&pat_lower) {
            out = format!("{}{}{}", &out[..idx], repl, &out[idx + pat.len()..]);
        }
    }
    out
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
pub struct PostInput {
    pub prefix: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub ttl_hours: Option<u32>,
    pub reply_to: Option<ActionHash>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchInput {
    pub terms: Vec<String>,
    pub prefix_filter: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PostSummary {
    pub hash: ActionHash,
    pub prefix: String,
    pub content: String,
    pub tags: Vec<String>,
    pub ttl_hours: u32,
    pub reply_to: Option<ActionHash>,
    pub author: String,
    pub created_at_ms: i64,
}
