//! Reacting to decoded messages: delivery, pongs and the closing handshake.

use super::{CLOSE_TIMEOUT, EVENT_COST, Link};
use crate::proto::frame::{self, Fail, Msg};
use crate::proto::reader::Reader;
use crate::{Event, lock};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

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
                    if closing || self.state() == super::State::Closed {
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
        if f.state == super::State::Closed {
            return;
        }
        f.failed = Some(code);
        f.state = super::State::Closing;
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
                if f.state != super::State::Closed && f.received.is_none() && self.buffered.load(Relaxed) < PONG_LIMIT {
                    self.push(self.frame(frame::PONG, &data));
                }
                drop(f);
                self.deliver(data.len(), Event::Ping { id, data });
            }
            Msg::Pong(data) => self.deliver(data.len(), Event::Pong { id, data }),
            Msg::Close(status) => self.closing(status),
        }
    }

    fn closing(&self, status: Option<(u16, String)>) {
        let mut f = lock(&self.flags);
        if f.state == super::State::Closed || f.received.is_some() {
            return;
        }
        f.received = Some(status.clone().unwrap_or((1005, String::new())));
        f.state = super::State::Closing;
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

    fn deliver(&self, bytes: usize, event: Event) {
        self.inbox.fetch_add(bytes + EVENT_COST, Relaxed);
        self.emit(event);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/handle.rs"]
mod tests;
