use std::time::Instant;
use anyhow::Result;
use bytes::Bytes;
use tracing::{debug, info};

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
        stream.finish().await
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
            stream.finish().await
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
