/// Vérifie si le modèle peut être chargé sans dépasser la VRAM disponible.
pub fn can_load_model_safely(
    model_size_gb: f32,
    context_size: u32,
    n_gpu_layers: i32,
    available_vram_gb: f32,
) -> bool {
    let estimated = estimate_vram_simple(model_size_gb, context_size, n_gpu_layers) / 1024.0;
    estimated <= available_vram_gb * 0.9 // marge de sécurité de 10%
}

/// Version qui utilise la VRAM détectée localement
pub fn can_load_model_with_local_gpu(
    model_size_gb: f32,
    context_size: u32,
    n_gpu_layers: i32,
) -> bool {
    let caps = detect_local_capabilities_from_config(&DaemonConfig::default()); // fallback
    can_load_model_safely(model_size_gb, context_size, n_gpu_layers, caps.vram_gb)
}
