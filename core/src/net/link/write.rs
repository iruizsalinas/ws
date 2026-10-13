//! Outbound frames: sealing, queueing and the writer thread.

use super::Link;
use crate::lock;
use crate::proto::frame;
use rustls::Connection;
use std::collections::VecDeque;
use std::io::Write;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::Ordering::Relaxed;

pub(super) enum Item {
    Data(Vec<u8>),
    Shutdown,
}

#[derive(Default)]
pub(super) struct Queue {
    items: VecDeque<Item>,
    done: bool,
}

impl Link {
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

    pub(super) fn write_loop(&self, mut tcp: TcpStream) {
        loop {
            let items: Vec<Item> = {
                let mut q = lock(&self.queue);
                while q.items.is_empty() && !q.done {
                    q = self.ready.wait(q).unwrap_or_else(|e| e.into_inner());
                }
                if q.done {
                    return;
                }
                q.items.drain(..).collect()
            };
            for item in items {
                match item {
                    Item::Data(d) => {
                        if tcp.write_all(&d).is_err() {
                            return self.end();
                        }
                        self.buffered.fetch_sub(d.len(), Relaxed);
                    }
                    Item::Shutdown => {
                        let _ = tcp.shutdown(Shutdown::Write);
                    }
                }
            }
        }
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
