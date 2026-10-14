//! Validation on the caller; DNS, connecting and upgrading on the shared runtime.
use super::dial::dial;
use super::link::{Config, Link};
use super::runtime;
use super::stream::{HeadError, Stream};
use super::transport;
use crate::proto::{base64, handshake, http, url};
use crate::queue::Events;
use crate::{Event, Id, random, tls};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

pub use super::options::Client as Options;
struct Target {
    url: url::Url,
    tls: Option<(Arc<ClientConfig>, ServerName<'static>)>,
}

pub(crate) fn connect(events: Events, id: Id, input: &str, opts: Options) -> Result<Arc<Link>, String> {
    let url = url::parse(input)?;
    if opts.max_message == 0 || opts.max_buffer == 0 {
        return Err("limits must be positive".into());
    }
    if opts.heartbeat.is_some_and(|k| k.is_zero()) {
        return Err("heartbeat must be positive".into());
    }
    let mut seen = HashSet::new();
    for p in &opts.protocols {
        if !http::is_token(p) || !seen.insert(p) {
            return Err(format!("invalid subprotocol: {p}"));
        }
    }
    for (name, value) in &opts.headers {
        http::check_header(name, value)?;
    }
    if opts.tls.is_some() && !url.tls {
        return Err("TLS settings need a wss:// URL".into());
    }
    let tls = if url.tls {
        let config = match &opts.tls {
            Some(c) => c.clone(),
            None => tls::client(None)?,
        };
        let name = ServerName::try_from(url.host.clone()).map_err(|_| "invalid URL host")?;
        Some((config, name))
    } else {
        None
    };
    let deadline = Instant::now().checked_add(opts.timeout).ok_or("connection timeout is too large")?;
    let rt = runtime::shared().map_err(|e| format!("cannot start I/O runtime: {e}"))?;
    let config = Config { message: opts.max_message, buffer: opts.max_buffer, heartbeat: opts.heartbeat };
    let link = Link::new(id, false, config, events);
    let worker = link.clone();
    rt.spawn(async move {
        let _lease = worker.lease();
        let target = Target { url, tls };
        let result = tokio::select! {
            biased;
            _ = worker.stopped.wait() => return,
            _ = worker.events.wait_closed() => { worker.terminate(); return; }
            result = tokio::time::timeout_at(deadline.into(), establish(target, &opts)) => result,
        };
        match result {
            Ok(Ok((stream, rest, protocol))) => worker.start(stream, rest, protocol, Event::Open { id }).await,
            Ok(Err(error)) => worker.fail(&error),
            Err(_) => worker.fail("handshake timed out"),
        }
    });
    Ok(link)
}

async fn establish(target: Target, opts: &Options) -> Result<(Stream, Vec<u8>, Option<String>), String> {
    let url = target.url;
    let addrs = super::dns::resolve(url.host.as_str(), url.port)
        .await
        .map_err(|e| format!("cannot resolve {}: {e}", url.host))?;
    let tcp = dial(addrs).await.map_err(|e| e.unwrap_or_else(|| format!("cannot resolve {}", url.host)))?;
    let mut stream = transport::client(tcp, target.tls).await.map_err(|e| format!("TLS handshake failed: {e}"))?;
    let key = base64::encode(&random::<16>());
    stream
        .write_all(&handshake::client_request(&url, &key, &opts.protocols, &opts.headers))
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;
    let (head, rest) = stream.read_head().await.map_err(|e| match e {
        HeadError::TooLarge => "handshake response too large".into(),
        HeadError::Io(e) => format!("handshake failed: {e}"),
    })?;
    let protocol = handshake::response(&head, &key, &opts.protocols)?;
    Ok((stream, rest, protocol))
}
