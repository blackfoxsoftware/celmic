//! Binary packet format shared with the mobile app. See docs/PROTOCOL.md.

pub const MAGIC: [u8; 4] = *b"CMIC";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 20;
pub const CODE_LEN: usize = 6;
pub const MAX_NAME_LEN: usize = 64;

pub const CODEC_PCM_S16LE: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    Hello = 0x01,
    Welcome = 0x02,
    Reject = 0x03,
    Audio = 0x10,
    Ping = 0x20,
    Pong = 0x21,
    Bye = 0x30,
}

impl PacketType {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x01 => Self::Hello,
            0x02 => Self::Welcome,
            0x03 => Self::Reject,
            0x10 => Self::Audio,
            0x20 => Self::Ping,
            0x21 => Self::Pong,
            0x30 => Self::Bye,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RejectReason {
    BadCode = 1,
    /// The desktop doesn't know this session (e.g. it restarted): HELLO again.
    UnknownSession = 2,
    UnsupportedVersion = 3,
    /// The user disconnected this phone from the desktop: stop, don't retry.
    Kicked = 4,
    /// Another phone paired and took over: stop, don't retry.
    Replaced = 5,
}

/// Sample rates the desktop accepts in AUDIO packets.
pub const SAMPLE_RATES: std::ops::RangeInclusive<u32> = 8_000..=192_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub kind: PacketType,
    pub codec: u8,
    pub channels: u8,
    pub session: u32,
    pub seq: u32,
    pub sample_rate: u32,
}

impl Header {
    pub fn new(kind: PacketType, session: u32) -> Self {
        Self {
            version: VERSION,
            kind,
            codec: 0,
            channels: 0,
            session,
            seq: 0,
            sample_rate: 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    TooShort,
    BadMagic,
    UnknownType(u8),
}

/// Parses a datagram into its header and payload.
///
/// The version byte is returned as-is so the caller can reject newer clients
/// with a proper `REJECT` instead of silently dropping them.
pub fn parse(buf: &[u8]) -> Result<(Header, &[u8]), ParseError> {
    if buf.len() < HEADER_LEN {
        return Err(ParseError::TooShort);
    }
    if buf[0..4] != MAGIC {
        return Err(ParseError::BadMagic);
    }
    let kind = PacketType::from_u8(buf[5]).ok_or(ParseError::UnknownType(buf[5]))?;
    let header = Header {
        version: buf[4],
        kind,
        codec: buf[6],
        channels: buf[7],
        session: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
        seq: u32::from_be_bytes(buf[12..16].try_into().unwrap()),
        sample_rate: u32::from_be_bytes(buf[16..20].try_into().unwrap()),
    };
    Ok((header, &buf[HEADER_LEN..]))
}

pub fn encode(header: &Header, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.push(header.version);
    out.push(header.kind as u8);
    out.push(header.codec);
    out.push(header.channels);
    out.extend_from_slice(&header.session.to_be_bytes());
    out.extend_from_slice(&header.seq.to_be_bytes());
    out.extend_from_slice(&header.sample_rate.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Splits a HELLO payload into `(pairing code, device name)`.
pub fn parse_hello(payload: &[u8]) -> Option<(&str, String)> {
    if payload.len() < CODE_LEN {
        return None;
    }
    let code = std::str::from_utf8(&payload[..CODE_LEN]).ok()?;
    if !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let name_bytes = &payload[CODE_LEN..payload.len().min(CODE_LEN + MAX_NAME_LEN)];
    let name = String::from_utf8_lossy(name_bytes).trim().to_string();
    Some((code, name))
}

/// Decodes little-endian i16 PCM into `out` as f32 in [-1, 1).
pub fn decode_pcm_s16le(payload: &[u8], out: &mut Vec<f32>) {
    out.clear();
    out.extend(
        payload
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut h = Header::new(PacketType::Audio, 0xdead_beef);
        h.seq = 42;
        h.sample_rate = 48_000;
        h.channels = 1;
        let bytes = encode(&h, &[1, 2, 3, 4]);
        let (parsed, payload) = parse(&bytes).unwrap();
        assert_eq!(parsed, h);
        assert_eq!(payload, &[1, 2, 3, 4]);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse(&[0; 4]), Err(ParseError::TooShort));
        assert_eq!(parse(&[0; 20]), Err(ParseError::BadMagic));
        let mut bytes = encode(&Header::new(PacketType::Ping, 1), &[]);
        bytes[5] = 0x99;
        assert_eq!(parse(&bytes), Err(ParseError::UnknownType(0x99)));
    }

    #[test]
    fn hello_payload() {
        let mut payload = b"123456".to_vec();
        payload.extend_from_slice("Pixel 9 ".as_bytes());
        assert_eq!(parse_hello(&payload), Some(("123456", "Pixel 9".to_string())));
        assert_eq!(parse_hello(b"12ab56"), None);
        assert_eq!(parse_hello(b"123"), None);
    }

    #[test]
    fn pcm_decode() {
        let mut out = Vec::new();
        decode_pcm_s16le(&[0x00, 0x80, 0xff, 0x7f, 0x00, 0x00], &mut out);
        assert_eq!(out, vec![-1.0, 32767.0 / 32768.0, 0.0]);
    }
}
