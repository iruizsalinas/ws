//! Sleep until the next real heartbeat deadline, with no per-connection polling.
use super::{Alive, Link};
use crate::lock;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

impl Link {
    pub(in crate::net::link) fn heartbeat_deadline(&self, alive: &Alive) -> Option<Instant> {
        let interval = self.heartbeat?;
        let seen = self
            .received
            .load(Relaxed)
            .checked_sub(1)
            .map_or(alive.seen, |ns| alive.seen.max(self.born + Duration::from_nanos(ns)));
        let last = match &alive.ping {
            Some(ping) => match *lock(&ping.sent) {
                Some(sent) if seen <= sent => sent,
                Some(_) => seen,
                None => {
                    let written = self.born + Duration::from_nanos(self.written.load(Relaxed));
                    seen.max(written).max(ping.queued)
                }
            },
            None => seen,
        };
        last.checked_add(interval)
    }
}
