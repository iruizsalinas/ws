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

#[derive(Default)]
pub(super) struct Queue {
    pub(super) items: VecDeque<Item>,
    pub(super) done: bool,
    /// A send was refused for a full buffer; report when there is room.
    pub(super) drain: bool,
}

impl Link {
    /// Sends a data or ping frame; fails unless the connection is open.
    pub fn send(&self, op: u8, payload: &[u8]) -> Result<(), &'static str> {
        let f = lock(&self.flags);
        if f.state != State::Open || f.sent_close {
            return Err(if f.state == State::Connecting { "not open yet" } else { "closed" });
        }
        // Checked under flags, which serializes application sends. A single
        // large frame is allowed to cross the threshold; subsequent sends
        // must wait for the writer, including its currently in-flight batch.
        if self.buffered.load(Relaxed) >= self.buffer {
            // Rechecked under the queue lock the writer reports progress
            // under, so it either sees the request or has already made room.
            let mut q = lock(&self.queue);
            if self.buffered.load(Relaxed) >= self.buffer {
                q.drain = true;
                return Err("send buffer is full");
            }
        }
        self.push(self.frame(op, payload));
        Ok(())
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
        if let Item::Data(d) = &item {
            if d.is_empty() {
                return;
            }
            self.buffered.fetch_add(d.len(), Relaxed);
        }
        q.items.push_back(item);
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
