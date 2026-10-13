//! Hubs, clients and option conversions shared with servers.

use crate::ffi::{FATAL, OK, WsStr, fail, registry};
use std::ffi::c_int;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use ws_core::{ClientOptions, Hub, Id, tls};

#[unsafe(no_mangle)]
pub extern "C" fn ws_hub() -> u64 {
    let mut reg = registry();
    let id = reg.token();
    reg.hubs.insert(id, Arc::new(Hub::default()));
    id
}

/// Forgets a hub when its Lua state closes: its servers stop, its
/// connections end and its pending upgrades are refused.
#[unsafe(no_mangle)]
pub extern "C" fn ws_shutdown(hub_id: u64) {
    let (hub, conns, servers) = {
        let mut reg = registry();
        let ids: Vec<Id> = reg.owners.iter().filter(|&(_, &h)| h == hub_id).map(|(&id, _)| id).collect();
        let (mut conns, mut servers) = (Vec::new(), Vec::new());
        for id in ids {
            reg.owners.remove(&id);
            conns.extend(reg.conns.remove(&id));
            servers.extend(reg.servers.remove(&id));
        }
        reg.replies.retain(|_, (h, _)| *h != hub_id);
        (reg.hubs.remove(&hub_id), conns, servers)
    };
    drop(servers);
    for conn in conns {
        conn.terminate();
    }
    drop(hub);
}

/// Monotonic seconds, for timers.
#[unsafe(no_mangle)]
pub extern "C" fn ws_now() -> f64 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_secs_f64()
}

pub(crate) fn hub(id: u64) -> Result<Arc<Hub>, c_int> {
    registry().hubs.get(&id).cloned().ok_or_else(|| fail(FATAL, "unknown hub"))
}

/// Newline-separated items; empty when absent.
fn lines(s: WsStr, what: &str) -> Result<Vec<String>, c_int> {
    Ok(s.text(what)?.unwrap_or("").lines().map(str::to_string).collect())
}

/// Heartbeat seconds from C: negative keeps `default`, zero disables.
/// ws.c rejects NaN, infinities and values above a million seconds.
pub(crate) fn heartbeat_from(seconds: f64, default: Option<Duration>) -> Option<Duration> {
    if seconds < 0.0 {
        default
    } else if seconds == 0.0 {
        None
    } else {
        Some(Duration::from_secs_f64(seconds.min(1.0e6)))
    }
}

/// A size limit from C; zero keeps `default`.
pub(crate) fn size(n: u64, default: usize) -> usize {
    if n == 0 { default } else { usize::try_from(n).unwrap_or(usize::MAX) }
}

/// Starts a client. `protocols` and `headers` hold one item per line,
/// headers as `name:value`; zero `timeout` or limits keep the defaults.
#[unsafe(no_mangle)]
pub extern "C" fn ws_connect(
    hub_id: u64,
    url: WsStr,
    protocols: WsStr,
    headers: WsStr,
    ca: WsStr,
    timeout: f64,
    max: u64,
    buffer: u64,
    heartbeat: f64,
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
        if let Some(ca) = ca.text("tls.ca")? {
            opts.tls = Some(tls::client(Some(ca)).map_err(|e| fail(FATAL, &e))?);
        }
        if timeout > 0.0 {
            opts.timeout = Duration::from_secs_f64(timeout.min(1.0e6));
        }
        opts.max_message = size(max, opts.max_message);
        opts.max_buffer = size(buffer, opts.max_buffer);
        opts.heartbeat = heartbeat_from(heartbeat, opts.heartbeat);
        let conn = hub.connect(url, opts).map_err(|e| fail(FATAL, &e))?;
        let conn_id = conn.id();
        let mut reg = registry();
        reg.conns.insert(conn_id, conn);
        reg.owners.insert(conn_id, hub_id);
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
