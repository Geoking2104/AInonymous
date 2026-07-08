use wide::f32x8;

/// Version SIMD (f32x8) de la quantization symétrique INT8.
/// Plus rapide sur les CPUs supportant AVX2.
pub fn quantize_f32_to_i8(data: &[f32]) -> (Vec<i8>, f32) {
    if data.is_empty() {
        return (vec![], 1.0);
    }

    // Trouver min/max avec SIMD quand possible
    let mut min_val = f32::MAX;
    let mut max_val = f32::MIN;

    let mut i = 0;
    while i + 8 <= data.len() {
        let v = f32x8::from(&data[i..i + 8]);
        min_val = min_val.min(v.reduce_min());
        max_val = max_val.max(v.reduce_max());
        i += 8;
    }

    // Reste scalaire
    for &v in &data[i..] {
        if v < min_val { min_val = v; }
        if v > max_val { max_val = v; }
    }

    let abs_max = min_val.abs().max(max_val.abs());
    if abs_max < 1e-8 {
        return (vec![0i8; data.len()], 1.0);
    }

    let scale = abs_max / 127.0;
    let inv_scale = 1.0 / scale;

    let mut quantized = Vec::with_capacity(data.len());

    // Partie SIMD
    i = 0;
    while i + 8 <= data.len() {
        let v = f32x8::from(&data[i..i + 8]);
        let scaled = v * f32x8::splat(inv_scale);
        let clamped = scaled.max(f32x8::splat(-127.0)).min(f32x8::splat(127.0));
        let rounded = clamped.round();

        for j in 0..8 {
            quantized.push(rounded.as_array_ref()[j] as i8);
        }
        i += 8;
    }

    // Reste scalaire
    for &v in &data[i..] {
        let q = (v * inv_scale).round().clamp(-127.0, 127.0) as i8;
        quantized.push(q);
    }

    (quantized, scale)
}

/// Déquantization INT8 → f32 (version scalaire, suffisamment rapide)
pub fn dequantize_i8_to_f32(data: &[i8], scale: f32) -> Vec<f32> {
    data.iter().map(|&q| q as f32 * scale).collect()
}
