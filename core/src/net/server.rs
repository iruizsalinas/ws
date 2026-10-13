//! Listening sockets and the server side of the handshake.

use super::link::Config;
use super::serve::serve;
use crate::queue::{Events, Permit};
use crate::{HEARTBEAT, Id, MAX_BUFFER, MAX_MESSAGE, lock};
use rustls::ServerConfig;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Connections still handshaking; more are dropped on accept.
const MAX_PENDING: usize = 256;

pub struct Options {
    pub host: String,
    pub port: u16,
    pub tls: Option<Arc<ServerConfig>>,
    pub max_message: usize,
    pub max_buffer: usize,
    /// Silence before a ping; `None` disables heartbeats.
    pub heartbeat: Option<Duration>,
    /// Includes handshakes, live connections and undrained connection events.
    /// Unlimited (`usize::MAX`) by default.
    pub max_connections: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            host: "0.0.0.0".into(),
            port: 0,
            tls: None,
            max_message: MAX_MESSAGE,
            max_buffer: MAX_BUFFER,
            heartbeat: Some(HEARTBEAT),
            max_connections: usize::MAX,
        }
    }
}

pub struct Server {
    id: Id,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    /// Disconnects once the accept thread has dropped the listener.
    released: Mutex<Option<Receiver<()>>>,
}

pub(super) struct Shared {
    pub id: Id,
    pub events: Events,
    pub tls: Option<Arc<ServerConfig>>,
    pub config: Config,
    pub pending: AtomicUsize,
    pub stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    max_connections: usize,
}

impl Server {
    pub fn id(&self) -> Id {
        self.id
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops accepting and releases the port; live connections are unaffected.
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
        // Wait for the port to be free, so a new connection is refused, not reset.
        if let Some(released) = lock(&self.released).take() {
            let _ = released.recv_timeout(Duration::from_secs(1));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn listen(events: Events, id: Id, opts: Options) -> std::io::Result<Server> {
    if opts.max_message == 0 || opts.max_buffer == 0 || opts.max_connections == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "limits must be positive"));
    }
    if opts.heartbeat.is_some_and(|k| k.is_zero()) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "heartbeat must be positive"));
    }
    let listener = TcpListener::bind((opts.host.as_str(), opts.port))?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared {
        id,
        events,
        tls: opts.tls,
        config: Config { message: opts.max_message, buffer: opts.max_buffer, heartbeat: opts.heartbeat },
        pending: AtomicUsize::new(0),
        stop: stop.clone(),
        connections: Arc::default(),
        max_connections: opts.max_connections,
    });
    let flag = stop.clone();
    let (done, released) = channel();
    std::thread::Builder::new().name("ws-accept".into()).spawn(move || {
        accept(listener, shared, flag);
        drop(done);
    })?;
    Ok(Server { id, addr, stop, released: Mutex::new(Some(released)) })
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
        let Some(permit) = Permit::acquire(&shared.connections, shared.max_connections) else {
            continue;
        };
        if shared.pending.fetch_add(1, Relaxed) >= MAX_PENDING {
            shared.pending.fetch_sub(1, Relaxed);
            continue;
        }
        let s = shared.clone();
        let spawned = std::thread::Builder::new().name("ws-conn".into()).spawn(move || serve(&s, tcp, permit));
        if spawned.is_err() {
            shared.pending.fetch_sub(1, Relaxed);
        }
    }
}
