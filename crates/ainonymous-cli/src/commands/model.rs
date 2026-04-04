use anyhow::Result;
use super::api_client;
use crate::ModelAction;

pub async fn handle(action: ModelAction, api_url: &str) -> Result<()> {
    let client = api_client();

    match action {
        ModelAction::List => {
            let resp = client.get(format!("{}/models", api_url)).send().await?;
            let models: serde_json::Value = resp.json().await?;

            println!("\n📦 Modèles disponibles:");
            println!("   {:<22} {:<8} {:<10} {:<12} {}",
                "ID", "VRAM", "Contexte", "Archi", "Multimodal");
            println!("   {}", "─".repeat(65));

            if let Some(data) = models["data"].as_array() {
                for m in data {
                    let meta = &m["meta"];
                    println!("   {:<22} {:<8} {:<10} {:<12} {}",
                        m["id"].as_str().unwrap_or("?"),
                        format!("{:.0}GB", meta["vram_required_gb"].as_f64().unwrap_or(0.0)),
                        format!("{}K", meta["context_length"].as_u64().unwrap_or(0) / 1000),
                        meta["architecture"].as_str().unwrap_or("?"),
                        if meta["multimodal"].as_bool().unwrap_or(false) { "✓" } else { "✗" },
                    );
                }
            }
            println!();
        }

        ModelAction::Pull { model_id, quant } => {
            println!("⬇️  Téléchargement {} ({})", model_id, quant);
            let resp = client
                .post(format!("{}/ainonymous/models/pull", api_url))
                .json(&serde_json::json!({
                    "model_id": model_id,
                    "quantization": quant,
                    "source": "huggingface"
                }))
                .send()
                .await?;

            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await?;
                println!("✅ Téléchargement démarré (job: {})", body["job_id"].as_str().unwrap_or("?"));
                println!("   Suivi: ainonymous status");
            } else {
                println!("❌ Erreur: {}", resp.text().await?);
            }
        }

        ModelAction::Remove { model_id } => {
            let models_dir = dirs::home_dir().unwrap_or_default().join(".models");
            let path = models_dir.join(format!("{}.gguf", model_id));
            if path.exists() {
                std::fs::remove_file(&path)?;
                println!("✅ Modèle {} supprimé", model_id);
            } else {
                println!("⚠️  Modèle {} non trouvé dans {:?}", model_id, models_dir);
            }
        }

        ModelAction::Info { model_id } => {
            let models_dir = dirs::home_dir().unwrap_or_default().join(".models");
            let path = models_dir.join(format!("{}.gguf", model_id));
            if path.exists() {
                let size = std::fs::metadata(&path)?.len();
                println!("📊 Modèle: {}", model_id);
                println!("   Fichier: {:?}", path);
                println!("   Taille:  {:.1} GB", size as f64 / 1_073_741_824.0);
            } else {
                println!("⚠️  Modèle {} non disponible localement", model_id);
                println!("   Télécharger: ainonymous model pull {}", model_id);
            }
        }
    }

    Ok(())
}
