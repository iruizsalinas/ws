//! Reacting to decoded messages: delivery, pongs and the closing handshake.

use super::{CLOSE_TIMEOUT, EVENT_COST, Link, State};
use crate::proto::frame::{self, Fail, Msg};
use crate::proto::reader::Reader;
use crate::{Event, TICK, lock};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

/// Undelivered inbound bytes before the reader waits for the application.
pub(super) const INBOX_LIMIT: usize = 1 << 20;
/// Pongs stop being queued past this much unsent data.
const PONG_LIMIT: usize = 1 << 20;

impl Link {
    /// Handles decoded messages; `false` once no more frames may be processed.
    pub(super) fn dispatch(&self, reader: &mut Reader) -> bool {
        loop {
            match reader.message() {
                Ok(None) => return true,
                Ok(Some(msg)) => {
                    let closing = matches!(msg, Msg::Close(_));
                    self.handle(msg);
                    if closing || self.state() == State::Closed {
                        return false;
                    }
                }
                Err(fail) => {
                    self.violation(fail);
                    return false;
                }
            }
        }
    }

    /// Fails the connection: close with the code, then drop TCP.
    fn violation(&self, Fail(code, why): Fail) {
        self.emit(Event::Error { id: self.id, error: why.to_string() });
        let mut f = lock(&self.flags);
        if f.state == State::Closed {
            return;
        }
        f.failed = Some(code);
        f.state = State::Closing;
        f.deadline.get_or_insert(Instant::now() + CLOSE_TIMEOUT);
        if !f.sent_close {
            f.sent_close = true;
            self.push(self.frame(frame::CLOSE, &frame::close_payload(code, "")));
        }
        self.push_shutdown();
    }

    fn handle(&self, msg: Msg) {
        let id = self.id;
        match msg {
            Msg::Text(data) | Msg::Binary(data) if lock(&self.flags).received.is_some() => drop(data),
            Msg::Text(data) => self.deliver(data.len(), Event::Message { id, data, binary: false }),
            Msg::Binary(data) => self.deliver(data.len(), Event::Message { id, data, binary: true }),
            Msg::Ping(data) => {
                let f = lock(&self.flags);
                if f.state != State::Closed && f.received.is_none() && self.buffered.load(Relaxed) < PONG_LIMIT {
                    self.push(self.frame(frame::PONG, &data));
                }
                drop(f);
                self.deliver(data.len(), Event::Ping { id, data });
            }
            // Answers to keepalive pings are internal.
            Msg::Pong(data) if self.ours(&data) => {}
            Msg::Pong(data) => self.deliver(data.len(), Event::Pong { id, data }),
            Msg::Close(status) => self.closing(status),
        }
    }

    fn closing(&self, status: Option<(u16, String)>) {
        let mut f = lock(&self.flags);
        if f.state == State::Closed || f.received.is_some() {
            return;
        }
        f.received = Some(status.clone().unwrap_or((1005, String::new())));
        f.state = State::Closing;
        f.deadline.get_or_insert(Instant::now() + CLOSE_TIMEOUT);
        if !f.sent_close {
            f.sent_close = true;
            let echo = status.map(|(code, _)| frame::close_payload(code, "")).unwrap_or_default();
            self.push(self.frame(frame::CLOSE, &echo));
        }
        // The server closes the TCP connection first (RFC 6455 section 7.1.1).
        if self.server {
            self.push_shutdown();
        }
    }

    /// Called once the application has taken an inbound event.
    pub fn consumed(&self, bytes: usize) {
        let _q = lock(&self.queue);
        self.inbox.fetch_sub(bytes + EVENT_COST, Relaxed);
        self.drained.notify_one();
    }

    /// Backpressure: waits (one tick at most) while the application lags.
    pub(super) fn wait_inbox(&self) -> bool {
        // Keep reading the closing handshake even if the application stopped
        // consuming events. Delivery stays bounded while closing.
        if self.state() != State::Open {
            return true;
        }
        if self.inbox.load(Relaxed) < INBOX_LIMIT {
            return true;
        }
        let q = lock(&self.queue);
        if self.inbox.load(Relaxed) >= INBOX_LIMIT {
            let _ = self.drained.wait_timeout(q, TICK);
        }
        self.inbox.load(Relaxed) < INBOX_LIMIT
    }

    fn deliver(&self, bytes: usize, event: Event) {
        if self.inbox.load(Relaxed) >= INBOX_LIMIT && self.state() != State::Open {
            return;
        }
        self.inbox.fetch_add(bytes + EVENT_COST, Relaxed);
        self.emit(event);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/handle.rs"]
mod tests;
