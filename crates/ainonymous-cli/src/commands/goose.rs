use anyhow::Result;

pub async fn launch(api_url: &str, profile: &str, team: bool, agents: u8) -> Result<()> {
    println!("🪿 Lancement de Goose avec le mesh AInonymous...");

    let model = match profile {
        "fast"     => "gemma4-e4b",
        "standard" => "gemma4-26b-moe",
        "powerful" => "gemma4-31b",
        other      => other,
    };

    println!("   Modèle: {} (profil: {})", model, profile);

    // Construire la config Goose
    let config = build_goose_config(model, api_url);
    let config_path = write_temp_goose_config(&config)?;

    if team {
        println!("   Mode équipe: {} agents", agents);
        launch_goose_team(&config_path, agents).await
    } else {
        launch_goose_single(&config_path).await
    }
}

fn build_goose_config(model: &str, api_url: &str) -> String {
    let base_url = api_url.trim_end_matches("/v1");
    format!(r#"
# Config Goose — AInonymous (générée automatiquement)
provider: openai-compatible
model: {model}
base_url: {base_url}/v1
api_key: "ainonymous-local"

extensions:
  - name: ainonymous-mesh
    type: stdio
    cmd: ainonymous
    args: ["mcp"]
    description: "Accès aux capacités du mesh AInonymous"
  - name: ainonymous-blackboard
    type: stdio
    cmd: ainonymous
    args: ["mcp", "--dna", "blackboard"]
    description: "Blackboard partagé pour collaboration d'agents"
"#, model = model, base_url = base_url)
}

fn write_temp_goose_config(config: &str) -> Result<std::path::PathBuf> {
    let path = dirs::config_dir()
        .unwrap_or_default()
        .join("goose")
        .join("ainonymous-profile.yaml");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, config)?;
    Ok(path)
}

async fn launch_goose_single(config_path: &std::path::Path) -> Result<()> {
    println!("   Démarrage Goose...\n");

    let status = std::process::Command::new("goose")
        .args(["--config", config_path.to_str().unwrap()])
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => anyhow::bail!("Goose s'est terminé avec une erreur"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("⚠️  Goose non trouvé. Installation:");
            println!("   cargo install goose");
            println!("   ou: https://github.com/block/goose");
            anyhow::bail!("Goose non installé")
        }
        Err(e) => anyhow::bail!("Erreur lancement Goose: {}", e),
    }
}

async fn launch_goose_team(config_path: &std::path::Path, agents: u8) -> Result<()> {
    println!("   Démarrage équipe de {} agents Goose...\n", agents);

    // Lancer N instances Goose en parallèle
    let mut handles = vec![];
    for i in 0..agents {
        let config = config_path.to_path_buf();
        let handle = tokio::spawn(async move {
            println!("   Agent {} démarré", i + 1);
            let _ = std::process::Command::new("goose")
                .args(["--config", config.to_str().unwrap()])
                .arg(format!("--agent-id={}", i + 1))
                .status();
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
    Ok(())
}
