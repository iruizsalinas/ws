//! The application's view: connection handles and the event hub.

use crate::net::{client, link::Link, server};
use crate::proto::frame;
use crate::queue::{Events, Queued};
use crate::{ClientOptions, Event, Id, Server, ServerOptions, State, lock, next_id};
use std::sync::mpsc::{Receiver, channel};
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

    /// Sends text or binary data; rejects text that is not valid UTF-8.
    pub fn send(&self, data: &[u8], binary: bool) -> Result<(), &'static str> {
        if binary {
            self.0.send(frame::BINARY, data)
        } else {
            let text =
                std::str::from_utf8(data).map_err(|_| "text must be valid UTF-8; use send(data, true) for binary")?;
            self.send_text(text)
        }
    }

    /// Sends text already validated by Rust's string type, without scanning it again.
    pub fn send_text(&self, data: &str) -> Result<(), &'static str> {
        self.0.send(frame::TEXT, data.as_bytes())
    }

    /// Sends a ping, rejecting payloads longer than 125 bytes.
    pub fn ping(&self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() > 125 {
            return Err("ping data must be at most 125 bytes");
        }
        self.0.send(frame::PING, data)
    }

    /// Starts closing, rejecting invalid codes or reasons longer than 123 bytes.
    pub fn close(&self, code: u16, reason: &str) -> Result<(), &'static str> {
        if !frame::valid_close_code(code) {
            return Err("invalid close code");
        }
        if reason.len() > 123 {
            return Err("close reason must be at most 123 bytes");
        }
        self.0.close(code, reason);
        Ok(())
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
    tx: Events,
    rx: Mutex<Receiver<Queued>>,
}

impl Default for Hub {
    fn default() -> Self {
        let (tx, rx) = channel();
        Hub { tx: tx.into(), rx: Mutex::new(rx) }
    }
}

/// Ends the connections nobody will receive: queued ones here, later ones
/// as soon as they see the hub closed (see `Link::emit`).
impl Drop for Hub {
    fn drop(&mut self) {
        self.tx.close();
        let rx = self.rx.get_mut().unwrap_or_else(|e| e.into_inner());
        while let Ok(queued) = rx.try_recv() {
            if let Event::Connection { conn, .. } = queued.event {
                conn.terminate();
            }
        }
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
        .map(|queued| queued.event)
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
