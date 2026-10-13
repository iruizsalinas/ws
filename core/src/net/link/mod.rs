//! A live connection: shared state, a reader thread and a writer thread.
//!
//! Lock order is `flags` -> `tls` -> `queue`. TLS records are sealed while
//! `tls` is held and queued before it is released, so they stay in order.

mod close;
mod handle;
mod read;
mod write;

use crate::{Event, Id, lock};
use rustls::Connection;
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use write::Queue;

const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
/// Accounting cost of an event beyond its payload.
const EVENT_COST: usize = 64;

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
    events: Sender<Event>,
    flags: Mutex<Flags>,
    queue: Mutex<Queue>,
    ready: Condvar,
    buffered: AtomicUsize,
    inbox: AtomicUsize,
    /// Signals inbox space and write progress; always paired with `queue`.
    drained: Condvar,
    io: OnceLock<Io>,
    protocol: OnceLock<Option<String>>,
}

impl Link {
    pub fn new(id: Id, server: bool, max: usize, events: Sender<Event>) -> Arc<Link> {
        let flags = Flags { state: State::Connecting, sent_close: false, received: None, failed: None, deadline: None };
        Arc::new(Link {
            id,
            server,
            max,
            events,
            flags: Mutex::new(flags),
            queue: Mutex::default(),
            ready: Condvar::new(),
            buffered: AtomicUsize::new(0),
            inbox: AtomicUsize::new(0),
            drained: Condvar::new(),
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

    /// Sends a data or ping frame; fails unless the connection is open.
    pub fn send(&self, op: u8, payload: &[u8]) -> Result<(), &'static str> {
        let f = lock(&self.flags);
        if f.state != State::Open || f.sent_close {
            return Err(if f.state == State::Connecting { "not open yet" } else { "closed" });
        }
        self.push(self.frame(op, payload));
        Ok(())
    }

    /// Called once the application has taken an inbound event.
    pub fn consumed(&self, bytes: usize) {
        let _q = lock(&self.queue);
        self.inbox.fetch_sub(bytes + EVENT_COST, Relaxed);
        self.drained.notify_one();
    }

    pub fn fail(&self, error: &str) {
        self.emit(Event::Error { id: self.id, error: error.to_string() });
        self.finish(1006, String::new());
    }

    /// Sends an event unless the close event already went out.
    fn emit(&self, event: Event) {
        let f = lock(&self.flags);
        if f.state != State::Closed {
            let _ = self.events.send(event);
        }
    }
}
