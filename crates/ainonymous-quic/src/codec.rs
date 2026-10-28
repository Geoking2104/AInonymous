//! Codec pour l'encodage/décodage des frames QUIC.
//! Format d'un frame : `[4 bytes: type] [4 bytes: length] [N bytes: payload]`.

use bytes::{Buf, BufMut, Bytes, BytesMut};

#[repr(u32)]
pub enum FrameType {
    Activation = 0x01,
    Token = 0x02,
    Control = 0x03,
    Metrics = 0x04,
    Ping = 0xFF,
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
        if buf.remaining() < 8 {
            return None;
        }
        let frame_type = buf.get_u32_le();
        let length = buf.get_u32_le() as usize;
        if buf.remaining() < length {
            return None;
        }
        let mut payload = vec![0u8; length];
        buf.copy_to_slice(&mut payload);
        Some(Frame {
            frame_type,
            payload: Bytes::from(payload),
        })
    }

    pub fn ping() -> Self {
        Frame {
            frame_type: FrameType::Ping as u32,
            payload: Bytes::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    fn frame_test(payload: &[u8]) -> Frame {
        Frame {
            frame_type: FrameType::Activation as u32,
            payload: Bytes::copy_from_slice(payload),
        }
    }

    #[test]
    fn aller_retour_conserve_type_et_payload() {
        let encoded = frame_test(&[1, 2, 3, 4, 5]).encode();
        let mut buf = encoded.clone();
        let decode = Frame::decode(&mut buf).expect("frame décodable");
        assert_eq!(decode.frame_type, FrameType::Activation as u32);
        assert_eq!(&decode.payload[..], &[1, 2, 3, 4, 5]);
        assert_eq!(buf.remaining(), 0, "tout le buffer doit être consommé");
    }

    #[test]
    fn entete_est_type_puis_longueur_en_le() {
        let encoded = frame_test(&[0xAA; 3]).encode();
        assert_eq!(encoded.len(), 8 + 3);
        assert_eq!(u32::from_le_bytes(encoded[0..4].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(encoded[4..8].try_into().unwrap()), 3);
        assert_eq!(&encoded[8..], &[0xAA, 0xAA, 0xAA]);
    }

    #[test]
    fn decode_refuse_un_entete_incomplet() {
        let mut buf = BytesMut::from(&[1u8, 2, 3, 4, 5, 6, 7][..]);
        assert!(Frame::decode(&mut buf).is_none());
    }

    #[test]
    fn decode_refuse_un_payload_tronque() {
        let encoded = frame_test(&[0u8; 10]).encode();
        let mut buf = BytesMut::from(&encoded[..12]); // entête + 4 octets sur 10
        assert!(Frame::decode(&mut buf).is_none());
    }

    #[test]
    fn longueur_absurde_ne_declenche_pas_d_allocation() {
        // Entrée adversariale : la longueur annoncée dépasse le buffer reçu.
        // Le decode doit refuser sans tenter d'allouer 4 Gio.
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&(FrameType::Control as u32).to_le_bytes());
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Frame::decode(&mut buf).is_none());
    }

    #[test]
    fn decode_accepte_un_payload_vide() {
        let encoded = frame_test(&[]).encode();
        assert_eq!(encoded.len(), 8);
        let mut buf = BytesMut::from(&encoded[..]);
        let frame = Frame::decode(&mut buf).expect("frame vide décodable");
        assert!(frame.payload.is_empty());
        assert_eq!(buf.remaining(), 0);
    }

    #[test]
    fn decode_deux_frames_consecutifs() {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&frame_test(&[1, 2]).encode());
        buf.extend_from_slice(&frame_test(&[3, 4, 5]).encode());

        assert_eq!(Frame::decode(&mut buf).unwrap().payload.len(), 2);
        assert_eq!(Frame::decode(&mut buf).unwrap().payload.len(), 3);
        assert_eq!(buf.remaining(), 0);
    }

    #[test]
    fn ping_est_un_frame_vide_de_type_0xff() {
        let ping = Frame::ping();
        assert_eq!(ping.frame_type, 0xFF);
        assert!(ping.payload.is_empty());

        let mut buf = BytesMut::from(&ping.encode()[..]);
        let decode = Frame::decode(&mut buf).unwrap();
        assert_eq!(decode.frame_type, FrameType::Ping as u32);
    }
}
