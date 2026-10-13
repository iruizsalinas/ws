//! Hubs, clients, servers and upgrade decisions.

use crate::ffi::{FATAL, OK, SOFT, WsStr, fail, registry};
use std::ffi::c_int;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use ws_core::{ClientOptions, Hub, Reply, ServerOptions, tls};

#[unsafe(no_mangle)]
pub extern "C" fn ws_hub() -> u64 {
    let mut reg = registry();
    let id = reg.token();
    reg.hubs.insert(id, Arc::new(Hub::default()));
    id
}

/// Monotonic seconds, for timers.
#[unsafe(no_mangle)]
pub extern "C" fn ws_now() -> f64 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_secs_f64()
}

fn hub(id: u64) -> Result<Arc<Hub>, c_int> {
    registry().hubs.get(&id).cloned().ok_or_else(|| fail(FATAL, "unknown hub"))
}

/// Newline-separated items; empty when absent.
fn lines(s: WsStr, what: &str) -> Result<Vec<String>, c_int> {
    Ok(s.text(what)?.unwrap_or("").lines().map(str::to_string).collect())
}

/// Starts a client. `protocols` and `headers` hold one item per line,
/// headers as `name:value`; zero `timeout` or `max` keep the defaults.
#[unsafe(no_mangle)]
pub extern "C" fn ws_connect(
    hub_id: u64,
    url: WsStr,
    protocols: WsStr,
    headers: WsStr,
    ca: WsStr,
    timeout: f64,
    max: u64,
    id: &mut u64,
) -> c_int {
    let run = || -> Result<u64, c_int> {
        let hub = hub(hub_id)?;
        let url = url.text("url")?.unwrap_or("");
        let mut opts = ClientOptions { protocols: lines(protocols, "protocols")?, ..Default::default() };
        for line in lines(headers, "headers")? {
            let (name, value) = line.split_once(':').ok_or_else(|| fail(FATAL, "invalid header"))?;
            opts.headers.push((name.to_string(), value.to_string()));
        }
        if let Some(ca) = ca.text("ca")? {
            opts.tls = Some(tls::client(Some(ca)).map_err(|e| fail(FATAL, &e))?);
        }
        if timeout > 0.0 {
            opts.timeout = Duration::from_secs_f64(timeout.min(1.0e6));
        }
        if max > 0 {
            opts.max_payload = usize::try_from(max).unwrap_or(usize::MAX);
        }
        let conn = hub.connect(url, opts).map_err(|e| fail(FATAL, &e))?;
        let conn_id = conn.id();
        registry().conns.insert(conn_id, conn);
        Ok(conn_id)
    };
    run().map_or_else(
        |status| status,
        |v| {
            *id = v;
            OK
        },
    )
}

/// Binds a server. Certificate problems are fatal; bind errors are not.
#[unsafe(no_mangle)]
pub extern "C" fn ws_listen(
    hub_id: u64,
    host: WsStr,
    port: u16,
    cert: WsStr,
    key: WsStr,
    max: u64,
    id: &mut u64,
) -> c_int {
    let run = || -> Result<u64, c_int> {
        let hub = hub(hub_id)?;
        let mut opts = ServerOptions { port, ..Default::default() };
        if let Some(host) = host.text("host")? {
            opts.host = host.to_string();
        }
        if max > 0 {
            opts.max_payload = usize::try_from(max).unwrap_or(usize::MAX);
        }
        opts.tls = match (cert.text("cert")?, key.text("key")?) {
            (Some(cert), Some(key)) => Some(tls::server(cert, key).map_err(|e| fail(FATAL, &e))?),
            (None, None) => None,
            _ => return Err(fail(FATAL, "options 'cert' and 'key' go together")),
        };
        let server = hub.listen(opts).map_err(|e| fail(SOFT, &e.to_string()))?;
        let server_id = server.id();
        registry().servers.insert(server_id, server);
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
    let server = registry().servers.remove(&server);
    drop(server);
}

/// Answers an upgrade request: status 0 accepts with `protocol`,
/// 400..=599 rejects with that status.
#[unsafe(no_mangle)]
pub extern "C" fn ws_reply(token: u64, status: u16, protocol: WsStr) -> c_int {
    let Some(tx) = registry().replies.remove(&token) else {
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
