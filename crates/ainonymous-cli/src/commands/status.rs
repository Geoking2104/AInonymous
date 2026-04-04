use anyhow::Result;
use super::api_client;

pub async fn show(api_url: &str) -> Result<()> {
    let client = api_client();

    // Statut mesh
    match client
        .get(format!("{}/ainonymous/mesh/status", api_url))
        .send().await
    {
        Ok(resp) if resp.status().is_success() => {
            let status: serde_json::Value = resp.json().await?;

            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("  AInonymous — Statut du mesh");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

            if let Some(local) = status.get("local_node") {
                println!("\n📍 Nœud local");
                println!("   Agent:  {}", local["agent_id"].as_str().unwrap_or("?"));
                println!("   Status: {}", local["status"].as_str().unwrap_or("?"));
                println!("   Load:   {:.0}%", local["current_load"].as_f64().unwrap_or(0.0) * 100.0);
            }

            if let Some(mesh) = status.get("mesh") {
                println!("\n🌐 Mesh");
                println!("   Pairs connectés: {}", mesh["peers_connected"].as_u64().unwrap_or(0));
                println!("   Pairs actifs:    {}", mesh["peers_active"].as_u64().unwrap_or(0));
                println!("   VRAM totale:     {:.1} GB", mesh["total_vram_gb"].as_f64().unwrap_or(0.0));
                println!("   Latence moy.:    {} ms", mesh["avg_latency_ms"].as_u64().unwrap_or(0));
            }

            if let Some(bb) = status.get("blackboard") {
                println!("\n📋 Blackboard");
                println!("   Posts (24h):  {}", bb["posts_last_24h"].as_u64().unwrap_or(0));
                println!("   Agents actifs: {}", bb["agents_active"].as_u64().unwrap_or(0));
            }
        }
        _ => {
            println!("⚠️  Daemon AInonymous non disponible.");
            println!("    Démarrer avec: ainonymous start");
        }
    }

    // Liste des modèles
    if let Ok(resp) = client.get(format!("{}/models", api_url)).send().await {
        if let Ok(models) = resp.json::<serde_json::Value>().await {
            println!("\n📦 Modèles disponibles");
            println!("   {:<20} {:<10} {:<10} {}", "ID", "VRAM", "Contexte", "Nœuds");
            println!("   {}", "─".repeat(55));
            if let Some(data) = models["data"].as_array() {
                for m in data {
                    println!("   {:<20} {:<10} {:<10} {}",
                        m["id"].as_str().unwrap_or("?"),
                        format!("{:.0}GB", m["meta"]["vram_required_gb"].as_f64().unwrap_or(0.0)),
                        format!("{}K", m["meta"]["context_length"].as_u64().unwrap_or(0) / 1000),
                        m["meta"]["nodes_available"].as_u64().unwrap_or(0),
                    );
                }
            }
        }
    }

    println!();
    Ok(())
}
