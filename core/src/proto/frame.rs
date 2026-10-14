//! Frame encoding and the message types the reader produces (RFC 6455 section 5).

pub const TEXT: u8 = 0x1;
pub const BINARY: u8 = 0x2;
pub const CLOSE: u8 = 0x8;
pub const PING: u8 = 0x9;
pub const PONG: u8 = 0xA;
pub const CONTINUATION: u8 = 0x0;

/// Size of an encoded frame carrying `len` payload bytes.
pub fn encoded_len(len: usize, masked: bool) -> usize {
    let extended = match len {
        0..=125 => 0,
        126..=0xFFFF => 2,
        _ => 8,
    };
    len.saturating_add(2 + extended + if masked { 4 } else { 0 })
}

pub fn encode(op: u8, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(encoded_len(payload.len(), mask.is_some()));
    encode_into(&mut out, op, payload, mask);
    out
}

/// Appends one frame, masking only its new payload even when `out` holds frames.
pub(crate) fn encode_into(out: &mut Vec<u8>, op: u8, payload: &[u8], mask: Option<[u8; 4]>) {
    let len = payload.len();
    out.reserve(encoded_len(len, mask.is_some()));
    out.push(0x80 | op);
    let bit = if mask.is_some() { 0x80 } else { 0 };
    match len {
        0..=125 => out.push(bit | len as u8),
        126..=0xFFFF => {
            out.push(bit | 126);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        _ => {
            out.push(bit | 127);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    let start = out.len() + if mask.is_some() { 4 } else { 0 };
    if let Some(key) = mask {
        out.extend_from_slice(&key);
    }
    out.extend_from_slice(payload);
    if let Some(key) = mask {
        apply_mask(&mut out[start..], key);
    }
}

pub fn apply_mask(data: &mut [u8], key: [u8; 4]) {
    let word = u32::from_ne_bytes(key);
    let (chunks, rest) = data.as_chunks_mut::<4>();
    for c in chunks {
        *c = (u32::from_ne_bytes(*c) ^ word).to_ne_bytes();
    }
    for (b, k) in rest.iter_mut().zip(key) {
        *b ^= k;
    }
}

/// Close payload: status code plus UTF-8 reason.
pub fn close_payload(code: u16, reason: &str) -> Vec<u8> {
    let mut p = code.to_be_bytes().to_vec();
    p.extend_from_slice(reason.as_bytes());
    p
}

/// Codes an endpoint may receive in a close frame.
pub fn valid_close_code(code: u16) -> bool {
    matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999)
}

#[derive(Debug, PartialEq)]
pub enum Msg {
    Text(Vec<u8>),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    /// `None` when the peer sent no status code.
    Close(Option<(u16, String)>),
}

/// A protocol violation: the close code to send and a description.
#[derive(Debug, PartialEq)]
pub struct Fail(pub u16, pub &'static str);
