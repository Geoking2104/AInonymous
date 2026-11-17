use anyhow::Result;
use super::api_client;

pub async fn list(api_url: &str, model: Option<&str>) -> Result<()> {
    let client = api_client();
    let mut req = client.get(format!("{}/ainonymous/mesh/nodes", api_url));
    if let Some(m) = model { req = req.query(&[("model_id", m)]); }

    let resp = req.send().await?;
    let data: serde_json::Value = resp.json().await?;

    println!("\n🌐 Nœuds du mesh{}:", model.map(|m| format!(" (modèle: {})", m)).unwrap_or_default());
    println!("   {:<16} {:<8} {:<8} {:<10} Région", "Agent (tronqué)", "Slots", "VRAM", "Load");
    println!("   {}", "─".repeat(60));

    if let Some(nodes) = data["nodes"].as_array() {
        if nodes.is_empty() {
            println!("   Aucun nœud disponible");
        }
        for n in nodes {
            let agent = n["agent_id"].as_str().unwrap_or("?");
            let short_agent = if agent.len() > 14 { &agent[..14] } else { agent };
            // Champs de `ainonymous_types::NodeSummary` (réponse du daemon)
            println!("   {:<16} {:<8} {:<8} {:<10} {}",
                short_agent,
                n["available_slots"].as_u64().unwrap_or(0),
                format!("{:.0}GB", n["vram_gb"].as_f64().unwrap_or(0.0)),
                format!("{:.0}%", n["current_load"].as_f64().unwrap_or(0.0) * 100.0),
                n["region_hint"].as_str().unwrap_or("unknown"),
            );
        }
    }
    println!();
    Ok(())
}
