//! Démarrage réel du conducteur sur l'hôte : `cargo run --release --example start -- <dir>`.
#[tokio::main]
async fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/relay-s1-conductor".into());
    let cold = !std::path::Path::new(&dir).exists();
    match holochain_android_probe::start_conductor(dir.as_ref(), b"spike-s1-test-passphrase", std::time::Duration::from_secs(2)).await {
        Ok(r) => println!("{}", serde_json::json!({"ok": true, "premier_demarrage": cold, "report": r})),
        Err(e) => { println!("{}", serde_json::json!({"ok": false, "error": e})); std::process::exit(1) }
    }
}
