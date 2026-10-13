//! Heartbeat: pings a quiet peer and drops one that stays silent.
//!
//! Signs of life are inbound bytes and writes the peer acknowledged. Checks
//! run only after a read found nothing to read, so an application that is
//! slow to take events never makes a healthy peer look dead.

use super::{Link, State};
use crate::proto::frame;
use std::sync::atomic::Ordering::Relaxed;
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

/// The reader's view of the peer.
pub(super) struct Alive {
    pub seen: Instant,
    ping: Option<Instant>,
}

impl Alive {
    pub fn new() -> Self {
        Alive { seen: Instant::now(), ping: None }
    }
}

impl Link {
    /// How long the reader may block, so checks keep up with the interval.
    pub(super) fn tick(&self) -> Duration {
        self.heartbeat.map_or(crate::TICK, |k| (k / 4).clamp(Duration::from_millis(10), crate::TICK))
    }

    /// Runs after a read timed out; `false` once the peer was dropped.
    pub(super) fn heartbeat(&self, alive: &mut Alive) -> bool {
        // Closing has its own deadline.
        let Some(interval) = self.heartbeat.filter(|_| self.state() == State::Open) else {
            return true;
        };
        let now = Instant::now();
        let acked = self.born + Duration::from_millis(self.acked.load(Relaxed));
        match decide(interval, now, alive.seen.max(acked), alive.ping) {
            Action::Wait => true,
            Action::Ping => {
                // Not counted against the send buffer: it is tiny, and only
                // sent while the connection is quiet.
                self.push(self.frame(frame::PING, &self.token));
                alive.ping = Some(now);
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
#[path = "../../../tests/unit/heartbeat.rs"]
mod tests;
