/// Codec pour l'encodage/décodage des frames QUIC
/// Format d'un frame :
///   [4 bytes: type] [4 bytes: length] [N bytes: payload]

use bytes::{Buf, BufMut, Bytes, BytesMut};

#[repr(u32)]
pub enum FrameType {
    Activation = 0x01,
    Token      = 0x02,
    Control    = 0x03,
    Metrics    = 0x04,
    Ping       = 0xFF,
}

pub struct Frame {
    pub frame_type: u32,
    pub payload: Bytes,
}

impl Frame {
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(8 + self.payload.len());
        buf.put_u32_le(self.frame_type);
        buf.put_u32_le(self.payload.len() as u32);
        buf.put_slice(&self.payload);
        buf.freeze()
    }

    pub fn decode(buf: &mut impl Buf) -> Option<Self> {
        if buf.remaining() < 8 { return None; }
        let frame_type = buf.get_u32_le();
        let length = buf.get_u32_le() as usize;
        if buf.remaining() < length { return None; }
        let mut payload = vec![0u8; length];
        buf.copy_to_slice(&mut payload);
        Some(Frame { frame_type, payload: Bytes::from(payload) })
    }

    pub fn ping() -> Self {
        Frame { frame_type: FrameType::Ping as u32, payload: Bytes::new() }
    }
}
