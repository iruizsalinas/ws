//! Waiting for events and flattening them into a C struct.

use crate::ffi::{FATAL, WsStr, fail, out, registry};
use std::ffi::c_int;
use std::time::Duration;
use ws_core::{Event, Request};

const OPEN: c_int = 1;
const MESSAGE: c_int = 2;
const PING: c_int = 3;
const PONG: c_int = 4;
const ERROR: c_int = 5;
const CLOSE: c_int = 6;
const REQUEST: c_int = 7;
const CONNECTION: c_int = 8;

/// One event; `data` stays valid until the next call on this thread.
#[repr(C)]
pub struct WsEvent {
    kind: c_int,
    id: u64,
    server: u64,
    code: c_int,
    data: WsStr,
}

/// Request fields, one per line: path, address, port, protocols separated
/// by commas, then `name:value` headers. None of them can hold a newline.
fn pack(req: &Request) -> Vec<u8> {
    let mut s = format!("{}\n{}\n{}\n{}\n", req.path, req.addr.ip(), req.addr.port(), req.protocols.join(","));
    for (name, value) in &req.headers {
        s += &format!("{name}:{value}\n");
    }
    s.into_bytes()
}

fn flatten(event: Event) -> (c_int, u64, u64, c_int, Vec<u8>) {
    let mut reg = registry();
    // Inbound data counts as consumed once it is handed over.
    let take = |id, data: Vec<u8>| {
        if let Some(c) = reg.conns.get(&id) {
            c.consumed(data.len());
        }
        data
    };
    match event {
        Event::Open { id } => (OPEN, id, 0, 0, Vec::new()),
        Event::Message { id, data, binary } => (MESSAGE, id, 0, binary as c_int, take(id, data)),
        Event::Ping { id, data } => (PING, id, 0, 0, take(id, data)),
        Event::Pong { id, data } => (PONG, id, 0, 0, take(id, data)),
        Event::Error { id, error } => (ERROR, id, 0, 0, error.into_bytes()),
        Event::Close { id, code, reason } => {
            reg.conns.remove(&id);
            (CLOSE, id, 0, c_int::from(code), reason.into_bytes())
        }
        Event::Request { server, req, reply } => {
            let token = reg.token();
            reg.replies.insert(token, reply);
            (REQUEST, token, server, 0, pack(&req))
        }
        Event::Connection { server, conn, req } => {
            let id = conn.id();
            reg.conns.insert(id, conn);
            (CONNECTION, id, server, 0, pack(&req))
        }
    }
}

/// Waits up to `timeout` seconds (forever if negative); 1 if `ev` was
/// filled, 0 on timeout.
#[unsafe(no_mangle)]
pub extern "C" fn ws_wait(hub: u64, timeout: f64, ev: &mut WsEvent) -> c_int {
    let Some(hub) = registry().hubs.get(&hub).cloned() else {
        return -fail(FATAL, "unknown hub");
    };
    let timeout = (timeout >= 0.0).then(|| Duration::from_secs_f64(timeout.min(1.0e6)));
    let Some(event) = hub.recv(timeout) else { return 0 };
    let (kind, id, server, code, data) = flatten(event);
    *ev = WsEvent { kind, id, server, code, data: out(data) };
    1
}
