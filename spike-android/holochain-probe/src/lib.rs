//! Sonde S1 : conducteur Holochain 0.7.0 embarqué (lair in-process), compilé
//! pour `aarch64-linux-android` avec le seul NDK 27 (sans Nix).
//!
//! [`start_conductor`] démarre un conducteur persistant dans `data_dir` et
//! renvoie la durée de démarrage. Le même code tourne sur l'hôte Linux
//! (`cargo run --release --example start`), dans l'APK Tauri (feature
//! `holochain` de `tauri-min`) et, via le symbole C, depuis du code JNI.
use holochain::conductor::{config::ConductorConfig, Conductor};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, serde::Serialize)]
pub struct StartReport {
    pub conductor_start_ms: u128,
    pub data_dir: String,
    pub holochain_version: &'static str,
}

/// Démarre un conducteur (lair in-process, données dans `data_dir`), le garde
/// actif `keep_alive`, puis l'arrête.
pub async fn start_conductor(data_dir: &Path, passphrase: &[u8], keep_alive: Duration) -> Result<StartReport, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let config = ConductorConfig {
        data_root_path: Some(PathBuf::from(data_dir).into()),
        ..ConductorConfig::default()
    };
    let pass = Arc::new(Mutex::new(sodoken::LockedArray::from(passphrase.to_vec())));
    let t = Instant::now();
    let handle = Conductor::builder()
        .config(config)
        .passphrase(Some(pass))
        .build()
        .await
        .map_err(|e| format!("démarrage du conducteur : {e}"))?;
    let ms = t.elapsed().as_millis();
    tokio::time::sleep(keep_alive).await;
    let _ = handle.shutdown().await;
    Ok(StartReport { conductor_start_ms: ms, data_dir: data_dir.display().to_string(), holochain_version: "0.7.0" })
}

/// Point d'entrée C (JNI ou test natif) : 0 = conducteur démarré puis arrêté.
///
/// # Safety
/// `data_dir` doit pointer vers une chaîne C UTF-8 valide terminée par NUL.
#[no_mangle]
pub unsafe extern "C" fn relay_holochain_probe_start(data_dir: *const std::os::raw::c_char) -> i64 {
    let dir = match unsafe { std::ffi::CStr::from_ptr(data_dir) }.to_str() {
        Ok(d) => PathBuf::from(d),
        Err(_) => return -2,
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return -1,
    };
    match rt.block_on(start_conductor(&dir, b"spike-s1-test-passphrase", Duration::ZERO)) {
        Ok(r) => r.conductor_start_ms as i64,
        Err(_) => -3,
    }
}
