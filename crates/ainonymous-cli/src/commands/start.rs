use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use super::check_daemon_running;

/// Délai max d'attente du daemon (connexion au conducteur Holochain incluse).
const DAEMON_WAIT_SECS: u64 = 60;
/// Délai max d'attente du proxy OpenAI-compatible.
const PROXY_WAIT_SECS: u64 = 30;

/// Démarre la pile locale : `ainonymous-daemon` (API HTTP, `AINON_DAEMON_URL`,
/// défaut `http://127.0.0.1:8890`) puis `ainonymous-proxy` (API OpenAI sur
/// `port`), en attendant que chacun réponde sur `/health`. Un composant déjà
/// en cours d'exécution est réutilisé. Le conducteur Holochain n'est pas lancé
/// ici (voir docs/JOIN_NETWORK.md).
pub async fn run(port: u16) -> Result<()> {
    let daemon_url = ainonymous_types::endpoints::daemon_url_from_env().url;
    let proxy_url = format!("http://127.0.0.1:{}", port);

    // 1. Daemon (hérite de AINON_CONFIG si défini)
    if health_ok(&daemon_url).await {
        println!("✓ Daemon déjà actif sur {}", daemon_url);
    } else {
        println!("🚀 Démarrage ainonymous-daemon (attendu sur {})...", daemon_url);
        let child = Command::new(sibling_binary("ainonymous-daemon")?).spawn()
            .context("lancement de ainonymous-daemon")?;
        wait_for_health("ainonymous-daemon", &daemon_url, child, DAEMON_WAIT_SECS).await?;
        println!("✓ Daemon prêt sur {}", daemon_url);
    }

    // 2. Proxy OpenAI-compatible, branché sur ce daemon
    if health_ok(&proxy_url).await {
        println!("✓ Proxy déjà actif sur {}", proxy_url);
    } else {
        println!("🚀 Démarrage ainonymous-proxy sur {}...", proxy_url);
        let child = Command::new(sibling_binary("ainonymous-proxy")?)
            .env("AINON_PORT", port.to_string())
            .env(ainonymous_types::endpoints::DAEMON_URL_ENV, &daemon_url)
            .spawn()
            .context("lancement de ainonymous-proxy")?;
        wait_for_health("ainonymous-proxy", &proxy_url, child, PROXY_WAIT_SECS).await?;
    }

    println!("✅ AInonymous prêt sur {}/v1", proxy_url);
    println!("   → API OpenAI-compatible disponible");
    println!("   → Rejoindre le mesh: ainonymous --auto");
    Ok(())
}

/// Binaire installé à côté de `ainonymous` (même répertoire `target/...` ou `bin/`).
fn sibling_binary(name: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().context("répertoire de l'exécutable introuvable")?;
    let path = sibling_path(dir, name);
    if !path.exists() {
        anyhow::bail!(
            "{} introuvable ({}). Compile-le avec `cargo build --workspace`.",
            name,
            path.display()
        );
    }
    Ok(path)
}

fn sibling_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX))
}

async fn health_ok(base_url: &str) -> bool {
    check_daemon_running(&format!("{}/v1", base_url.trim_end_matches('/'))).await
}

/// Attend que `base_url/health` réponde ; échoue tout de suite si le processus
/// s'arrête (config invalide, port pris…) au lieu d'attendre le délai complet.
async fn wait_for_health(name: &str, base_url: &str, mut child: Child, secs: u64) -> Result<()> {
    for i in 0..secs {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("{} s'est arrêté au démarrage ({})", name, status);
        }
        if health_ok(base_url).await {
            println!();
            return Ok(());
        }
        print!("\r   Attente {}... {}s", name, i + 1);
        let _ = std::io::stdout().flush();
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    anyhow::bail!("{} n'a pas répondu sur {}/health en {} s", name, base_url, secs)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_path_is_in_same_dir() {
        let p = sibling_path(Path::new("/opt/ainon/bin"), "ainonymous-proxy");
        assert_eq!(p.parent(), Some(Path::new("/opt/ainon/bin")));
        assert!(p.file_name().unwrap().to_string_lossy().starts_with("ainonymous-proxy"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn wait_fails_fast_when_process_exits() {
        let child = Command::new("false").spawn().unwrap();
        let t0 = std::time::Instant::now();
        let err = wait_for_health("test", "http://127.0.0.1:1", child, 20).await.unwrap_err();
        assert!(err.to_string().contains("arrêté"), "{err}");
        assert!(t0.elapsed().as_secs() < 10);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn wait_times_out_when_nothing_listens() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let err = wait_for_health("test", "http://127.0.0.1:1", child, 1).await.unwrap_err();
        assert!(err.to_string().contains("n'a pas répondu"), "{err}");
    }
}
