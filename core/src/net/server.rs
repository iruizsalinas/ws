//! Synchronous listener creation; acceptance runs on the shared I/O runtime.
use super::{link::Config, runtime};
use crate::queue::Events;
use crate::signal::Signal;
use crate::{Id, lock};
use rustls::ServerConfig;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use super::options::Server as Options;

pub struct Server {
    id: Id,
    addr: SocketAddr,
    stop: Arc<Signal>,
    released: Mutex<Option<Receiver<()>>>,
}

pub(super) struct Shared {
    pub id: Id,
    pub events: Events,
    pub tls: Option<Arc<ServerConfig>>,
    pub config: Config,
    pub pending: Arc<AtomicUsize>,
    pub stop: Arc<Signal>,
    pub connections: Arc<AtomicUsize>,
    pub max_connections: usize,
}

impl Server {
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Releases the port before returning; existing connections are unaffected.
    pub fn close(&self) {
        if self.stop.set() {
            return;
        }
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
    let rt = runtime::shared()?;
    let listener = TcpListener::bind((opts.host.as_str(), opts.port))?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let listener = {
        let _entered = rt.enter();
        tokio::net::TcpListener::from_std(listener)?
    };
    let stop = Arc::new(Signal::default());
    let shared = Arc::new(Shared {
        id,
        events,
        tls: opts.tls,
        config: Config { message: opts.max_message, buffer: opts.max_buffer, heartbeat: opts.heartbeat },
        pending: Arc::default(),
        stop: stop.clone(),
        connections: Arc::default(),
        max_connections: opts.max_connections,
    });
    let (done, released) = channel();
    rt.spawn(async move {
        super::accept::accept(listener, shared).await;
        drop(done);
    });
    Ok(Server { id, addr, stop, released: Mutex::new(Some(released)) })
}
