//! The application's view: connection handles and the event hub.

use crate::net::{client, link::Link, server};
use crate::proto::frame;
use crate::{ClientOptions, Event, Id, Server, ServerOptions, State, lock, next_id};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A handle to one connection.
#[derive(Clone)]
pub struct Conn(pub(crate) Arc<Link>);

impl Conn {
    pub fn id(&self) -> Id {
        self.0.id()
    }

    pub fn state(&self) -> State {
        self.0.state()
    }

    pub fn protocol(&self) -> Option<&str> {
        self.0.protocol()
    }

    /// Bytes queued but not yet written to the socket.
    pub fn buffered(&self) -> usize {
        self.0.buffered()
    }

    /// Text must be valid UTF-8; the caller checks.
    pub fn send(&self, data: &[u8], binary: bool) -> Result<(), &'static str> {
        self.0.send(if binary { frame::BINARY } else { frame::TEXT }, data)
    }

    /// At most 125 bytes; the caller checks.
    pub fn ping(&self, data: &[u8]) -> Result<(), &'static str> {
        self.0.send(frame::PING, data)
    }

    /// `code` must pass `frame::valid_close_code`; `reason` is at most 123 bytes.
    pub fn close(&self, code: u16, reason: &str) {
        self.0.close(code, reason)
    }

    pub fn terminate(&self) {
        self.0.terminate()
    }

    /// Acknowledges a delivered message, ping or pong of `bytes` bytes,
    /// letting the reader continue once it was throttled.
    pub fn consumed(&self, bytes: usize) {
        self.0.consumed(bytes)
    }
}

/// Collects events from every connection and server it created.
pub struct Hub {
    tx: Sender<Event>,
    rx: Mutex<Receiver<Event>>,
}

impl Default for Hub {
    fn default() -> Self {
        let (tx, rx) = channel();
        Hub { tx, rx: Mutex::new(rx) }
    }
}

impl Hub {
    /// Waits up to `timeout` (forever if `None`) for the next event.
    pub fn recv(&self, timeout: Option<Duration>) -> Option<Event> {
        let rx = lock(&self.rx);
        match timeout {
            None => rx.recv().ok(),
            Some(t) => rx.recv_timeout(t).ok(),
        }
    }

    /// Starts connecting in the background; `Err` means invalid arguments.
    pub fn connect(&self, url: &str, opts: ClientOptions) -> Result<Conn, String> {
        client::connect(self.tx.clone(), next_id(), url, opts).map(Conn)
    }

    /// Binds, then accepts in the background.
    pub fn listen(&self, opts: ServerOptions) -> std::io::Result<Server> {
        server::listen(self.tx.clone(), next_id(), opts)
    }
}
