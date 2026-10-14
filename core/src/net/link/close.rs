//! Closing: the handshake, aborts and the single final event.

use super::{CLOSE_TIMEOUT, Link, State};
use crate::proto::frame;
use crate::{Event, lock};
use std::sync::atomic::Ordering::Relaxed;
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
                self.push_frame(frame::CLOSE, &frame::close_payload(code, reason));
            }
            _ => {}
        }
        self.changed.notify_one();
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
        self.stopped.set();
        self.drained.notify_one();
        if let Some(io) = self.io.get() {
            io.shutdown();
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

    /// EOF only closes the peer's sending direction. Give our close reply time
    /// to leave the writer before finish() drops the queue and shuts down TCP.
    pub(super) async fn flush_close(&self) {
        let deadline = {
            let f = lock(&self.flags);
            if f.received.is_none() && f.failed.is_none() {
                return;
            }
            f.deadline
        };
        let Some(deadline) = deadline else { return };
        let stopped = self.stopped.wait();
        let timeout = tokio::time::sleep_until(deadline.into());
        tokio::pin!(stopped, timeout);
        loop {
            let drained = self.drained.notified();
            let pending = {
                let q = lock(&self.queue);
                !q.done && (self.buffered.load(Relaxed) > 0 || q.shutdown)
            };
            if !pending {
                return;
            }
            tokio::select! {
                _ = &mut stopped => return,
                _ = drained => {}
                _ = &mut timeout => return,
            }
        }
    }
}
