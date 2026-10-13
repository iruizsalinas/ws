//! Servers and upgrade decisions.

use crate::ffi::{FATAL, OK, SOFT, WsStr, fail, registry};
use crate::hub::{heartbeat_from, hub, size};
use std::ffi::c_int;
use ws_core::{Reply, ServerOptions, tls};

/// Binds a server; zero limits keep the defaults (unlimited connections),
/// and `heartbeat` follows `heartbeat_from`.
/// Certificate problems are fatal; bind errors are not.
#[unsafe(no_mangle)]
pub extern "C" fn ws_listen(
    hub_id: u64,
    host: WsStr,
    port: u16,
    cert: WsStr,
    key: WsStr,
    max: u64,
    buffer: u64,
    connections: u64,
    heartbeat: f64,
    id: &mut u64,
) -> c_int {
    let run = || -> Result<u64, c_int> {
        let hub = hub(hub_id)?;
        let mut opts = ServerOptions { port, ..Default::default() };
        if connections > 0 {
            opts.max_connections =
                usize::try_from(connections).map_err(|_| fail(FATAL, "connection limit is too large"))?;
        }
        if let Some(host) = host.text("host")? {
            opts.host = host.to_string();
        }
        opts.max_message = size(max, opts.max_message);
        opts.max_buffer = size(buffer, opts.max_buffer);
        opts.heartbeat = heartbeat_from(heartbeat, opts.heartbeat);
        opts.tls = match (cert.text("tls.cert")?, key.text("tls.key")?) {
            (Some(cert), Some(key)) => Some(tls::server(cert, key).map_err(|e| fail(FATAL, &e))?),
            (None, None) => None,
            _ => return Err(fail(FATAL, "options 'tls.cert' and 'tls.key' go together")),
        };
        let server = hub.listen(opts).map_err(|e| fail(SOFT, &e.to_string()))?;
        let server_id = server.id();
        let mut reg = registry();
        reg.servers.insert(server_id, server);
        reg.owners.insert(server_id, hub_id);
        Ok(server_id)
    };
    run().map_or_else(
        |status| status,
        |v| {
            *id = v;
            OK
        },
    )
}

/// The bound port, or 0 for an unknown server.
#[unsafe(no_mangle)]
pub extern "C" fn ws_port(server: u64) -> u16 {
    registry().servers.get(&server).map_or(0, |s| s.addr().port())
}

/// Stops listening; live connections are unaffected.
#[unsafe(no_mangle)]
pub extern "C" fn ws_unlisten(server: u64) {
    let server = {
        let mut reg = registry();
        reg.owners.remove(&server);
        reg.servers.remove(&server)
    };
    drop(server);
}

/// Answers an upgrade request: status 0 accepts with `protocol`,
/// 400..=599 rejects with that status.
#[unsafe(no_mangle)]
pub extern "C" fn ws_reply(token: u64, status: u16, protocol: WsStr) -> c_int {
    let Some((_, tx)) = registry().replies.remove(&token) else {
        return OK;
    };
    // Invalid arguments still answer the client, with a server error.
    let (reply, result) = match (status, protocol.text("protocol")) {
        (_, Err(e)) => (Reply::Reject(500), e),
        (0, Ok(p)) => (Reply::Accept(p.map(str::to_string)), OK),
        (400..=599, _) => (Reply::Reject(status), OK),
        _ => (Reply::Reject(500), fail(FATAL, "status must be from 400 to 599")),
    };
    let _ = tx.send(reply);
    result
}
