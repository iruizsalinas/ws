//! A live connection: shared state, a reader thread and a writer thread.
//!
//! Lock order is `flags` -> `tls` -> `queue`. TLS records are sealed while
//! `tls` is held and queued before it is released, so they stay in order.

mod close;
mod handle;
mod heartbeat;
mod read;
mod send;
mod write;

use crate::queue::Events;
use crate::{Event, Id, lock};
use rustls::Connection;
use send::Queue;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
/// Accounting cost of an event beyond its payload.
const EVENT_COST: usize = 64;

/// Per-connection settings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Config {
    /// Largest message accepted, in bytes.
    pub message: usize,
    /// Stop accepting application frames while the writer is this far behind.
    pub buffer: usize,
    /// Ping after this much silence; drop the peer after as much again.
    pub heartbeat: Option<Duration>,
}

#[cfg(test)]
impl Config {
    /// Defaults with heartbeat off, for tests that drive a link by hand.
    pub fn test(message: usize) -> Self {
        Config { message, buffer: crate::MAX_BUFFER, heartbeat: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Connecting,
    Open,
    Closing,
    Closed,
}

struct Flags {
    state: State,
    sent_close: bool,
    /// The peer's close frame (1005 when it carried no code).
    received: Option<(u16, String)>,
    /// Close code we sent after a protocol violation.
    failed: Option<u16>,
    deadline: Option<Instant>,
}

struct Io {
    tcp: TcpStream,
    tls: Option<Mutex<Connection>>,
}

pub(crate) struct Link {
    id: Id,
    server: bool,
    max: usize,
    buffer: usize,
    heartbeat: Option<Duration>,
    /// Payload of heartbeat pings, so their pongs are not reported.
    token: [u8; 8],
    events: Events,
    flags: Mutex<Flags>,
    queue: Mutex<Queue>,
    ready: Condvar,
    buffered: AtomicUsize,
    inbox: AtomicUsize,
    /// Signals inbox space and write progress; always paired with `queue`.
    drained: Condvar,
    /// When the link was created, and milliseconds after it of the last
    /// write the peer acknowledged by making room for more.
    born: Instant,
    acked: AtomicU64,
    io: OnceLock<Io>,
    protocol: OnceLock<Option<String>>,
}

impl Link {
    pub fn new(id: Id, server: bool, config: Config, events: Events) -> Arc<Link> {
        let flags = Flags { state: State::Connecting, sent_close: false, received: None, failed: None, deadline: None };
        Arc::new(Link {
            id,
            server,
            max: config.message,
            buffer: config.buffer,
            heartbeat: config.heartbeat,
            token: crate::random(),
            events,
            flags: Mutex::new(flags),
            queue: Mutex::default(),
            ready: Condvar::new(),
            buffered: AtomicUsize::new(0),
            inbox: AtomicUsize::new(0),
            drained: Condvar::new(),
            born: Instant::now(),
            acked: AtomicU64::new(0),
            io: OnceLock::new(),
            protocol: OnceLock::new(),
        })
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn state(&self) -> State {
        lock(&self.flags).state
    }

    pub fn protocol(&self) -> Option<&str> {
        self.protocol.get().and_then(|p| p.as_deref())
    }

    pub fn buffered(&self) -> usize {
        if self.state() == State::Closed { 0 } else { self.buffered.load(Relaxed) }
    }

    pub fn fail(&self, error: &str) {
        self.emit(Event::Error { id: self.id, error: error.to_string() });
        self.finish(1006, String::new());
    }

    /// Sends an event unless the close event already went out. Once the hub
    /// is gone nothing can read or close this connection, so it ends.
    fn emit(&self, event: Event) {
        let sent = {
            let f = lock(&self.flags);
            f.state == State::Closed || self.events.send(event).is_ok()
        };
        if !sent || self.events.closed() {
            self.terminate();
        }
    }
}
