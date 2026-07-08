/// Estime la VRAM nécessaire (en MB) pour charger un modèle.
///
/// Formule approximative :
/// - Poids du modèle sur GPU
/// - KV-cache (2 * layers * context * hidden_size * bytes_per_element * parallel)
/// - Activations + overhead
pub fn estimate_model_vram_mb(
    model_size_gb: f32,
    n_layers: u32,
    context_size: u32,
    hidden_size: u32,
    n_gpu_layers: i32,
    kv_bytes: u32,        // 2 pour f16, 1 pour q8_0, 0.5 pour q4_0
    parallel: u32,
) -> f32 {
    let gpu_layers = if n_gpu_layers < 0 {
        n_layers as f32
    } else {
        n_gpu_layers as f32
    };

    // Poids du modèle sur GPU
    let model_weights_mb = model_size_gb * 1024.0 * (gpu_layers / n_layers as f32);

    // KV-cache estimation (très approximative)
    let kv_cache_mb = (n_layers as f32)
        * (context_size as f32)
        * (hidden_size as f32)
        * (kv_bytes as f32)
        * 2.0 // K + V
        * (parallel as f32)
        / 1024.0 / 1024.0;

    // Overhead (activations, CUDA context, etc.)
    let overhead_mb = 512.0 + (parallel as f32 * 128.0);

    model_weights_mb + kv_cache_mb + overhead_mb
}

/// Version simplifiée qui utilise des valeurs par défaut raisonnables
pub fn estimate_vram_simple(
    model_size_gb: f32,
    context_size: u32,
    n_gpu_layers: i32,
) -> f32 {
    // Hypothèses raisonnables pour un modèle type 7B-13B
    estimate_model_vram_mb(
        model_size_gb,
        32,           // ~32 layers
        context_size,
        4096,         // hidden size typique
        n_gpu_layers,
        2,            // f16 KV-cache
        1,            // parallel=1
    )
}
