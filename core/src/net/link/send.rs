//! Bounded outbound WebSocket bytes. TLS work belongs to the transport future.
use super::heartbeat::Sent;
use super::{Link, State};
use crate::{lock, proto::frame};
use std::collections::VecDeque;
use std::sync::atomic::Ordering::Relaxed;

pub(super) enum Item {
    Data(Vec<u8>),
    Heartbeat(Vec<u8>, Sent),
    Shutdown,
}
const JOIN: usize = 16 * 1024;

#[derive(Default)]
pub(super) struct Queue {
    pub items: VecDeque<Item>,
    pub done: bool,
    pub shutdown: bool,
    pub drain: Option<usize>,
    /// At most one unanswered pong waits under congestion.
    pub pong: Option<Vec<u8>>,
}

impl Link {
    pub fn send(&self, op: u8, payload: &[u8]) -> Result<(), &'static str> {
        let f = lock(&self.flags);
        if f.state != State::Open || f.sent_close {
            return Err(if f.state == State::Connecting { "not open yet" } else { "closed" });
        }
        let len = frame::encoded_len(payload.len(), !self.server);
        if !self.fits(len) {
            let mut q = lock(&self.queue);
            // Register the drain request under the same lock as progress(),
            // then recheck: otherwise a concurrent drain could be missed.
            if !self.fits(len) {
                q.drain = Some(len);
                return Err("send buffer is full");
            }
        }
        self.push(self.frame(op, payload));
        Ok(())
    }

    pub(super) fn fits(&self, len: usize) -> bool {
        let buffered = self.buffered.load(Relaxed);
        buffered == 0 || buffered.saturating_add(len) <= self.buffer
    }

    pub(super) fn frame(&self, op: u8, payload: &[u8]) -> Vec<u8> {
        frame::encode(op, payload, (!self.server).then(crate::random::<4>))
    }

    pub(super) fn push(&self, data: Vec<u8>) {
        if self.io.get().is_some() {
            self.enqueue(Item::Data(data));
        }
    }

    pub(super) fn push_heartbeat(&self, sent: Sent) {
        if self.io.get().is_some() {
            self.enqueue(Item::Heartbeat(self.frame(frame::PING, &self.token), sent));
        }
    }

    pub(super) fn flush_pong(&self) {
        let f = lock(&self.flags);
        let pong = lock(&self.queue).pong.take();
        if f.state != State::Closed
            && f.received.is_none()
            && f.failed.is_none()
            && let Some(data) = pong
        {
            self.push(self.frame(frame::PONG, &data));
        }
    }

    pub(super) fn push_shutdown(&self) {
        if self.io.get().is_some() {
            self.enqueue(Item::Shutdown);
        }
    }

    pub(super) fn enqueue(&self, item: Item) {
        let mut q = lock(&self.queue);
        if q.done {
            return;
        }
        match item {
            Item::Data(data) if data.is_empty() => return,
            Item::Data(data) => {
                self.buffered.fetch_add(data.len(), Relaxed);
                match q.items.back_mut() {
                    Some(Item::Data(last)) if data.len() < JOIN && last.len() < JOIN => last.extend_from_slice(&data),
                    _ => q.items.push_back(Item::Data(data)),
                }
            }
            Item::Heartbeat(data, sent) => {
                self.buffered.fetch_add(data.len(), Relaxed);
                q.items.push_back(Item::Heartbeat(data, sent));
            }
            Item::Shutdown => {
                q.pong = None;
                q.shutdown = true;
                q.items.push_back(Item::Shutdown);
            }
        }
        drop(q);
        self.ready.notify_one();
    }

    pub(super) fn stop_writer(&self) {
        let mut q = lock(&self.queue);
        q.done = true;
        q.items.clear();
        q.pong = None;
        q.shutdown = false;
        drop(q);
        self.ready.notify_one();
    }
}
