//! Shared application handles; reader and writer futures use one connection task.
//! Lock order is flags -> queue. No lock is held across an await.
mod budget;
mod close;
mod handle;
mod heartbeat;
mod io;
mod lease;
mod read;
mod send;
mod state;
mod write;

use crate::Id;
use crate::queue::Events;
use crate::signal::Signal;
use io::Io;
use send::Queue;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_COST: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Config {
    /// Maximum assembled inbound message, including fragments.
    pub message: usize,
    /// Encoded outbound bytes; a single larger send is allowed when empty.
    pub buffer: usize,
    /// Quiet-peer probe interval; None disables probes.
    pub heartbeat: Option<Duration>,
}

#[cfg(test)]
impl Config {
    pub fn test(message: usize) -> Self {
        Self { message, buffer: crate::MAX_BUFFER, heartbeat: None }
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
    /// Set before queueing our only close frame.
    sent_close: bool,
    /// The peer's validated close status (1005 for an empty payload).
    received: Option<(u16, String)>,
    /// Our protocol-violation code when the peer never replies.
    failed: Option<u16>,
    /// Absolute bound on the closing handshake.
    deadline: Option<Instant>,
}

pub(crate) struct Link {
    id: Id,
    server: bool,
    max: usize,
    buffer: usize,
    heartbeat: Option<Duration>,
    token: [u8; 8],
    pub(super) events: Events,
    flags: Mutex<Flags>,
    queue: Mutex<Queue>,
    ready: Notify,
    buffered: AtomicUsize,
    inbox: AtomicUsize,
    /// Wakes only the reader whose inbound queue was consumed.
    consumed: Notify,
    /// Wakes the reader waiting for outbound close bytes to flush.
    drained: Notify,
    changed: Notify,
    pub(super) stopped: Signal,
    born: Instant,
    written: Arc<AtomicU64>,
    received: Arc<AtomicU64>,
    io: OnceLock<Io>,
    protocol: OnceLock<Option<String>>,
}

impl Link {
    pub fn new(id: Id, server: bool, config: Config, events: Events) -> Arc<Self> {
        Arc::new(Self {
            id,
            server,
            max: config.message,
            buffer: config.buffer,
            heartbeat: config.heartbeat,
            token: crate::random(),
            events,
            flags: Mutex::new(Flags {
                state: State::Connecting,
                sent_close: false,
                received: None,
                failed: None,
                deadline: None,
            }),
            queue: Mutex::default(),
            ready: Notify::new(),
            buffered: AtomicUsize::new(0),
            inbox: AtomicUsize::new(0),
            consumed: Notify::new(),
            drained: Notify::new(),
            changed: Notify::new(),
            stopped: Signal::default(),
            born: Instant::now(),
            written: Arc::new(AtomicU64::new(0)),
            received: Arc::new(AtomicU64::new(0)),
            io: OnceLock::new(),
            protocol: OnceLock::new(),
        })
    }
}
