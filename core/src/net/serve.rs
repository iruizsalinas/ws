//! The server side of one handshake, from TCP accept to open.

use super::link::Link;
use super::server::Shared;
use super::stream::{HeadError, Stream};
use crate::proto::handshake;
use crate::{Conn, Event, Reply, Request};
use rustls::{Connection, ServerConnection};
use std::io::{ErrorKind, Read};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Decrements the pending count once the handshake is over.
struct Pending<'a>(Option<&'a AtomicUsize>);

impl Pending<'_> {
    fn done(&mut self) {
        if let Some(p) = self.0.take() {
            p.fetch_sub(1, Relaxed);
        }
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.done();
    }
}

pub(super) fn serve(shared: &Shared, tcp: TcpStream) {
    let mut pending = Pending(Some(&shared.pending));
    let Ok(addr) = tcp.peer_addr() else { return };
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let alive = || true;
    let tls = match &shared.tls {
        Some(config) => match ServerConnection::new(config.clone()) {
            Ok(c) => Some(Connection::from(c)),
            Err(_) => return,
        },
        None => None,
    };
    let mut s = Stream::new(tcp, tls, deadline, &alive);
    if s.handshake().is_err() {
        return;
    }
    let (head, rest) = match s.read_head() {
        Ok(h) => h,
        Err(HeadError::TooLarge) => return reject(&mut s, 431),
        Err(HeadError::Io(e)) if e.kind() == ErrorKind::TimedOut => return reject(&mut s, 408),
        Err(HeadError::Io(_)) => return,
    };
    let parsed = match handshake::request(&head) {
        Ok(r) => r,
        Err(status) => return reject(&mut s, status),
    };
    let req = Request { path: parsed.target, headers: parsed.headers, protocols: parsed.protocols, addr };
    let (tx, rx) = sync_channel(1);
    if shared.events.send(Event::Request { server: shared.id, req: req.clone(), reply: tx }).is_err() {
        return;
    }
    let left = deadline.saturating_duration_since(Instant::now());
    let protocol = match rx.recv_timeout(left) {
        Ok(Reply::Accept(protocol)) => protocol,
        Ok(Reply::Reject(status)) => return reject(&mut s, status),
        Err(_) => return reject(&mut s, 503),
    };
    if s.write_all(&handshake::switching(&parsed.key, protocol.as_deref())).is_err() {
        return;
    }
    pending.done();
    let link = Link::new(crate::next_id(), true, shared.max, shared.events.clone());
    let open = Event::Connection { server: shared.id, conn: Conn(link.clone()), req };
    link.start(s.tcp, s.tls, rest, protocol, open);
}

/// Sends a bodiless error response and closes gracefully.
fn reject(s: &mut Stream, status: u16) {
    if s.write_all(&handshake::reject(status)).is_err() {
        return;
    }
    if let Some(tls) = &mut s.tls {
        tls.send_close_notify();
        let _ = tls.complete_io(&mut s.tcp);
    }
    let _ = s.tcp.shutdown(Shutdown::Write);
    // Drain briefly: closing with unread input can reset the connection
    // before the peer has read the response.
    let until = Instant::now() + Duration::from_secs(1);
    let mut buf = [0u8; 4096];
    let mut total = 0;
    while total < 64 * 1024 {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || s.tcp.set_read_timeout(Some(left)).is_err() {
            break;
        }
        match s.tcp.read(&mut buf) {
            Ok(n) if n > 0 => total += n,
            _ => break,
        }
    }
}
