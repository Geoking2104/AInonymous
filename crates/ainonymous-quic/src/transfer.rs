/// Quantization symétrique dynamique INT8 d'un tenseur f32.
/// Retourne (données_quantisées, scale).
pub fn quantize_f32_to_i8(data: &[f32]) -> (Vec<i8>, f32) {
    if data.is_empty() {
        return (vec![], 1.0);
    }

    let mut min_val = f32::MAX;
    let mut max_val = f32::MIN;

    for &v in data {
        if v < min_val { min_val = v; }
        if v > max_val { max_val = v; }
    }

    let abs_max = min_val.abs().max(max_val.abs());
    let scale = if abs_max > 0.0 { abs_max / 127.0 } else { 1.0 };

    let quantized: Vec<i8> = data
        .iter()
        .map(|&v| {
            let q = (v / scale).round().clamp(-127.0, 127.0) as i8;
            q
        })
        .collect();

    (quantized, scale)
}

/// Déquantization INT8 → f32
pub fn dequantize_i8_to_f32(data: &[i8], scale: f32) -> Vec<f32> {
    data.iter().map(|&q| q as f32 * scale).collect()
}
