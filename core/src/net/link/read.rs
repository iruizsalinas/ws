//! Startup and the reader thread.

use super::write::{Item, seal};
use super::{Io, Link, State};
use crate::proto::reader::Reader;
use crate::{Event, TICK, lock};
use rustls::Connection;
use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Undelivered inbound bytes before the reader waits for the application.
const INBOX_LIMIT: usize = 1 << 20;

impl Link {
    /// Switches to the open state and runs the connection to completion.
    pub fn start(
        self: &Arc<Self>,
        tcp: TcpStream,
        tls: Option<Connection>,
        rest: Vec<u8>,
        protocol: Option<String>,
        open: Event,
    ) {
        let Ok((writer, shared)) = tcp.try_clone().and_then(|a| Ok((a, tcp.try_clone()?))) else {
            return self.fail("cannot clone socket");
        };
        let _ = tcp.set_nodelay(true);
        let _ = tcp.set_write_timeout(None);
        let _ = self.protocol.set(protocol);
        let tls = tls.map(|mut t| {
            t.set_buffer_limit(None);
            Mutex::new(t)
        });
        let _ = self.io.set(Io { tcp: shared, tls });
        {
            let mut f = lock(&self.flags);
            if f.state != State::Connecting {
                return;
            }
            f.state = State::Open;
            let _ = self.events.send(open);
        }
        let link = self.clone();
        if std::thread::Builder::new().name("ws-write".into()).spawn(move || link.write_loop(writer)).is_err() {
            return self.fail("cannot start writer thread");
        }
        self.read_loop(tcp, rest);
    }

    fn read_loop(&self, mut tcp: TcpStream, rest: Vec<u8>) {
        let _ = tcp.set_read_timeout(Some(TICK));
        let tls = self.io.get().and_then(|io| io.tls.as_ref());
        let mut reader = Reader::new(self.server, self.max);
        let mut plain = rest;
        let mut ok = true;
        let mut buf = vec![0u8; 16 * 1024];
        if let Some(tls) = tls
            && let Err(e) = self.open(tls, &[], &mut plain)
        {
            return self.fail(&e);
        }
        loop {
            if ok && !plain.is_empty() {
                reader.push(&plain);
                plain.clear();
                ok = self.dispatch(&mut reader);
            }
            if self.state() == State::Closed || self.expired() {
                break;
            }
            if !self.wait_inbox() {
                continue;
            }
            match tcp.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => match tls {
                    Some(tls) => match self.open(tls, &buf[..n], &mut plain) {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(e) => return self.fail(&e),
                    },
                    None => plain.extend_from_slice(&buf[..n]),
                },
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
                Err(_) => break,
            }
        }
        // Process anything decrypted alongside a close_notify.
        if ok && !plain.is_empty() {
            reader.push(&plain);
            self.dispatch(&mut reader);
        }
        self.end();
    }

    /// Backpressure: waits (one tick at most) while the application lags.
    fn wait_inbox(&self) -> bool {
        if self.inbox.load(Relaxed) < INBOX_LIMIT {
            return true;
        }
        let q = lock(&self.queue);
        if self.inbox.load(Relaxed) >= INBOX_LIMIT {
            let _ = self.drained.wait_timeout(q, TICK);
        }
        self.inbox.load(Relaxed) < INBOX_LIMIT
    }

    fn expired(&self) -> bool {
        lock(&self.flags).deadline.is_some_and(|d| Instant::now() >= d)
    }

    /// Decrypts `raw` into `plain`; `Ok(false)` once the peer sent close_notify.
    fn open(&self, tls: &Mutex<Connection>, mut raw: &[u8], plain: &mut Vec<u8>) -> Result<bool, String> {
        let mut conn = lock(tls);
        let mut result = Ok(true);
        while result.is_ok() && !raw.is_empty() {
            result = match conn.read_tls(&mut raw) {
                Ok(_) => conn.process_new_packets().map(|_| true).map_err(|e| format!("TLS error: {e}")),
                Err(e) => Err(e.to_string()),
            };
        }
        let mut chunk = [0u8; 16 * 1024];
        while result == Ok(true) {
            match conn.reader().read(&mut chunk) {
                Ok(0) => result = Ok(false),
                Ok(n) => plain.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => result = Err(e.to_string()),
            }
        }
        // Alerts, key updates and tickets go out in order with app data.
        if conn.wants_write() {
            self.enqueue(Item::Data(seal(&mut conn, &[])));
        }
        result
    }
}
