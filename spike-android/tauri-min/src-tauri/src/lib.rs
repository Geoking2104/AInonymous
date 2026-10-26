use std::time::Instant;
use tauri::Manager;

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

#[derive(serde::Serialize)]
struct StartupInfo {
    os: &'static str,
    arch: &'static str,
    ms_since_rust_start: u128,
    holochain_embedded: bool,
}

#[tauri::command]
fn startup_info() -> StartupInfo {
    let info = StartupInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        ms_since_rust_start: START.get().map(|s| s.elapsed().as_millis()).unwrap_or(0),
        holochain_embedded: cfg!(feature = "holochain"),
    };
    log::info!("S1 startup_info ms_since_rust_start={}", info.ms_since_rust_start);
    info
}

/// Démarre le conducteur Holochain embarqué dans le dossier de données de l'app.
#[tauri::command]
async fn holochain_start(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    #[cfg(feature = "holochain")]
    {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("conductor");
        let r = holochain_android_probe::start_conductor(&dir, b"spike-s1-test-passphrase", std::time::Duration::from_secs(1)).await?;
        log::info!("S1 conductor_start_ms={}", r.conductor_start_ms);
        serde_json::to_value(r).map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "holochain"))]
    {
        let _ = app;
        Err("APK construit sans la feature `holochain`".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = START.set(Instant::now());
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![startup_info, holochain_start])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Tauri");
}
