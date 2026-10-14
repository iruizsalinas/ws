//! WebSocket (RFC 6455) client and server on shared asynchronous I/O and rustls.
//!
//! Connections share a bounded runtime and report to a [`Hub`], which the
//! embedding event loop drains on its owning thread.

#![forbid(unsafe_code)]

mod event;
mod hub;
mod net;
pub mod proto;
mod queue;
mod reply;
mod signal;
pub mod tls;

pub use event::{Event, Reply, Request};
pub use hub::{Conn, Hub};
pub use net::client::Options as ClientOptions;
pub use net::link::State;
pub use net::server::{Options as ServerOptions, Server};
pub use proto::http::Headers;
pub use reply::ReplySender;

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// Identifies a connection or server; unique within the process.
pub type Id = u64;

/// Default limit for a single message, in bytes.
pub const MAX_MESSAGE: usize = 1 << 20;
/// Default limit for unsent data per connection, in bytes.
pub const MAX_BUFFER: usize = 16 << 20;
/// Default silence before a heartbeat ping; as much again without a reply drops the peer.
pub const HEARTBEAT: Duration = Duration::from_secs(30);

pub(crate) fn next_id() -> Id {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Relaxed)
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn random<const N: usize>() -> [u8; N] {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; N];
    SystemRandom::new().fill(&mut bytes).expect("system random generator failed");
    bytes
}
