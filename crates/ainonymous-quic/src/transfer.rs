/// Quantization asymétrique dynamique en UINT8 (0-255).
/// Retourne (données_quantisées_u8, scale, zero_point).
pub fn quantize_f32_to_u8_asymmetric(data: &[f32]) -> (Vec<u8>, f32, u8) {
    if data.is_empty() {
        return (vec![], 1.0, 0);
    }

    let mut min_val = f32::MAX;
    let mut max_val = f32::MIN;

    for &v in data {
        if v < min_val { min_val = v; }
        if v > max_val { max_val = v; }
    }

    if (max_val - min_val).abs() < 1e-8 {
        // Tenseur presque constant
        return (vec![128u8; data.len()], 1.0, 128);
    }

    let scale = (max_val - min_val) / 255.0;
    let zero_point = ((-min_val) / scale).round().clamp(0.0, 255.0) as u8;

    let quantized: Vec<u8> = data
        .iter()
        .map(|&v| {
            let q = ((v / scale) + zero_point as f32).round().clamp(0.0, 255.0) as u8;
            q
        })
        .collect();

    (quantized, scale, zero_point)
}

/// Déquantization UINT8 asymétrique → f32
pub fn dequantize_u8_to_f32(data: &[u8], scale: f32, zero_point: u8) -> Vec<f32> {
    data.iter()
        .map(|&q| (q as f32 - zero_point as f32) * scale)
        .collect()
}
