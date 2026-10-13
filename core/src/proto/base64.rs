//! Standard padded base64 (RFC 4648), as used by the handshake.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Strict decoding: canonical padding and zero trailing bits only.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let s = input.as_bytes();
    if !s.len().is_multiple_of(4) {
        return None;
    }
    let pad = s.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut n: u32 = 0;
    for (i, &c) in s[..s.len() - pad].iter().enumerate() {
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        n = n << 6 | v;
        if i % 4 == 3 {
            out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            n = 0;
        }
    }
    match pad {
        1 => {
            if n & 3 != 0 {
                return None;
            }
            out.extend_from_slice(&[(n >> 10) as u8, (n >> 2) as u8]);
        }
        2 => {
            if n & 15 != 0 {
                return None;
            }
            out.push((n >> 4) as u8);
        }
        _ => {}
    }
    Some(out)
}
