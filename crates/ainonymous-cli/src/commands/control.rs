use anyhow::Result;

pub async fn stop(api_url: &str) -> Result<()> {
    let base = api_url.trim_end_matches("/v1");
    match reqwest::get(format!("{}/shutdown", base)).await {
        Ok(_) => println!("✅ AInonymous daemon arrêté"),
        Err(_) => println!("⚠️  Daemon non disponible ou déjà arrêté"),
    }
    Ok(())
}
