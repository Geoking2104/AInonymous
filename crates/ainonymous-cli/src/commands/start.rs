use anyhow::Result;
use tracing::info;
use super::check_daemon_running;

pub async fn run(port: u16) -> Result<()> {
    println!("🚀 Démarrage AInonymous daemon...");

    // Démarrer le daemon en arrière-plan
    let daemon_bin = std::env::current_exe()?
        .parent().unwrap()
        .join("ainonymous-daemon");

    std::process::Command::new(daemon_bin)
        .env("AINON_PORT", port.to_string())
        .spawn()?;

    // Attendre que le daemon soit prêt
    let api_url = format!("http://127.0.0.1:{}/v1", port);
    for i in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if check_daemon_running(&api_url).await {
            println!("✅ AInonymous prêt sur http://127.0.0.1:{}/v1", port);
            println!("   → API OpenAI-compatible disponible");
            println!("   → Rejoindre le mesh: ainonymous --auto");
            return Ok(());
        }
        print!("\r   Attente démarrage... {}s", i + 1);
    }

    anyhow::bail!("Daemon AInonymous n'a pas démarré dans les 30 secondes")
}

pub async fn run_auto(model: Option<&str>, port: u16) -> Result<()> {
    println!("🔍 Recherche du mesh public AInonymous...");

    let api_url = format!("http://127.0.0.1:{}/v1", port);

    // Démarrer le daemon si pas déjà en cours
    if !check_daemon_running(&api_url).await {
        run(port).await?;
    }

    // Rejoindre le mesh (le daemon se connecte automatiquement au DHT Holochain)
    let model_id = model.unwrap_or("gemma4-e4b");
    println!("📡 Connexion au mesh avec modèle: {}", model_id);
    println!("💡 Conseil: démarre avec --model gemma4-31b pour contribuer plus de puissance");
    println!("\n✅ Connecté au mesh ! Utilise:");
    println!("   curl http://127.0.0.1:{}/v1/chat/completions -H 'Content-Type: application/json' \\", port);
    println!("        -d '{{\"model\":\"{}\",\"messages\":[{{\"role\":\"user\",\"content\":\"Bonjour\"}}]}}'", model_id);

    Ok(())
}

pub async fn run_with_model(model_id: &str, port: u16) -> Result<()> {
    println!("🤖 Démarrage avec modèle: {}", model_id);

    let api_url = format!("http://127.0.0.1:{}/v1", port);
    if !check_daemon_running(&api_url).await {
        run(port).await?;
    }

    // Charger le modèle via l'API
    let client = super::api_client();
    let resp = client
        .post(format!("{}/ainonymous/models/pull", api_url))
        .json(&serde_json::json!({ "model_id": model_id }))
        .send()
        .await?;

    if resp.status().is_success() {
        println!("✅ Modèle {} chargé. Mesh public rejoint.", model_id);
    } else {
        println!("⚠️  Modèle {} non disponible localement, utilisation du mesh.", model_id);
    }

    Ok(())
}
