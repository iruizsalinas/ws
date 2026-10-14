//! Heartbeat: pings a quiet peer and drops one that stays silent.
//!
//! Only inbound traffic answers a transmitted ping. While a ping is queued
//! behind application data, local write progress defers its stall timeout.
//! Checks run only after a read found nothing, so slow application consumption
//! does not make a healthy peer look dead.

use super::{Link, State};
mod clock;
use crate::lock;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Wait,
    Ping,
    Dead,
}

/// What to do after `last` (the latest sign of life), given the ping sent
/// at `ping`, if any: a ping is answered by any sign of life after it.
pub(crate) fn decide(interval: Duration, now: Instant, last: Instant, ping: Option<Instant>) -> Action {
    match ping {
        Some(sent) if last <= sent => {
            if now.saturating_duration_since(sent) >= interval {
                Action::Dead
            } else {
                Action::Wait
            }
        }
        _ if now.saturating_duration_since(last) >= interval => Action::Ping,
        _ => Action::Wait,
    }
}

/// Shared completion time for one heartbeat's socket write.
pub(super) type Sent = Arc<Mutex<Option<Instant>>>;

pub(in crate::net::link) struct Probe {
    pub(in crate::net::link) queued: Instant,
    pub(in crate::net::link) sent: Sent,
}

/// The reader's view of the peer.
pub(super) struct Alive {
    pub seen: Instant,
    pub(in crate::net::link) ping: Option<Probe>,
}

impl Alive {
    pub fn new() -> Self {
        Alive { seen: Instant::now(), ping: None }
    }
}

impl Link {
    /// Runs after a read timed out; `false` once the peer was dropped.
    pub(super) fn heartbeat(&self, alive: &mut Alive) -> bool {
        // Closing has its own deadline.
        let Some(interval) = self.heartbeat.filter(|_| self.state() == State::Open) else {
            return true;
        };
        let now = Instant::now();
        if let Some(ns) = self.received.load(Relaxed).checked_sub(1) {
            alive.seen = alive.seen.max(self.born + Duration::from_nanos(ns));
        }
        let sent = alive.ping.as_ref().and_then(|ping| *lock(&ping.sent));
        if let Some(ping) = &alive.ping
            && sent.is_none()
        {
            // Don't start the response deadline while the ping is still behind
            // a healthy transfer, but don't let a stalled writer wait forever.
            let written = self.born + Duration::from_nanos(self.written.load(Relaxed));
            if now.saturating_duration_since(alive.seen.max(written).max(ping.queued)) < interval {
                return true;
            }
            self.fail("heartbeat timed out");
            return false;
        }
        match decide(interval, now, alive.seen, sent) {
            Action::Wait => true,
            Action::Ping => {
                let f = lock(&self.flags);
                if f.state != State::Open {
                    return true;
                }
                // One tiny reserved control frame, even when the send buffer is
                // full. The writer starts its response clock after transmission.
                let sent = Arc::new(Mutex::new(None));
                let queued = Instant::now();
                self.push_heartbeat(sent.clone());
                alive.ping = Some(Probe { queued, sent });
                true
            }
            Action::Dead => {
                self.fail("heartbeat timed out");
                false
            }
        }
    }

    /// Whether `data` answers one of our heartbeat pings.
    pub(super) fn ours(&self, data: &[u8]) -> bool {
        data == self.token
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/unit/heartbeat.rs"]
mod tests;
