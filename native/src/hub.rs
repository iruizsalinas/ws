//! Hubs, clients and option conversions shared with servers.

use crate::ffi::{FATAL, OK, WsStr, fail, registry};
use std::ffi::c_int;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use ws_core::{ClientOptions, Hub, tls};

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

pub(crate) fn hub(id: u64) -> Result<Arc<Hub>, c_int> {
    registry().hubs.get(&id).cloned().ok_or_else(|| fail(FATAL, "unknown hub"))
}

/// Newline-separated items; empty when absent.
fn lines(s: WsStr, what: &str) -> Result<Vec<String>, c_int> {
    Ok(s.text(what)?.unwrap_or("").lines().map(str::to_string).collect())
}

/// Keepalive seconds from C: negative keeps `default`, zero disables.
/// ws.c rejects NaN, infinities and values above a million seconds.
pub(crate) fn keepalive_from(seconds: f64, default: Option<Duration>) -> Option<Duration> {
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
    keepalive: f64,
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
        opts.keepalive = keepalive_from(keepalive, opts.keepalive);
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
