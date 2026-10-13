//! Listening sockets and the server side of the handshake.

use super::serve::serve;
use crate::{Event, Id, MAX_PAYLOAD};
use rustls::ServerConfig;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Connections still handshaking; more are dropped on accept.
const MAX_PENDING: usize = 256;

pub struct Options {
    pub host: String,
    pub port: u16,
    pub tls: Option<Arc<ServerConfig>>,
    pub max_payload: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options { host: "0.0.0.0".into(), port: 0, tls: None, max_payload: MAX_PAYLOAD }
    }
}

pub struct Server {
    id: Id,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

pub(super) struct Shared {
    pub id: Id,
    pub events: Sender<Event>,
    pub tls: Option<Arc<ServerConfig>>,
    pub max: usize,
    pub pending: AtomicUsize,
}

impl Server {
    pub fn id(&self) -> Id {
        self.id
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops accepting; live connections are unaffected.
    pub fn close(&self) {
        if self.stop.swap(true, Relaxed) {
            return;
        }
        // Wake the blocked accept() so the listener is released.
        let ip = match self.addr.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip => ip,
        };
        let _ = TcpStream::connect_timeout(&SocketAddr::new(ip, self.addr.port()), Duration::from_secs(1));
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn listen(events: Sender<Event>, id: Id, opts: Options) -> std::io::Result<Server> {
    let listener = TcpListener::bind((opts.host.as_str(), opts.port))?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared { id, events, tls: opts.tls, max: opts.max_payload, pending: AtomicUsize::new(0) });
    let flag = stop.clone();
    std::thread::Builder::new().name("ws-accept".into()).spawn(move || accept(listener, shared, flag))?;
    Ok(Server { id, addr, stop })
}

fn accept(listener: TcpListener, shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    for tcp in listener.incoming() {
        if stop.load(Relaxed) {
            return;
        }
        let Ok(tcp) = tcp else {
            // Out of descriptors or similar: back off instead of spinning.
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        if shared.pending.fetch_add(1, Relaxed) >= MAX_PENDING {
            shared.pending.fetch_sub(1, Relaxed);
            continue;
        }
        let s = shared.clone();
        let spawned = std::thread::Builder::new().name("ws-conn".into()).spawn(move || serve(&s, tcp));
        if spawned.is_err() {
            shared.pending.fetch_sub(1, Relaxed);
        }
    }
}
