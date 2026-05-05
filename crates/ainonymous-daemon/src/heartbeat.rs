use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

use ainonymous_types::NodeHeartbeat;
use crate::{DaemonConfig, holochain::HolochainClient};

/// Boucle de heartbeat — publie l'état du nœud dans le DHT toutes les 30s
pub async fn run_heartbeat(holochain: HolochainClient, config: DaemonConfig) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        let hb = collect_heartbeat(&config).await;
        match holochain.send_heartbeat(hb).await {
            Ok(()) => debug!("Heartbeat publié"),
            Err(e) => warn!("Heartbeat échoué: {}", e),
        }
    }
}

async fn collect_heartbeat(config: &DaemonConfig) -> NodeHeartbeat {
    let (load, pressure) = measure_system_load().await;
    let slots = measure_available_slots(config).await;

    NodeHeartbeat {
        agent_id: "local".into(),
        current_load: load,
        available_slots: slots,
        queue_depth: 0, // TODO: récupérer depuis le proxy
        memory_pressure: pressure,
        temperature_c: read_gpu_temperature(),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
    }
}

async fn measure_system_load() -> (f32, f32) {
    // Lecture /proc/loadavg sur Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/loadavg") {
            if let Some(load_1m) = content.split_whitespace().next() {
                if let Ok(load) = load_1m.parse::<f32>() {
                    let ncpu = num_cpus();
                    let normalized = (load / ncpu as f32).min(1.0);
                    let pressure = memory_pressure_linux();
                    return (normalized, pressure);
                }
            }
        }
    }
    // Fallback : load faible
    (0.1, 0.1)
}

async fn measure_available_slots(config: &DaemonConfig) -> u8 {
    // TODO: interroger le proxy pour les slots disponibles
    config.max_concurrent_requests
}

fn read_gpu_temperature() -> Option<f32> {
    // Linux NVIDIA : /sys/class/hwmon
    #[cfg(target_os = "linux")]
    {
        // Chercher un fichier temp1_input dans les hwmon
        for i in 0..10 {
            let path = format!("/sys/class/hwmon/hwmon{}/temp1_input", i);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(millideg) = content.trim().parse::<i32>() {
                    return Some(millideg as f32 / 1000.0);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn memory_pressure_linux() -> f32 {
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        let mut total = 0u64;
        let mut available = 0u64;
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[0] {
                    "MemTotal:" => total = parts[1].parse().unwrap_or(0),
                    "MemAvailable:" => available = parts[1].parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
        if total > 0 {
            return 1.0 - (available as f32 / total as f32);
        }
    }
    0.5
}

#[cfg(not(target_os = "linux"))]
fn memory_pressure_linux() -> f32 { 0.3 }

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
