//! An incremental reader that validates frames and assembles messages.

use super::frame::{BINARY, CLOSE, CONTINUATION, Fail, Msg, PING, PONG, TEXT, apply_mask, valid_close_code};
use std::ops::Range;

struct Frame {
    fin: bool,
    op: u8,
    payload: Range<usize>,
}

struct Partial {
    op: u8,
    data: Vec<u8>,
    /// Bytes of `data` known to be valid UTF-8 so far (text only).
    checked: usize,
}

pub struct Reader {
    buf: Vec<u8>,
    pos: usize,
    server: bool,
    max: usize,
    partial: Option<Partial>,
}

impl Reader {
    /// `server` expects masked frames; `max` bounds a whole message.
    pub fn new(server: bool, max: usize) -> Self {
        Reader { buf: Vec::new(), pos: 0, server, max, partial: None }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.input().extend_from_slice(data);
    }

    /// Compact before borrowing the destination for a bounded transport read.
    pub(crate) fn input(&mut self) -> &mut Vec<u8> {
        if self.pos > 0 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        &mut self.buf
    }

    /// The next complete message, `None` if more input is needed.
    pub fn message(&mut self) -> Result<Option<Msg>, Fail> {
        loop {
            let Some(frame) = self.frame()? else {
                return Ok(None);
            };
            if let Some(msg) = self.assemble(frame)? {
                return Ok(Some(msg));
            }
        }
    }

    fn frame(&mut self) -> Result<Option<Frame>, Fail> {
        let b = &self.buf[self.pos..];
        if b.len() < 2 {
            return Ok(None);
        }
        let (fin, op, masked) = (b[0] & 0x80 != 0, b[0] & 0x0F, b[1] & 0x80 != 0);
        if b[0] & 0x70 != 0 {
            return Err(Fail(1002, "reserved bits set"));
        }
        if !matches!(op, CONTINUATION | TEXT | BINARY | CLOSE | PING | PONG) {
            return Err(Fail(1002, "unknown opcode"));
        }
        if masked != self.server {
            return Err(Fail(1002, if self.server { "unmasked client frame" } else { "masked server frame" }));
        }
        // Reject invalid sequencing before buffering an attacker-chosen payload.
        if op < CLOSE {
            match (op, self.partial.is_some()) {
                (CONTINUATION, false) => return Err(Fail(1002, "unexpected continuation frame")),
                (TEXT | BINARY, true) => return Err(Fail(1002, "expected continuation frame")),
                _ => {}
            }
        }
        let (len, mut at) = match b[1] & 0x7F {
            126 if b.len() >= 4 => (u64::from(u16::from_be_bytes([b[2], b[3]])), 4),
            127 if b.len() >= 10 => (u64::from_be_bytes(b[2..10].try_into().unwrap_or_default()), 10),
            126 | 127 => return Ok(None),
            n => (u64::from(n), 2),
        };
        if (at == 4 && len < 126) || (at == 10 && (len < 65536 || len >> 63 != 0)) {
            return Err(Fail(1002, "invalid payload length encoding"));
        }
        if op >= CLOSE {
            if !fin || len > 125 {
                return Err(Fail(1002, "invalid control frame"));
            }
        } else {
            let pending = self.partial.as_ref().map_or(0, |p| p.data.len()) as u64;
            if len.saturating_add(pending) > self.max as u64 {
                return Err(Fail(1009, "message too big"));
            }
        }
        let mask = if masked {
            if b.len() < at + 4 {
                return Ok(None);
            }
            at += 4;
            Some([b[at - 4], b[at - 3], b[at - 2], b[at - 1]])
        } else {
            None
        };
        let len = len as usize; // bounded by `max` or 125 above
        if len > b.len() - at {
            return Ok(None);
        }
        let start = self.pos + at;
        let end = start + len;
        if let Some(key) = mask {
            apply_mask(&mut self.buf[start..end], key);
        }
        self.pos = end;
        Ok(Some(Frame { fin, op, payload: start..end }))
    }

    fn assemble(&mut self, Frame { fin, op, payload }: Frame) -> Result<Option<Msg>, Fail> {
        let payload = &self.buf[payload];
        match op {
            PING => return Ok(Some(Msg::Ping(payload.to_vec()))),
            PONG => return Ok(Some(Msg::Pong(payload.to_vec()))),
            CLOSE => return close(payload).map(Some),
            _ => {}
        }
        // `frame` validated sequencing before accepting the payload.
        let partial = match self.partial.as_mut() {
            Some(p) => {
                p.data.extend_from_slice(payload);
                p
            }
            None => self.partial.insert(Partial { op, data: payload.to_vec(), checked: 0 }),
        };
        if partial.op == TEXT {
            match std::str::from_utf8(&partial.data[partial.checked..]) {
                Ok(_) => partial.checked = partial.data.len(),
                Err(e) if e.error_len().is_none() && !fin => partial.checked += e.valid_up_to(),
                Err(_) => return Err(Fail(1007, "invalid UTF-8 in text message")),
            }
        }
        if !fin {
            return Ok(None);
        }
        let Partial { op, data, .. } = self.partial.take().unwrap_or_else(|| unreachable!());
        Ok(Some(if op == TEXT { Msg::Text(data) } else { Msg::Binary(data) }))
    }
}

fn close(payload: &[u8]) -> Result<Msg, Fail> {
    match payload {
        [] => Ok(Msg::Close(None)),
        [_] => Err(Fail(1002, "invalid close frame")),
        [a, b, reason @ ..] => {
            let code = u16::from_be_bytes([*a, *b]);
            if !valid_close_code(code) {
                return Err(Fail(1002, "invalid close code"));
            }
            let reason = std::str::from_utf8(reason).map_err(|_| Fail(1007, "invalid UTF-8 in close reason"))?;
            Ok(Msg::Close(Some((code, reason.to_string()))))
        }
    }
}
