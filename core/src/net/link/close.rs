//! Closing: the handshake, aborts and the single final event.

use super::{CLOSE_TIMEOUT, Link, State};
use crate::proto::frame;
use crate::{Event, lock};
use std::net::Shutdown;
use std::time::Instant;

impl Link {
    /// Starts the closing handshake, or aborts a pending connection.
    pub fn close(&self, code: u16, reason: &str) {
        let mut f = lock(&self.flags);
        match f.state {
            State::Connecting => {
                drop(f);
                self.fail("connection closed before it was established");
            }
            State::Open => {
                f.sent_close = true;
                f.state = State::Closing;
                f.deadline = Some(Instant::now() + CLOSE_TIMEOUT);
                self.push(self.frame(frame::CLOSE, &frame::close_payload(code, reason)));
            }
            _ => {}
        }
    }

    /// Drops the connection immediately.
    pub fn terminate(&self) {
        self.finish(1006, String::new());
    }

    pub(super) fn finish(&self, code: u16, reason: String) {
        {
            let mut f = lock(&self.flags);
            if f.state == State::Closed {
                return;
            }
            f.state = State::Closed;
            let _ = self.events.send(Event::Close { id: self.id, code, reason });
        }
        self.stop_writer();
        self.drained.notify_all();
        if let Some(io) = self.io.get() {
            let _ = io.tcp.shutdown(Shutdown::Both);
        }
    }

    /// Ends with the peer's close code, ours after a violation, or 1006.
    pub(super) fn end(&self) {
        let (code, reason) = {
            let f = lock(&self.flags);
            match (&f.received, f.failed) {
                (Some((code, reason)), _) => (*code, reason.clone()),
                (None, Some(code)) => (code, String::new()),
                (None, None) => (1006, String::new()),
            }
        };
        self.finish(code, reason);
    }
}
