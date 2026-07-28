use std::time::Instant;
use anyhow::Result;
use bytes::Bytes;
use tracing::{debug, info};
use wide::f32x8;

use ainonymous_types::inference::{ActivationHeader, DType, GeneratedToken};
use crate::{QuicError, QuicSession, MAX_ACTIVATION_SIZE, COMPRESSION_THRESHOLD_BPS};

/// Transfert d'activations tensorielles via QUIC
pub struct ActivationTransfer;

impl ActivationTransfer {
    /// Envoyer un bloc d'activations vers le nœud suivant dans le pipeline
    pub async fn send(
        session: &QuicSession,
        header: ActivationHeader,
        activations: &[u8],
    ) -> Result<(), QuicError> {
        let start = Instant::now();
        let original_size = activations.len();

        // Décider de la compression selon la bande passante estimée
        let should_compress = session.config.compress
            || session.config.bandwidth_bps
                .map(|bw| bw < COMPRESSION_THRESHOLD_BPS)
                .unwrap_or(false);

        let (data, compressed) = if should_compress {
            let encoded = zstd::encode_all(activations, 1)
                .map_err(|e| QuicError::CompressionFailed(e.to_string()))?;
            debug!(
                "Activations compressées: {} → {} bytes ({:.0}%)",
                original_size, encoded.len(),
                (1.0 - encoded.len() as f32 / original_size as f32) * 100.0
            );
            (encoded, true)
        } else {
            (activations.to_vec(), false)
        };

        // Construire et envoyer le header (64 bytes)
        let mut final_header = header;
        final_header.compressed = compressed;
        let header_bytes = final_header.to_bytes();

        let mut stream = session.connection.open_uni().await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;

        // Header
        stream.write_all(&header_bytes).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;

        // Body (taille puis données)
        let size_bytes = (data.len() as u64).to_le_bytes();
        stream.write_all(&size_bytes).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        stream.write_all(&data).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        stream.finish()
            .map_err(|e| QuicError::StreamError(e.to_string()))?;

        let elapsed = start.elapsed();
        let throughput_mbps = original_size as f64 / elapsed.as_secs_f64() / 1_000_000.0;
        info!(
            "Activations envoyées: {} bytes en {:?} ({:.1} MB/s)",
            original_size, elapsed, throughput_mbps
        );

        Ok(())
    }

    /// Recevoir un bloc d'activations depuis le nœud précédent
    pub async fn receive(session: &QuicSession) -> Result<(ActivationHeader, Vec<u8>), QuicError> {
        let mut stream = session.connection.accept_uni().await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;

        // Lire header (64 bytes)
        let mut header_buf = [0u8; ActivationHeader::SIZE];
        read_exact_from_stream(&mut stream, &mut header_buf).await?;
        let header = ActivationHeader::from_bytes(&header_buf);

        // Lire taille du body
        let mut size_buf = [0u8; 8];
        read_exact_from_stream(&mut stream, &mut size_buf).await?;
        let body_size = u64::from_le_bytes(size_buf) as usize;

        if body_size > MAX_ACTIVATION_SIZE {
            return Err(QuicError::PayloadTooLarge(body_size, MAX_ACTIVATION_SIZE));
        }

        // Lire body
        let mut body = vec![0u8; body_size];
        let mut offset = 0;
        while offset < body_size {
            let chunk = stream.read_chunk(body_size - offset, true).await
                .map_err(|e| QuicError::StreamError(e.to_string()))?
                .ok_or(QuicError::StreamError("stream fermé prématurément".into()))?;
            let n = chunk.bytes.len();
            body[offset..offset + n].copy_from_slice(&chunk.bytes);
            offset += n;
        }

        // Décompresser si nécessaire
        let activations = if header.compressed {
            zstd::decode_all(&body[..])
                .map_err(|e| QuicError::DecompressionFailed(e.to_string()))?
        } else {
            body
        };

        Ok((header, activations))
    }
}

/// Stream de tokens en temps réel via QUIC
pub struct TokenStream {
    send_stream: Option<quinn::SendStream>,
    recv_stream: Option<quinn::RecvStream>,
}

impl TokenStream {
    /// Créer un stream d'émission de tokens (côté nœud final)
    pub async fn sender(session: &QuicSession) -> Result<Self, QuicError> {
        let stream = session.connection.open_uni().await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        Ok(Self { send_stream: Some(stream), recv_stream: None })
    }

    /// Créer un stream de réception de tokens (côté coordinateur)
    pub async fn receiver(session: &QuicSession) -> Result<Self, QuicError> {
        let stream = session.connection.accept_uni().await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        Ok(Self { send_stream: None, recv_stream: Some(stream) })
    }

    /// Envoyer un token généré
    pub async fn send_token(&mut self, token: &GeneratedToken) -> Result<(), QuicError> {
        let stream = self.send_stream.as_mut()
            .ok_or(QuicError::StreamError("pas de stream d'émission".into()))?;

        let data = serde_json::to_vec(token)
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        let len = (data.len() as u32).to_le_bytes();
        stream.write_all(&len).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        stream.write_all(&data).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        Ok(())
    }

    /// Recevoir le prochain token (retourne None si stream terminé)
    pub async fn recv_token(&mut self) -> Result<Option<GeneratedToken>, QuicError> {
        let stream = self.recv_stream.as_mut()
            .ok_or(QuicError::StreamError("pas de stream de réception".into()))?;

        // Lire taille
        let mut len_buf = [0u8; 4];
        match try_read_exact(stream, &mut len_buf).await? {
            false => return Ok(None), // stream terminé
            true => {}
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 { return Ok(None); }

        // Lire données
        let mut data = vec![0u8; len];
        read_exact_from_stream(stream, &mut data).await?;

        let token = serde_json::from_slice::<GeneratedToken>(&data)
            .map_err(|e| QuicError::StreamError(e.to_string()))?;
        Ok(Some(token))
    }

    /// Fermer le stream d'émission
    pub async fn finish(&mut self) -> Result<(), QuicError> {
        if let Some(stream) = self.send_stream.as_mut() {
            stream.finish()
                .map_err(|e| QuicError::StreamError(e.to_string()))?;
        }
        Ok(())
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn read_exact_from_stream(
    stream: &mut quinn::RecvStream,
    buf: &mut [u8],
) -> Result<(), QuicError> {
    let mut offset = 0;
    while offset < buf.len() {
        let chunk = stream.read_chunk(buf.len() - offset, true).await
            .map_err(|e| QuicError::StreamError(e.to_string()))?
            .ok_or(QuicError::StreamError("stream fermé prématurément".into()))?;
        let n = chunk.bytes.len();
        buf[offset..offset + n].copy_from_slice(&chunk.bytes);
        offset += n;
    }
    Ok(())
}

/// Retourne false si le stream est terminé proprement, Err si erreur
async fn try_read_exact(stream: &mut quinn::RecvStream, buf: &mut [u8]) -> Result<bool, QuicError> {
    let first = stream.read_chunk(1, true).await
        .map_err(|e| QuicError::StreamError(e.to_string()))?;
    match first {
        None => return Ok(false),
        Some(chunk) => {
            buf[0] = chunk.bytes[0];
        }
    }
    if buf.len() > 1 {
        read_exact_from_stream(stream, &mut buf[1..]).await?;
    }
    Ok(true)
}

// ─── Quantization (Palier G, transfert compact des activations) ───────────────

/// Quantization symétrique dynamique INT8 d'un tenseur f32, avec accélération
/// SIMD (f32x8, AVX2) pour la passe min/max et la passe de mise à l'échelle.
/// Retourne (données_quantisées, scale).
pub fn quantize_f32_to_i8(data: &[f32]) -> (Vec<i8>, f32) {
    if data.is_empty() {
        return (vec![], 1.0);
    }

    // Trouver min/max — vectorise le chargement par blocs de 8, réduction
    // scalaire via as_array_ref() (reduce_min/reduce_max indisponibles dans
    // la version de `wide` utilisée ici).
    let mut min_val = f32::MAX;
    let mut max_val = f32::MIN;

    let mut i = 0;
    while i + 8 <= data.len() {
        let v = f32x8::from(&data[i..i + 8]);
        for &x in v.as_array_ref() {
            if x < min_val { min_val = x; }
            if x > max_val { max_val = x; }
        }
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

/// Quantization asymétrique dynamique en UINT8 (0-255), meilleure précision
/// que la version symétrique quand la distribution du tenseur n'est pas
/// centrée sur zéro. Retourne (données_quantisées_u8, scale, zero_point).
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
