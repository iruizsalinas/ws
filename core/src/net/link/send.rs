//! Outbound frames: the send API, sealing and the writer's queue.

use super::{Link, State};
use crate::lock;
use crate::proto::frame;
use rustls::Connection;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::Ordering::Relaxed;

pub(super) enum Item {
    Data(Vec<u8>),
    Shutdown,
}

/// Writes smaller than this join the previous one, so tiny frames cost
/// their bytes rather than a queue entry and an allocation each.
const JOIN: usize = 16 * 1024;

#[derive(Default)]
pub(super) struct Queue {
    pub(super) items: VecDeque<Item>,
    pub(super) done: bool,
    /// Size of a send refused for a full buffer; reported once it fits.
    pub(super) drain: Option<usize>,
}

impl Link {
    /// Sends a data or ping frame; fails unless the connection is open.
    pub fn send(&self, op: u8, payload: &[u8]) -> Result<(), &'static str> {
        let f = lock(&self.flags);
        if f.state != State::Open || f.sent_close {
            return Err(if f.state == State::Connecting { "not open yet" } else { "closed" });
        }
        // Checked under flags, which serializes application sends, and
        // before the frame is built. The writer's in-flight batch counts.
        let len = frame::encoded_len(payload.len(), !self.server);
        if !self.fits(len) {
            // Rechecked under the queue lock the writer reports progress
            // under, so it either sees the request or has already made room.
            let mut q = lock(&self.queue);
            if !self.fits(len) {
                q.drain = Some(len);
                return Err("send buffer is full");
            }
        }
        self.push(self.frame(op, payload));
        Ok(())
    }

    /// Whether a frame of `len` bytes may be queued now: it must fit in the
    /// room left, so a larger one is only taken once nothing is waiting.
    pub(super) fn fits(&self, len: usize) -> bool {
        let buffered = self.buffered.load(Relaxed);
        buffered == 0 || buffered.saturating_add(len) <= self.buffer
    }

    pub(super) fn frame(&self, op: u8, payload: &[u8]) -> Vec<u8> {
        // Clients mask every frame with a fresh unpredictable key.
        let mask = (!self.server).then(crate::random::<4>);
        frame::encode(op, payload, mask)
    }

    pub(super) fn push(&self, data: Vec<u8>) {
        let Some(io) = self.io.get() else { return };
        let mut tls = io.tls.as_ref().map(lock);
        let data = match tls.as_deref_mut() {
            Some(conn) => seal(conn, &data),
            None => data,
        };
        self.enqueue(Item::Data(data));
        drop(tls);
    }

    /// Queues a TCP half-close after everything already queued.
    pub(super) fn push_shutdown(&self) {
        let Some(io) = self.io.get() else { return };
        let mut tls = io.tls.as_ref().map(lock);
        if let Some(conn) = tls.as_deref_mut() {
            conn.send_close_notify();
            self.enqueue(Item::Data(seal(conn, &[])));
        }
        self.enqueue(Item::Shutdown);
        drop(tls);
    }

    pub(super) fn enqueue(&self, item: Item) {
        let mut q = lock(&self.queue);
        if q.done {
            return;
        }
        match item {
            Item::Data(d) if d.is_empty() => return,
            Item::Data(d) => {
                self.buffered.fetch_add(d.len(), Relaxed);
                match q.items.back_mut() {
                    Some(Item::Data(last)) if d.len() < JOIN && last.len() < JOIN => last.extend_from_slice(&d),
                    _ => q.items.push_back(Item::Data(d)),
                }
            }
            Item::Shutdown => q.items.push_back(Item::Shutdown),
        }
        drop(q);
        self.ready.notify_one();
    }

    pub(super) fn stop_writer(&self) {
        let mut q = lock(&self.queue);
        q.done = true;
        q.items.clear();
        drop(q);
        self.ready.notify_all();
    }
}

/// Encrypts `plain` and drains every pending TLS record.
pub(super) fn seal(conn: &mut Connection, plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plain.len() + 64);
    if !plain.is_empty() {
        let _ = conn.writer().write_all(plain);
    }
    while conn.wants_write() {
        if conn.write_tls(&mut out).is_err() {
            break;
        }
    }
    out
}
