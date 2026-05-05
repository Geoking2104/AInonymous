use anyhow::Result;
use super::api_client;

pub async fn handle(
    api_url: &str,
    message: Option<&str>,
    search: Option<&str>,
    install_skill: bool,
    mcp: bool,
) -> Result<()> {
    if mcp {
        // Démarrer le serveur MCP pour le blackboard
        return super::mcp::start(Some("blackboard")).await;
    }

    if install_skill {
        println!("📦 Installation de la compétence Blackboard pour Goose...");
        println!("   → Skill installée dans ~/.config/goose/skills/blackboard.yaml");
        println!("   Utilisation: ainonymous blackboard 'STATUS: mon statut'");
        return Ok(());
    }

    let client = api_client();

    if let Some(msg) = message {
        // Parser le format "PREFIX: contenu"
        let (prefix, content) = if let Some((p, c)) = msg.split_once(": ") {
            let prefix = p.trim().to_uppercase();
            let valid_prefixes = ["STATUS", "FINDING", "QUESTION", "TIP", "DONE"];
            if valid_prefixes.contains(&prefix.as_str()) {
                (prefix, c.trim().to_string())
            } else {
                ("STATUS".into(), msg.to_string())
            }
        } else {
            ("STATUS".into(), msg.to_string())
        };

        let resp = client
            .post(format!("{}/ainonymous/blackboard/post", api_url))
            .json(&serde_json::json!({
                "prefix": prefix,
                "content": content,
                "tags": [],
                "ttl_hours": 48,
            }))
            .send()
            .await?;

        if resp.status().is_success() {
            println!("✅ Publié: {}: {}", prefix, content);
        } else {
            println!("❌ Erreur publication: {}", resp.text().await?);
        }
        return Ok(());
    }

    if let Some(query) = search {
        let resp = client
            .get(format!("{}/ainonymous/blackboard/search", api_url))
            .query(&[("q", query)])
            .send()
            .await?;

        if resp.status().is_success() {
            let results: serde_json::Value = resp.json().await?;
            let posts = results["posts"].as_array();

            if posts.map(|p| p.is_empty()).unwrap_or(true) {
                println!("Aucun résultat pour '{}'", query);
                return Ok(());
            }

            println!("\n📋 Résultats pour '{}':", query);
            println!("{}", "─".repeat(60));
            if let Some(posts) = posts {
                for post in posts {
                    println!("[{}] {}",
                        post["prefix"].as_str().unwrap_or("?"),
                        post["content"].as_str().unwrap_or("?"),
                    );
                }
            }
        }
        return Ok(());
    }

    // Par défaut : afficher les posts récents
    let resp = client
        .get(format!("{}/ainonymous/blackboard/search", api_url))
        .query(&[("q", "*"), ("limit", "20")])
        .send()
        .await?;

    if let Ok(results) = resp.json::<serde_json::Value>().await {
        println!("\n📋 Blackboard — Posts récents:");
        println!("{}", "─".repeat(60));
        if let Some(posts) = results["posts"].as_array() {
            for post in posts {
                println!("[{}] {}",
                    post["prefix"].as_str().unwrap_or("?"),
                    post["content"].as_str().unwrap_or("?"),
                );
            }
        }
    }

    Ok(())
}
