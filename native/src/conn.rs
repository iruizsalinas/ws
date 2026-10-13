//! Per-connection calls. Unknown ids behave like closed connections.

use crate::ffi::{FATAL, OK, SOFT, WsStr, fail, out, registry, with_conn};
use std::ffi::c_int;
use ws_core::State;
use ws_core::proto::frame;

fn sent(r: Result<(), &'static str>) -> c_int {
    r.map_or_else(|e| fail(SOFT, e), |()| OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn ws_send(id: u64, data: WsStr, binary: bool) -> c_int {
    let data = data.bytes().unwrap_or_default();
    if !binary && std::str::from_utf8(data).is_err() {
        return fail(FATAL, "text must be valid UTF-8; use send(data, true) for binary");
    }
    with_conn(id, |c| sent(c.send(data, binary)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ws_ping(id: u64, data: WsStr) -> c_int {
    let data = data.bytes().unwrap_or_default();
    if data.len() > 125 {
        return fail(FATAL, "ping data must be at most 125 bytes");
    }
    with_conn(id, |c| sent(c.ping(data)))
}

/// Starts the closing handshake; closing twice is harmless.
#[unsafe(no_mangle)]
pub extern "C" fn ws_close(id: u64, code: c_int, reason: WsStr) -> c_int {
    let Some(code) = u16::try_from(code).ok().filter(|&c| frame::valid_close_code(c)) else {
        return fail(FATAL, "invalid close code");
    };
    let reason = match reason.text("close reason") {
        Ok(r) => r.unwrap_or(""),
        Err(e) => return e,
    };
    if reason.len() > 123 {
        return fail(FATAL, "close reason must be at most 123 bytes");
    }
    let conn = registry().conns.get(&id).cloned();
    if let Some(c) = conn
        && let Err(e) = c.close(code, reason)
    {
        return fail(FATAL, e);
    }
    OK
}

#[unsafe(no_mangle)]
pub extern "C" fn ws_terminate(id: u64) {
    let conn = registry().conns.get(&id).cloned();
    if let Some(c) = conn {
        c.terminate();
    }
}

/// 0 connecting, 1 open, 2 closing, 3 closed.
#[unsafe(no_mangle)]
pub extern "C" fn ws_state(id: u64) -> c_int {
    let state = registry().conns.get(&id).map_or(State::Closed, |c| c.state());
    match state {
        State::Connecting => 0,
        State::Open => 1,
        State::Closing => 2,
        State::Closed => 3,
    }
}

/// The negotiated subprotocol, or a null string.
#[unsafe(no_mangle)]
pub extern "C" fn ws_protocol(id: u64) -> WsStr {
    let protocol = registry().conns.get(&id).and_then(|c| c.protocol().map(str::to_string));
    protocol.map_or(WsStr::NONE, |p| out(p.into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ws_buffered(id: u64) -> u64 {
    registry().conns.get(&id).map_or(0, |c| c.buffered() as u64)
}
