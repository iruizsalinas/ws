//! Connecting to the first reachable address.

use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

/// Delay before trying the next resolved address.
const STAGGER: Duration = Duration::from_millis(250);

/// Happy Eyeballs (RFC 8305): each address gets a head start before the
/// next attempt begins, and the first connection to succeed wins.
pub(super) fn dial(addrs: Vec<SocketAddr>, deadline: Instant) -> Result<TcpStream, Option<String>> {
    let (tx, rx) = channel();
    let mut error = None;
    let mut pending = 0;
    let mut settle = |r: Result<TcpStream, String>, pending: &mut usize| {
        *pending -= 1;
        r.map_err(|e| error = Some(e)).ok()
    };
    for addr in addrs {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let tx = tx.clone();
        let attempt = move || {
            let r = TcpStream::connect_timeout(&addr, left).map_err(|e| format!("cannot connect to {addr}: {e}"));
            let _ = tx.send(r);
        };
        if std::thread::Builder::new().name("ws-dial".into()).spawn(attempt).is_err() {
            continue;
        }
        pending += 1;
        if let Ok(r) = rx.recv_timeout(STAGGER)
            && let Some(s) = settle(r, &mut pending)
        {
            return Ok(s);
        }
    }
    while pending > 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(r) => {
                if let Some(s) = settle(r, &mut pending) {
                    return Ok(s);
                }
            }
            Err(_) => return Err(Some("handshake timed out".into())),
        }
    }
    Err(error)
}
